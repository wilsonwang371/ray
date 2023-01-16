#include <ray/api.h>

// clang-format off
#include "ray/core_worker/context.h"
#include <engine/wasm_engine.h>

// https://github.com/CLIUtils/CLI11
#include <cmd/CLI11.hpp>
// clang-format on

using namespace std;
using namespace wasm_engine;

uint8_t *read_file(const string &file_name, size_t *length_ptr) {
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

int main(int argc, char **argv) {
  CLI::App app{"RayWa - Ray WebAssembly CLI"};
  app.set_version_flag("-v,--version", string(CLI11_VERSION));

  string wasm_file;
  CLI::Option *wasm_file_opt =
      app.add_option("-f,--wasm-file,file", wasm_file, "WebAssembly file to run");

  CLI11_PARSE(app, argc, argv);

  if (wasm_file_opt->count() == 0) {
    cout << "Please specify wasm file" << endl;
    return 0;
  }
#ifdef RAYWA_DEBUG
  cerr << "wasm_file = " << wasm_file << endl;
#endif

  size_t length = 0;
  auto wasm_bytes = read_file(wasm_file, &length);

  auto engine = WasmEngine();
  auto store = WasmStore(engine);
  auto linker = WasmLinker(engine);

  WasiConfig wasi;
  wasi.inherit_argv();
  wasi.inherit_env();
  wasi.inherit_stdin();
  wasi.inherit_stdout();
  wasi.inherit_stderr();
  store.context().set_wasi(std::move(wasi)).unwrap();

  unwrap(linker.define_wasi());
  register_ray_handlers(linker);

  auto module = compile_wasm_module(engine, wasm_bytes, length);
  auto instance = init_wasm_module(linker, store, module);

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

  ray::Init();

  call_function_by_name(instance, store, "_start", {});

  /// common task
  //auto task_object = ray::Task(Plus).Remote(1, 2);
  //int task_result = *(ray::Get(task_object));
  //cerr << "task_result = " << task_result << endl;

  ray::Shutdown();

  return 0;
}
