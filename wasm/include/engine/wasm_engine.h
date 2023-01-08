#if !defined(WASM_ENGINE_H)

#define WASM_ENGINE_H

#define ENGINE_WASMTIME 1

namespace wasm_engine {

#if defined(ENGINE_WASMTIME)

#include "wasmtime.hh"
using namespace wasmtime;

template<typename T, typename E>
T unwrap(Result<T, E> result) {
  if (result) {
    return result.ok();
  }
  std::cerr << "error: " << result.err().message() << "\n";
  std::abort();
}

typedef Engine WasmEngine;
typedef Func WasmFunction;
typedef Store WasmStore;
typedef FuncType WasmFunctionType;
typedef Linker WasmLinker;
typedef ValKind WasmValueType;

#elif defined(ENGINE_WAVM)
//TODO: add wavm engine
#elif defined(ENGINE_WAMR)
//TODO: add wamr engine
#endif

}

#endif  // WASM_ENGINE_H
