#pragma once

#define ENGINE_WASMTIME 1

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

typedef ValKind WasmValueType;
typedef Caller WasmCaller;

#elif defined(ENGINE_WAVM)
// TODO: add wavm engine
#elif defined(ENGINE_WAMR)
// TODO: add wamr engine
#endif

WasmModule CompileWasmModule(WasmEngine &engine, std::string &code);

WasmInstance InstantiateWasmModule(WasmLinker &linker,
                                   WasmStore &store,
                                   WasmModule &module);

void ExecuteInstanceFunction(WasmInstance &instance,
                             WasmStore &store,
                             std::string &func_name,
                             const std::vector<Val> &params);

void RegisterWasmRayHandlers(WasmLinker &linker);

}  // namespace wasm_engine
