// Copyright 2023 The Ray Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
#include <ray/api.h>
// clang-format off
#include "ray/core_worker/context.h"
#include "engine/wasm_engine.h"
// clang-format on

#include <ray/api/function_manager.h>
#include <ray/api/overload.h>

using namespace std;

namespace wasm_engine {

WasmModule compile_wasm_module(WasmEngine &engine, uint8_t *code, size_t length) {
  auto data = Span<uint8_t>(reinterpret_cast<uint8_t *>(code), length);
  return unwrap(WasmModule::compile(engine, data));
}

WasmInstance init_wasm_module(WasmLinker &linker, WasmStore &store, WasmModule &module) {
  return unwrap(linker.instantiate(store, module));
}

void register_ray_handlers(WasmLinker &linker) {
  unwrap(linker.func_new(
      "ray", "get", WasmFunctionType({WasmValueType::I32}, {}), [
      ](auto caller, auto params, auto results) -> auto{ return monostate(); }));

  unwrap(linker.func_new(
      "ray", "put", WasmFunctionType({WasmValueType::I32}, {}), [
      ](auto caller, auto params, auto results) -> auto{ return monostate(); }));

  unwrap(linker.func_wrap(
      "ray", "call", [](WasmCaller caller, int32_t funcref_idx, int32_t args) -> auto{
        auto tbl = caller.get_export(WASMFUNC_TBL_NAME);
        if (!holds_alternative<WasmTable>(*tbl)) {
          cerr << "cannot get function table: " << WASMFUNC_TBL_NAME << endl;
          return monostate();
        }

        auto table = get<WasmTable>(*tbl);
        auto val = table.get(caller.context(), funcref_idx);
        if (val->kind() != ValKind::FuncRef) {
          cerr << "cannot get function ref: " << funcref_idx << endl;
          return monostate();
        }

        optional<WasmFunction> func = val->funcref();
        if (!func) {
          cerr << "cannot get function: " << funcref_idx << endl;
          return monostate();
        }
#ifdef RAYWA_DEBUG
        cerr << "create remote function: 0x" << hex << funcref_idx << " args 0x" << args
             << endl;
#endif
        ray::internal::RegisterRemoteFunctions(to_string(funcref_idx),
                                               (void (*)())(0L + funcref_idx));
        optional<Extern> export_memory = caller.get_export("memory");
        if (!export_memory) {
          cerr << "cannot get memory" << endl;
          return monostate();
        }
        if (!holds_alternative<WasmMemory>(*export_memory)) {
          cerr << "memory not found" << endl;
          return monostate();
        }

        auto memory = get<WasmMemory>(*caller.get_export("memory"));
        if (memory.data_size(caller.context()) <= args) {
          cerr << "data address out of range: 0x" << hex << args << endl;
          return monostate();
        }

#ifdef RAYWA_DEBUG
        cerr << "memory size: 0x" << hex << memory.data_size(caller.context()) << endl;
#endif

        return monostate();
      }));
}

/* get raw pointer of function */
size_t function_raw_pointer(WasmStore &store, WasmFunction &func) {
  size_t raw = wasmtime_func_to_raw(store.context().raw_context(), &func.raw_func());
  return raw;
}

/* get function from exports by function name */
optional<WasmFunction> function_from_exports(WasmInstance &instance,
                                             WasmStore &store,
                                             const char *func_name) {
  optional<Extern> export_func;
  if (!(export_func = instance.get(store, func_name))) {
    cerr << "cannot get function: " << func_name << endl;
    return nullopt;
  }

  // check variant type
  if (!holds_alternative<WasmFunction>(*export_func)) {
    cerr << "function " << func_name << " not found" << endl;
    return nullopt;
  }

  auto func = get<WasmFunction>(*export_func);
#ifdef RAYWA_DEBUG
  cerr << "found function: \"" << func_name << "\"" << endl;
#endif
  return optional<WasmFunction>(func);
}

/* get table from exports by table name */
optional<WasmTable> table_from_exports(WasmInstance &instance,
                                       WasmStore &store,
                                       const char *tbl_name) {
  optional<Extern> export_tbl;
  if (!(export_tbl = instance.get(store, tbl_name))) {
    cerr << "cannot get table: " << tbl_name << endl;
    return nullopt;
  }

  // check variant type
  if (!holds_alternative<WasmTable>(*export_tbl)) {
    cerr << "table " << tbl_name << " not found" << endl;
    return nullopt;
  }

  auto table = get<WasmTable>(*export_tbl);
#ifdef RAYWA_DEBUG
  cerr << "table \"" << tbl_name << "\" size = " << table.size(store) << endl;
#endif
  return optional<WasmTable>(table);
}

/* get function pointer from exports by item index */
optional<WasmFunction> function_from_exports(WasmInstance &instance,
                                             WasmStore &store,
                                             int index) {
  auto tmp = instance.get(store, index);
  if (!tmp) {
    cerr << "cannot get export " << index << endl;
    return nullopt;
  }
  auto name = get<string_view>(*tmp);
  auto ext = get<Extern>(*tmp);
  if (!holds_alternative<WasmFunction>(ext)) {
    cerr << "export " << index << " is not a function" << endl;
    return nullopt;
  }
  auto func = get<WasmFunction>(ext);

#ifdef RAYWA_DEBUG
  cerr << "found function " << name << " at export index " << index
       << ". store_id: " << func.raw_func().store_id
       << " func index: " << func.raw_func().index << endl;
#endif
  return func;
}

/* get function pointer from table using table class */
optional<WasmFunction> function_from_table(WasmInstance &instance,
                                           WasmStore &store,
                                           WasmTable &table,
                                           int index) {
  optional<WasmValue> val;
  if (!(val = table.get(store, index))) {
    cerr << "cannot get function at index " << index << endl;
    return nullopt;
  }

  if (val->kind() != ValKind::FuncRef) {
    cerr << "value at index " << index << " is not function" << endl;
    return nullopt;
  }

  optional<WasmFunction> func = val->funcref();
  if (!func) {
    cout << "function at index " << index << " not found" << endl;
    return nullopt;
  }

  return func;
}

/* get function pointer from table using table name */
optional<WasmFunction> function_from_table(WasmInstance &instance,
                                           WasmStore &store,
                                           const char *table_name,
                                           int index) {
  auto table = table_from_exports(instance, store, table_name);
  if (!table) {
    cerr << "cannot get table: " << table_name << endl;
    return nullopt;
  }

  return function_from_table(instance, store, *table, index);
}

/* call function by name */
void call_function_by_name(WasmInstance &instance,
                           WasmStore &store,
                           const char *func_name,
                           vector<WasmValue> args) {
  auto func = function_from_exports(instance, store, func_name);
  if (!func) {
    cerr << "function " << func_name << " not found" << endl;
    return;
  }

  auto raw = function_raw_pointer(store, *func);
#ifdef RAYWA_DEBUG
  cerr << "calling function \"" << func_name << "\" ";
  cerr << "raw pointer: 0x" << hex << raw << endl;
#endif

  func->call(store, args).unwrap();
}

// TODO: refer to https://github.com/Mossaka/wasi-callback/blob/main/src/main.rs for a way
// to register a callback

}  // namespace wasm_engine
