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

// clang-format off
#include "ray/core_worker/context.h"
#include "engine/wasm_engine.h"
// clang-format on

namespace wasm_engine {

WasmModule CompileWasmModule(WasmEngine &engine, uint8_t *code, size_t length) {
  auto data = Span<uint8_t>(reinterpret_cast<uint8_t *>(code), length);
  return unwrap(WasmModule::compile(engine, data));
}

WasmInstance InstantiateWasmModule(WasmLinker &linker,
                                   WasmStore &store,
                                   WasmModule &module) {
  return unwrap(linker.instantiate(store, module));
}

void ExecuteInstanceFunction(WasmInstance &instance,
                             WasmStore &store,
                             std::string &func_name,
                             const std::vector<Val> &params) {
  auto func = std::get<WasmFunction>(*instance.get(store, func_name));
  unwrap(func.call(store, params));
}

void RegisterWasmRayHandlers(WasmLinker &linker) {
  unwrap(linker.func_new(
      "ray", "get", WasmFunctionType({WasmValueType::I32}, {}), [
      ](auto caller, auto params, auto results) -> auto{ return std::monostate(); }));

  unwrap(linker.func_new(
      "ray", "put", WasmFunctionType({WasmValueType::I32}, {}), [
      ](auto caller, auto params, auto results) -> auto{ return std::monostate(); }));

  unwrap(linker.func_wrap(
      "ray", "call", [](WasmCaller caller, int32_t name, int32_t args) -> auto{
        caller.get_export("table");
        // TODO:
        return std::monostate();
      }));
}

// TODO: refer to https://github.com/Mossaka/wasi-callback/blob/main/src/main.rs for a way to register a callback

}  // namespace wasm_engine
