#include <ray/api.h>

// clang-format off
#include "ray/core_worker/context.h"
#include <engine/wasm_engine.h>

// https://github.com/CLIUtils/CLI11
#include <cmd/CLI11.hpp>
// clang-format on

#define FUNC_TABLE_NAME "__indirect_function_table"

using namespace std;
using namespace wasm_engine;

/// common function
int Plus(int x, int y) { return x + y; }
/// Declare remote function
RAY_REMOTE(Plus);

uint8_t *read_file(const string &file_name, size_t *length_ptr) {
  // read all bytes from wasm file
  ifstream infile(file_name, ios::binary | ios::ate);
  if (!infile.is_open()) {
    cout << "Failed to open wasm file" << endl;
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
  cout << "wasm_file = " << wasm_file << endl;

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
  RegisterWasmRayHandlers(linker);

  auto module = CompileWasmModule(engine, wasm_bytes, length);
  auto instance = InstantiateWasmModule(linker, store, module);

  std::optional<Extern> export_tbl;
  if (!(export_tbl = instance.get(store, FUNC_TABLE_NAME))) {
    cout << "cannot get table: " << FUNC_TABLE_NAME << endl;
    return 0;
  }

  // check variant type
  if (!std::holds_alternative<WasmTable>(*export_tbl)) {
    cout << "table " << FUNC_TABLE_NAME << " not found" << endl;
    return 0;
  }

  auto table = std::get<WasmTable>(*export_tbl);
  cout << "table size = " << table.size(store) << endl;

  // iterate table
  for (int i = 0; i < table.size(store); i++) {
    std::optional<Val> val;
    if (!(val = table.get(store, i))) {
      cout << "cannot get function at index " << i << endl;
      continue;
    }

    if (val->kind() != ValKind::FuncRef) {
      cout << "value at index " << i << " is not function" << endl;
      continue;
    }

    std::optional<WasmFunction> func = val->funcref();
    if (!func) {
      cout << "function at index " << i << " not found" << endl;
      continue;
    }

    // get the function type
    WasmFunctionType func_type = func->type(store);

    cout << func_type->params().size() << " -> " << func_type->results().size()
         << endl;
  }

  ray::Init();

  // call main function
  auto main_func = instance.get(store, "_start");
  if (!main_func) {
    cout << "main function not found" << endl;
    return 0;
  }
  std::get<Func>(*main_func).call(store, {}).unwrap();

  /// common task
  auto task_object = ray::Task(Plus).Remote(1, 2);
  int task_result = *(ray::Get(task_object));
  cout << "task_result = " << task_result << endl;

  ray::Shutdown();

  return 0;
}
