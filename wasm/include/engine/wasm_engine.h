#if !defined(WASM_ENGINE_H)

#define WASM_ENGINE_H

#define ENGINE_WASMTIME 1

#if defined(ENGINE_WASMTIME)

#include "wasmtime.hh"

#elif defined(ENGINE_WAVM)
//TODO: add wavm engine
#elif defined(ENGINE_WAMR)
//TODO: add wamr engine
#endif

#endif  // WASM_ENGINE_H
