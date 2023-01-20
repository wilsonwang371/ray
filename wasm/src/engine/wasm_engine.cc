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
#include <engine/wasm_engine.h>
// clang-format on

#include <engine/utils.h>
#include <ray/api/function_manager.h>
#include <ray/api/overload.h>

#include <fstream>

#include "ray/util/logging.h"

using namespace std;

namespace wasm_engine {

static uint8_t *read_file(const string &file_name, size_t *length_ptr) {
  // read all bytes from wasm file
  ifstream infile(file_name, ios::binary | ios::ate);
  if (!infile.is_open()) {
    cerr << "failed to open wasm file" << endl;
    return 0;
  }

  // get length of file
  infile.seekg(0, std::ios::end);
  size_t length = infile.tellg();
  infile.seekg(0, std::ios::beg);

  // allocate memory:
  uint8_t *buffer = new uint8_t[length];
  infile.read((char *)buffer, length);
  infile.close();

  *length_ptr = length;
  return buffer;
}

// compile wasm file to wasm module
optional<WasmModule> compile_wasm_file(WasmEngine &engine, const string &filename) {
  size_t length;
  uint8_t *code = read_file(filename, &length);
  if (code == NULL) {
    RAY_LOG(DEBUG) << "failed to read wasm file";
    return nullopt;
  }

  auto module = compile_wasm_bytes(engine, code, length);
  delete[] code;
  return module;
}

WasmModule compile_wasm_bytes(WasmEngine &engine, uint8_t *code, size_t length) {
  auto data = Span<uint8_t>(reinterpret_cast<uint8_t *>(code), length);
  return unwrap(WasmModule::compile(engine, data));
}

optional<WasmInstance> init_wasm_module(WasmLinker &linker,
                                        WasmStore &store,
                                        WasmModule &module) {
  RAY_LOG(DEBUG) << "start to instantiate wasm module";
  auto result = linker.instantiate(store, module);
  if (!result) {
    RAY_LOG(ERROR) << "failed to instantiate wasm module";
    return nullopt;
  }
  if (result) {
    return optional<WasmInstance>(result.ok());
  }
  RAY_LOG(ERROR) << "failed to instantiate wasm module";
  return nullopt;
}

static void register_ray_handlers(WasmLinker &linker) {
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

        // print all parameters
        auto func_type = func->type(caller.context());
        auto params = func_type->params();
        auto results = func_type->results();
#ifdef RAYWA_DEBUG
        cerr << "function: " << funcref_idx << " params: " << params.size()
             << " results: " << results.size() << endl;
#endif
        // calculate bytes used by parameters
        size_t params_bytes = 0;
        for (auto &param : params) {
#ifdef RAYWA_DEBUG
          cerr << "param: " << param.kind() << endl;
#endif
          switch (param.kind()) {
          case WasmValueType::I32:
            params_bytes += sizeof(int32_t);
            break;
          case WasmValueType::I64:
            params_bytes += sizeof(int64_t);
            break;
          case WasmValueType::F32:
            params_bytes += sizeof(float);
            break;
          case WasmValueType::F64:
            params_bytes += sizeof(double);
            break;
          default:
            cerr << "unsupported parameter type: " << param.kind() << endl;
            return monostate();
          }
        }

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

        // make sure the parameter size is within the memory range
        if (memory.data_size(caller.context()) < args + params_bytes) {
          cerr << "data address out of range: 0x" << hex << args << " + " << params_bytes
               << endl;
          return monostate();
        }

#ifdef RAYWA_DEBUG
        cerr << "memory size: 0x" << hex << memory.data_size(caller.context()) << endl;
        print_hex(memory.data(caller.context()).data(), args, params_bytes);
#endif

#ifdef RAYWA_DEBUG
        cerr << "create remote function: 0x" << hex << funcref_idx << " args 0x" << args
             << endl;
#endif
        int (*fp)() = (int (*)())(0L + funcref_idx);  // TODO fix this hack
        ray::internal::RegisterRemoteFunctions(to_string(funcref_idx), fp);
        // auto object = ray::Task(fp).Remote();  // TODO args
        // auto result = ray::Get(object);
        ray::Task(fp).Remote();
        // sleep for 5 second to wait for the result
        sleep(5);

        return monostate();
      }));
}

/* init wasm host environment related stuff */
void init_host_env(WasmLinker &linker, WasmStore &store) {
  WasiConfig wasi;
  wasi.inherit_argv();
  wasi.inherit_env();
  wasi.inherit_stdin();
  wasi.inherit_stdout();
  wasi.inherit_stderr();
  store.context().set_wasi(std::move(wasi)).unwrap();

  unwrap(linker.define_wasi());
  register_ray_handlers(linker);
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
