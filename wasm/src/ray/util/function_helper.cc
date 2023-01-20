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

#include "function_helper.h"

#include <boost/range/iterator_range.hpp>
#include <memory>

#include "ray/util/logging.h"

namespace ray {
namespace internal {

FunctionHelper::FunctionHelper() {
  RAY_LOG(INFO) << "FunctionHelper is initialized.";
  wasm_engine_ = std::make_shared<WasmEngine>();
  wasm_store_ = std::make_shared<WasmStore>(*wasm_engine_);
  wasm_linker_ = std::make_shared<WasmLinker>(*wasm_engine_);
  init_host_env(*wasm_linker_, *wasm_store_);
}

void FunctionHelper::LoadDll(const std::filesystem::path &lib_path) {
  RAY_LOG(INFO) << "Start loading the library " << lib_path << ".";

  auto it = libraries_.find(lib_path.string());
  if (it != libraries_.end()) {
    return;
  }

  RAY_CHECK(std::filesystem::exists(lib_path))
      << lib_path << " dynamic library not found.";

  std::shared_ptr<boost::dll::shared_library> lib = nullptr;
  try {
    lib = std::make_shared<boost::dll::shared_library>(
        lib_path.string(), boost::dll::load_mode::type::rtld_lazy);
  } catch (std::exception &e) {
    RAY_LOG(FATAL) << "Failed to load library, lib_path: " << lib_path
                   << ", failed reason: " << e.what();
    return;
  } catch (...) {
    RAY_LOG(FATAL) << "Failed to load library, lib_path: " << lib_path
                   << ", unknown failed reason.";
    return;
  }

  RAY_CHECK(libraries_.emplace(lib_path.string(), lib).second);

  try {
    auto entry_func = boost::dll::import_alias<msgpack::sbuffer(
        const std::string &, const ArgsBufferList &, msgpack::sbuffer *)>(
        *lib, "TaskExecutionHandler");
    auto function_names = LoadAllRemoteFunctions(lib_path.string(), *lib, entry_func);
    if (function_names.empty()) {
      RAY_LOG(WARNING)
          << "No remote functions in library " << lib_path
          << ". If you've already used Ray::Task or Ray::Actor in the library, please "
             "ensure the remote functions have been registered by `RAY_REMOTE` macro.";
      lib->unload();
      return;
    }
    RAY_LOG(INFO) << "The library " << lib_path
                  << " is loaded successfully. The remote functions: " << function_names
                  << ".";
    return;
  } catch (boost::system::system_error &e) {
    RAY_LOG(INFO) << "The library " << lib_path << " isn't integrated with Ray, skip it.";
    lib->unload();
  } catch (std::exception &e) {
    RAY_LOG(WARNING) << "Failed to get entry function from library: " << lib_path
                     << ", failed reason: " << e.what();
    lib->unload();
  } catch (...) {
    RAY_LOG(WARNING) << "Failed to get entry function from library: " << lib_path
                     << ", unknown failed reason.";
    lib->unload();
  }
  return;
}

void FunctionHelper::LoadWasm(const std::filesystem::path &lib_path) {
  RAY_LOG(INFO) << "Start loading the library " << lib_path << ".";
  
  auto module = compile_wasm_file(*wasm_engine_, lib_path.string());
  if (!module) {
    RAY_LOG(ERROR) << "cannot compile wasm file: " << lib_path.string();
    return;
  }

  auto instance = init_wasm_module(*wasm_linker_, *wasm_store_, *module);
  if (!instance) {
    RAY_LOG(ERROR) << "cannot init wasm module: " << lib_path.string();
    return;
  }

  RAY_LOG(INFO) << "Here2";
  auto table = table_from_exports(*instance, *wasm_store_, WASMFUNC_TBL_NAME);
  if (!table) {
    cerr << "cannot get table: " << WASMFUNC_TBL_NAME << endl;
    return;
  }

  wasm_instances_.emplace(lib_path.string(), *instance);
  wasm_modules_.emplace(lib_path.string(), *module);

  // iterate table
  for (int i = 0; i < table->size(*wasm_store_); i++) {
    auto func = function_from_table(*instance, *wasm_store_, WASMFUNC_TBL_NAME, i);
    if (!func) {
      continue;
    }

    if (i == 1) {
      cerr << "calling function at index " << i << endl;
      unwrap(func->call(*wasm_store_, {1, 3}));
    }

    // get the function type
    WasmFunctionType func_type = func->type(*wasm_store_);

    char func_name[256];
    snprintf(func_name, sizeof(func_name), "%d", i);
    wasm_funcs_.emplace(string(func_name), *func);

#ifdef RAYWA_DEBUG
    size_t raw = function_raw_pointer(*wasm_store_, *func);
    cerr << "function raw pointer: 0x" << hex << raw << ", ";
    cerr << "table: " << WASMFUNC_TBL_NAME << " idx: " << i << ", ["
          << func_type->params().size() << "] -> " << func_type->results().size()
          << ", store id: " << func->raw_func().store_id
          << ", idx: " << func->raw_func().index << endl;
#endif

    // for (int j = 0; j < 1000; j++) {
    //   auto func_inner = function_from_exports(instance, store, j);
    //   if (!func_inner) {
    //     break;
    //   }
    //   if (func_inner->raw_func().store_id == func->raw_func().store_id &&
    //       func_inner->raw_func().index == func->raw_func().index) {
    //     cout << "found!! " << endl;
    //   }
    // }
  }
}

std::string FunctionHelper::LoadAllRemoteFunctions(const std::string lib_path,
                                                   const boost::dll::shared_library &lib,
                                                   const EntryFunction &entry_function) {
  static const std::string internal_function_name = "GetRemoteFunctions";
  if (!lib.has(internal_function_name)) {
    RAY_LOG(WARNING) << "Internal function '" << internal_function_name
                     << "' not found in " << lib_path;
    return "";
  }
  // Both default worker and user dynamic library static link libray_api.so which has a
  // singleton class RayRuntimeHolder, the user dynamic library will get a new un-init
  // instance of RayRuntimeHolder, so we need to init the RayRuntimeHolder singleton when
  // loading the user dynamic library to make sure the new instance valid.
  auto init_func =
      boost::dll::import_alias<void(std::shared_ptr<RayRuntime>)>(lib, "InitRayRuntime");
  init_func(RayRuntimeHolder::Instance().Runtime());

  auto get_remote_func = boost::dll::import_alias<
      std::pair<const RemoteFunctionMap_t &, const RemoteMemberFunctionMap_t &>()>(
      lib, internal_function_name);
  std::string names_str;
  auto function_maps = get_remote_func();
  for (const auto &pair : function_maps.first) {
    names_str.append(pair.first).append(", ");
    remote_funcs_.emplace(pair.first, entry_function);
  }
  for (const auto &pair : function_maps.second) {
    names_str.append(pair.first).append(", ");
    remote_member_funcs_.emplace(pair.first, entry_function);
  }
  if (!names_str.empty()) {
    names_str.pop_back();
    names_str.pop_back();
  }
  return names_str;
}

void FindWasmBinary(std::filesystem::path path,
                    std::list<std::filesystem::path> &wasm_binaries) {
  static const std::unordered_set<std::string> wasm_binary_extension = {".wasm"};
  auto extension = path.extension();
  if (wasm_binary_extension.find(extension.string()) != wasm_binary_extension.end()) {
    wasm_binaries.emplace_back(path);
  }
}

void FindDynamicLibrary(std::filesystem::path path,
                        std::list<std::filesystem::path> &dynamic_libraries) {
#if defined(_WIN32)
  static const std::unordered_set<std::string> dynamic_library_extension = {".dll"};
#elif __APPLE__
  static const std::unordered_set<std::string> dynamic_library_extension = {".dylib",
                                                                            ".so"};
#else
  static const std::unordered_set<std::string> dynamic_library_extension = {".so"};
#endif
  auto extension = path.extension();
  if (dynamic_library_extension.find(extension.string()) !=
      dynamic_library_extension.end()) {
    dynamic_libraries.emplace_back(path);
  }
}

void FunctionHelper::LoadWasmFunctionsFromPaths(const std::vector<std::string> &paths) {
  std::list<std::filesystem::path> wasm_binaries;
  // Lookup wasm binaries from paths.
  for (auto path : paths) {
    if (std::filesystem::is_directory(path)) {
      for (auto &entry :
           boost::make_iterator_range(std::filesystem::directory_iterator(path), {})) {
        FindWasmBinary(entry, wasm_binaries);
      }
    } else if (std::filesystem::exists(path)) {
      FindWasmBinary(path, wasm_binaries);
    } else {
      RAY_LOG(FATAL) << path << " wasm binary not found.";
    }
  }

  // Try to load all found wasm binaries.
  for (auto wasm_binary : wasm_binaries) {
    RAY_LOG(INFO) << "Found wasm binary: " << wasm_binary;
    LoadWasm(wasm_binary);
  }
}

void FunctionHelper::LoadFunctionsFromPaths(const std::vector<std::string> &paths) {
  std::list<std::filesystem::path> dynamic_libraries;
  // Lookup dynamic libraries from paths.
  for (auto path : paths) {
    if (std::filesystem::is_directory(path)) {
      for (auto &entry :
           boost::make_iterator_range(std::filesystem::directory_iterator(path), {})) {
        FindDynamicLibrary(entry, dynamic_libraries);
      }
    } else if (std::filesystem::exists(path)) {
      FindDynamicLibrary(path, dynamic_libraries);
    } else {
      RAY_LOG(FATAL) << path << " dynamic library not found.";
    }
  }

  // Try to load all found libraries.
  for (auto lib : dynamic_libraries) {
    LoadDll(lib);
  }
}

const EntryFunction &FunctionHelper::GetExecutableFunctions(
    const std::string &function_name) {
  auto it = remote_funcs_.find(function_name);
  if (it == remote_funcs_.end()) {
    throw RayFunctionNotFound("Executable function not found, the function name " +
                              function_name);
  } else {
    return it->second;
  }
}

const WasmFunction &FunctionHelper::GetWasmFunctions(const std::string &function_name) {
  auto it = wasm_funcs_.find(function_name);
  if (it == wasm_funcs_.end()) {
    throw RayFunctionNotFound("Wasm function not found, the function name " +
                              function_name);
  } else {
    return it->second;
  }
}

const EntryFunction &FunctionHelper::GetExecutableMemberFunctions(
    const std::string &function_name) {
  auto it = remote_member_funcs_.find(function_name);
  if (it == remote_member_funcs_.end()) {
    throw RayFunctionNotFound("Executable member function not found, the function name " +
                              function_name);
  } else {
    return it->second;
  }
}

}  // namespace internal
}  // namespace ray