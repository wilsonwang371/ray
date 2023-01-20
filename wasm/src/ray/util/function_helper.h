// Copyright 2020-2021 The Ray Authors.
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

#pragma once

#include <ray/api/common_types.h>
#include <ray/api/ray_runtime_holder.h>

#include <boost/dll.hpp>
#include <filesystem>
#include <memory>
#include <msgpack.hpp>
#include <string>
#include <unordered_map>

#include "ray/core_worker/context.h"
#include <engine/wasm_engine.h>

using namespace ::ray::internal;
using namespace wasm_engine;

namespace ray {
namespace internal {

using EntryFunction = std::function<msgpack::sbuffer(
    const std::string &, const ArgsBufferList &, msgpack::sbuffer *)>;

class FunctionHelper {
 public:
  static FunctionHelper &GetInstance() {
    // We use `new` here because we don't want to destruct this instance forever.
    // If we do destruct, the shared libraries will be unloaded. And Maybe the unloading
    // will bring some errors which hard to debug.
    static auto *instance = new FunctionHelper();
    return *instance;
  }

  void LoadDll(const std::filesystem::path &lib_path);
  void LoadFunctionsFromPaths(const std::vector<std::string> &paths);
  const EntryFunction &GetExecutableFunctions(const std::string &function_name);
  const EntryFunction &GetExecutableMemberFunctions(const std::string &function_name);

  // wasm related functions
  void LoadWasm(const std::filesystem::path &lib_path);
  void LoadWasmFunctionsFromPaths(const std::vector<std::string> &paths);
  const WasmFunction &GetWasmFunctions(const std::string &function_name);

  std::shared_ptr<WasmEngine> wasm_engine_;
  std::shared_ptr<WasmStore> wasm_store_;
  std::shared_ptr<WasmLinker> wasm_linker_;

 private:
  FunctionHelper();
  ~FunctionHelper() = default;
  FunctionHelper(FunctionHelper const &) = delete;
  FunctionHelper(FunctionHelper &&) = delete;
  std::string LoadAllRemoteFunctions(const std::string lib_path,
                                     const boost::dll::shared_library &lib,
                                     const EntryFunction &entry_function);
  std::unordered_map<std::string, std::shared_ptr<boost::dll::shared_library>> libraries_;
  // Map from remote function name to executable entry function.
  std::unordered_map<std::string, EntryFunction> remote_funcs_;
  // Map from remote member function name to executable entry function.
  std::unordered_map<std::string, EntryFunction> remote_member_funcs_;

  // Map from wasm function name to executable entry function.
  std::unordered_map<std::string, WasmFunction> wasm_funcs_;

  std::unordered_map<std::string, WasmModule> wasm_modules_;
  std::unordered_map<std::string, WasmInstance> wasm_instances_;
};
}  // namespace internal
}  // namespace ray