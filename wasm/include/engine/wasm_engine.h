#pragma once

#define ENGINE_WASMTIME 1
#define RAYWA_DEBUG 1

#define WASMFUNC_TBL_NAME "__indirect_function_table"

namespace wasm_engine {

#if defined(ENGINE_WASMTIME)

using namespace std;

#include "wasmtime.hh"
using namespace wasmtime;

template <typename T, typename E>
T unwrap(Result<T, E> result) {
  if (result) {
    return result.ok();
  }
  std::cerr << "error: " << result.err().message() << "\n";
  std::abort();
}

typedef Engine WasmEngine;
typedef Linker WasmLinker;
typedef Store WasmStore;
typedef Module WasmModule;
typedef Instance WasmInstance;

typedef Func WasmFunction;
typedef FuncType WasmFunctionType;
typedef Memory WasmMemory;

typedef ValKind WasmValueType;
typedef Caller WasmCaller;
typedef Val WasmValue;

typedef Table WasmTable;

#elif defined(ENGINE_WAVM)
// TODO: add wavm engine
#elif defined(ENGINE_WAMR)
// TODO: add wamr engine
#endif

optional<WasmModule> compile_wasm_file(WasmEngine &, const string &);

WasmModule compile_wasm_bytes(WasmEngine &, uint8_t *, size_t);

optional<WasmInstance> init_wasm_module(WasmLinker &, WasmStore &, WasmModule &);

void init_host_env(WasmLinker &, WasmStore &);

// void register_ray_handlers(WasmLinker &);

/* get raw pointer of function */
size_t function_raw_pointer(WasmStore &, WasmFunction &);

/* get function from exports by function name */
optional<WasmFunction> function_from_exports(WasmInstance &, WasmStore &, const char *);

/* get table from exports by table name */
optional<WasmTable> table_from_exports(WasmInstance &, WasmStore &, const char *);

/* get function pointer from exports by item index */
optional<WasmFunction> function_from_exports(WasmInstance &, WasmStore &, int);

/* get function pointer from table using table class */
optional<WasmFunction> function_from_table(WasmInstance &, WasmStore &, WasmTable &, int);

/* get function pointer from table using table name */
optional<WasmFunction> function_from_table(WasmInstance &,
                                           WasmStore &,
                                           const char *,
                                           int);

/* call function by name */
void call_function_by_name(WasmInstance &, WasmStore &, const char *, vector<WasmValue>);

}  // namespace wasm_engine
