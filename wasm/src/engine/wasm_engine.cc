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
#include "stdio.h"

using namespace std;

static map<uint32_t, string> objref_map;

namespace wasm_engine {

static uint8_t *read_file(const string &file_name, size_t *length_ptr) {
  // read all bytes from wasm file
  ifstream infile(file_name, ios::binary | ios::ate);
  if (!infile.is_open()) {
    RAY_LOG(DEBUG) << "failed to open wasm file";
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

template <typename T>
uint32_t launch_remote(ValType::ListRef params_fmt,
                       T (*fp)(),
                       int32_t funcref_idx,
                       int32_t args,
                       WasmMemory &memory,
                       WasmCaller &caller) {
  ray::internal::RegisterRemoteFunctions(to_string(funcref_idx), fp);
  auto task = ray::Task(fp);
  auto offset = args;
  for (auto &param : params_fmt) {
    switch (param.kind()) {
    case WasmValueType::I32: {
      int32_t val = 0;
      memcpy(&val, memory.data(caller.context()).data() + offset, sizeof(int32_t));
      offset += sizeof(int32_t);
      RAY_LOG(DEBUG) << "push arg: " << val;
      task.PushOneArgument(val);
    } break;
    case WasmValueType::I64: {
      int64_t val = 0;
      memcpy(&val, memory.data(caller.context()).data() + offset, sizeof(int64_t));
      offset += sizeof(int64_t);
      RAY_LOG(DEBUG) << "push arg: " << val;
      task.PushOneArgument(val);
    } break;
    case WasmValueType::F32: {
      float val;
      memcpy(&val, memory.data(caller.context()).data() + offset, sizeof(float));
      offset += sizeof(float);
      RAY_LOG(DEBUG) << "push arg: " << val;
      task.PushOneArgument(val);
    } break;
    case WasmValueType::F64: {
      double val;
      memcpy(&val, memory.data(caller.context()).data() + offset, sizeof(double));
      offset += sizeof(double);
      RAY_LOG(DEBUG) << "push arg: " << val;
      task.PushOneArgument(val);
    } break;
    default:
      RAY_LOG(DEBUG) << "unsupported parameter type: " << param.kind();
      return 0;
    }
  }

  auto result = task.Remote();
  // TODO(wilson.wang): we need to deal with async/sync call cases

  // TODO(wilson.wang): we need to deal with multiple results

  // get a random int32
  srand(time(NULL));
  uint32_t resid = rand();

  objref_map[resid] = result.ID();
  char tmp[256];
  tmp[0] = '\0';
  for (uint8_t b : result.ID()) {
    snprintf(tmp, 256, "%s%02x", tmp, b);
  }
  RAY_LOG(DEBUG) << "objref_map[" << resid << "] = " << tmp;
  {
    // temporary code to get result
    int32_t resval = *(ray::Get(result));
    RAY_LOG(DEBUG) << "remote call result: " << resval;
  }
  return resid;
}

static void register_ray_handlers(WasmLinker &linker) {
  unwrap(linker.func_new(
      "ray", "get", WasmFunctionType({WasmValueType::I32}, {}), [
      ](auto caller, auto params, auto results) -> auto{ return monostate(); }));

  unwrap(linker.func_new(
      "ray", "put", WasmFunctionType({WasmValueType::I32}, {}), [
      ](auto caller, auto params, auto results) -> auto{ return monostate(); }));

  unwrap(linker.func_wrap(
      "ray",
      "call",
      [](WasmCaller caller, int32_t funcref_idx, int32_t args) -> uint32_t {
        auto tbl = caller.get_export(WASMFUNC_TBL_NAME);
        if (!holds_alternative<WasmTable>(*tbl)) {
          RAY_LOG(ERROR) << "cannot get function table: " << WASMFUNC_TBL_NAME;
          return 0;
        }

        auto table = get<WasmTable>(*tbl);
        auto val = table.get(caller.context(), funcref_idx);
        if (val->kind() != ValKind::FuncRef) {
          RAY_LOG(ERROR) << "cannot get function ref: " << funcref_idx;
          return 0;
        }

        optional<WasmFunction> func = val->funcref();
        if (!func) {
          RAY_LOG(ERROR) << "cannot get function: " << funcref_idx;
          return 0;
        }

        // print all parameters
        auto func_type = func->type(caller.context());
        auto params_fmt = func_type->params();
        auto results_fmt = func_type->results();
        if (results_fmt.size() > 1) {
          RAY_LOG(ERROR) << "unsupported results size: " << results_fmt.size();
          return 0;
        }
        RAY_LOG(DEBUG) << "function: " << funcref_idx << " params: " << params_fmt.size()
                       << " results: " << results_fmt.size();
        // calculate bytes used by parameters
        size_t params_bytes = 0;
        for (auto &param : params_fmt) {
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
            RAY_LOG(DEBUG) << "unsupported parameter type: " << param.kind();
            return 0;
          }
        }

        // get exported memory
        optional<Extern> export_memory = caller.get_export("memory");
        if (!export_memory) {
          RAY_LOG(ERROR) << "cannot get memory";
          return 0;
        }
        if (!holds_alternative<WasmMemory>(*export_memory)) {
          RAY_LOG(ERROR) << "memory not found";
          return 0;
        }

        // get actual memory object
        auto memory = get<WasmMemory>(*caller.get_export("memory"));
        if (memory.data_size(caller.context()) <= args) {
          RAY_LOG(ERROR) << "data address out of range: 0x" << hex << args;
          return 0;
        }

        // make sure the parameter size is within the memory range
        if (memory.data_size(caller.context()) < args + params_bytes) {
          RAY_LOG(ERROR) << "data address out of range: 0x" << hex << args << " + "
                         << params_bytes;
          return 0;
        }

        RAY_LOG(DEBUG) << "memory size: 0x" << hex << memory.data_size(caller.context());
        print_hex(memory.data(caller.context()).data(), args, params_bytes);

        RAY_LOG(DEBUG) << "create remote function: 0x" << hex << funcref_idx << " args 0x"
                       << args;

        // iterate result format
        for (auto &result : results_fmt) {
          switch (result.kind()) {
          case WasmValueType::I32: {
            int32_t (*fp)() = (int32_t(*)())(0L + funcref_idx);
            return launch_remote<int32_t>(
                params_fmt, fp, funcref_idx, args, memory, caller);
          } break;
          case WasmValueType::I64: {
            int64_t (*fp64)() = (int64_t(*)())(0L + funcref_idx);
            return launch_remote<int64_t>(
                params_fmt, fp64, funcref_idx, args, memory, caller);
          } break;
          case WasmValueType::F32: {
            float (*fp32)() = (float (*)())(0L + funcref_idx);
            return launch_remote<float>(
                params_fmt, fp32, funcref_idx, args, memory, caller);
          } break;
          case WasmValueType::F64: {
            double (*fp64d)() = (double (*)())(0L + funcref_idx);
            return launch_remote<double>(
                params_fmt, fp64d, funcref_idx, args, memory, caller);
          } break;
          default:
            RAY_LOG(DEBUG) << "unsupported result type: " << result.kind();
            return 0;
          }
        }
        return 0;
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
    RAY_LOG(DEBUG) << "cannot get function: " << func_name;
    return nullopt;
  }

  // check variant type
  if (!holds_alternative<WasmFunction>(*export_func)) {
    RAY_LOG(DEBUG) << "function " << func_name << " not found";
    return nullopt;
  }

  auto func = get<WasmFunction>(*export_func);
  RAY_LOG(DEBUG) << "found function: \"" << func_name << "\"";
  return optional<WasmFunction>(func);
}

/* get table from exports by table name */
optional<WasmTable> table_from_exports(WasmInstance &instance,
                                       WasmStore &store,
                                       const char *tbl_name) {
  optional<Extern> export_tbl;
  if (!(export_tbl = instance.get(store, tbl_name))) {
    RAY_LOG(DEBUG) << "cannot get table: " << tbl_name;
    return nullopt;
  }

  // check variant type
  if (!holds_alternative<WasmTable>(*export_tbl)) {
    RAY_LOG(DEBUG) << "table " << tbl_name << " not found";
    return nullopt;
  }

  auto table = get<WasmTable>(*export_tbl);
  RAY_LOG(DEBUG) << "table \"" << tbl_name << "\" size = " << table.size(store);
  return optional<WasmTable>(table);
}

/* get function pointer from exports by item index */
optional<WasmFunction> function_from_exports(WasmInstance &instance,
                                             WasmStore &store,
                                             int index) {
  auto tmp = instance.get(store, index);
  if (!tmp) {
    RAY_LOG(DEBUG) << "cannot get export " << index;
    return nullopt;
  }
  auto name = get<string_view>(*tmp);
  auto ext = get<Extern>(*tmp);
  if (!holds_alternative<WasmFunction>(ext)) {
    RAY_LOG(DEBUG) << "export " << index << " is not a function";
    return nullopt;
  }
  auto func = get<WasmFunction>(ext);

  RAY_LOG(DEBUG) << "found function " << name << " at export index " << index
                 << ". store_id: " << func.raw_func().store_id
                 << " func index: " << func.raw_func().index;
  return func;
}

/* get function pointer from table using table class */
optional<WasmFunction> function_from_table(WasmInstance &instance,
                                           WasmStore &store,
                                           WasmTable &table,
                                           int index) {
  optional<WasmValue> val;
  if (!(val = table.get(store, index))) {
    RAY_LOG(DEBUG) << "cannot get function at index " << index;
    return nullopt;
  }

  if (val->kind() != ValKind::FuncRef) {
    RAY_LOG(DEBUG) << "value at index " << index << " is not function";
    return nullopt;
  }

  optional<WasmFunction> func = val->funcref();
  if (!func) {
    cout << "function at index " << index << " not found";
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
    RAY_LOG(DEBUG) << "cannot get table: " << table_name;
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
    RAY_LOG(DEBUG) << "function " << func_name << " not found";
    return;
  }

  auto raw = function_raw_pointer(store, *func);
  RAY_LOG(DEBUG) << "calling function \"" << func_name << "\" ";
  RAY_LOG(DEBUG) << "raw pointer: 0x" << hex << raw;

  func->call(store, args).unwrap();
}

// TODO: refer to https://github.com/Mossaka/wasi-callback/blob/main/src/main.rs for a way
// to register a callback

}  // namespace wasm_engine
