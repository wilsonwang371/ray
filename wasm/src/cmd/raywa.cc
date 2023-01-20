#include <ray/api.h>

// clang-format off
#include "ray/core_worker/context.h"
#include <engine/wasm_engine.h>
#include "absl/strings/str_split.h"

// https://github.com/CLIUtils/CLI11
#include <cmd/CLI11.hpp>
// clang-format on

using namespace std;
using namespace wasm_engine;

int main(int argc, char **argv) {
  string wasm_file;
  string code_search_path;

  CLI::App app{"RayWa - Ray WebAssembly CLI"};

  CLI::Option *code_search_path_opt =
      app.add_option("-p,--code-search-path", code_search_path, "Code search path");
  code_search_path_opt->type_name("PATH");

  CLI::Option *wasm_file_opt =
      app.add_option("file", wasm_file, "WebAssembly file to run");
  wasm_file_opt->required();
  wasm_file_opt->type_name("FILE");

  app.set_version_flag("-v,--version", string(CLI11_VERSION));

  CLI11_PARSE(app, argc, argv);

#ifdef RAYWA_DEBUG
  cerr << "wasm_file = " << wasm_file << endl;
  cerr << "code_search_path = " << code_search_path << endl;
#endif

  auto engine = WasmEngine();
  auto store = WasmStore(engine);
  auto linker = WasmLinker(engine);

  init_host_env(linker, store);

  auto module = compile_wasm_file(engine, wasm_file);
  if (!module) {
    cerr << "cannot compile wasm file: " << wasm_file << endl;
    return 1;
  }

  auto instance = init_wasm_module(linker, store, *module);
  if (!instance) {
    cerr << "cannot init wasm module: " << wasm_file << endl;
    return 1;
  }

  //   auto table = table_from_exports(instance, store, WASMFUNC_TBL_NAME);
  //   if (!table) {
  //     cerr << "cannot get table: " << WASMFUNC_TBL_NAME << endl;
  //     return 1;
  //   }

  //   // iterate table
  //   for (int i = 0; i < table->size(store); i++) {
  //     auto func = function_from_table(instance, store, WASMFUNC_TBL_NAME, i);
  //     if (!func) {
  //       continue;
  //     }

  //     size_t raw = function_raw_pointer(store, *func);

  //     if (i == 1) {
  //       cerr << "calling function at index " << i << endl;
  //       unwrap(func->call(store, {1, 3}));
  //     }

  //     // get the function type
  //     WasmFunctionType func_type = func->type(store);

  // #ifdef RAYWA_DEBUG
  //     cerr << "function raw pointer: 0x" << hex << raw << ", ";
  //     cerr << "table: " << WASMFUNC_TBL_NAME << " idx: " << i << ", ["
  //          << func_type->params().size() << "] -> " << func_type->results().size()
  //          << ", store id: " << func->raw_func().store_id
  //          << ", idx: " << func->raw_func().index << endl;
  // #endif

  //     for (int j = 0; j < 1000; j++) {
  //       auto func_inner = function_from_exports(instance, store, j);
  //       if (!func_inner) {
  //         break;
  //       }
  //       if (func_inner->raw_func().store_id == func->raw_func().store_id &&
  //           func_inner->raw_func().index == func->raw_func().index) {
  //         cout << "found!! " << endl;
  //       }
  //     }
  //   }

  // call_function_by_name(instance, store, "add", {1, 2});

  ray::RayConfig config;
  if (code_search_path_opt->count() > 0) {
    config.code_search_path = absl::StrSplit(code_search_path, ':', absl::SkipEmpty());
  }
  ray::Init(config);

  call_function_by_name(*instance, store, "_start", {});

  /// common task
  // auto task_object = ray::Task(Plus).Remote(1, 2);
  // int task_result = *(ray::Get(task_object));
  // cerr << "task_result = " << task_result << endl;

  ray::Shutdown();
  return 0;
}
