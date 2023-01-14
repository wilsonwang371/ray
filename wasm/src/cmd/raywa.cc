#include <ray/api.h>

// clang-format off
#include "ray/core_worker/context.h"
#include <engine/wasm_engine.h>

// https://github.com/CLIUtils/CLI11
#include <cmd/CLI11.hpp>
// clang-format on

using namespace std;

using namespace wasm_engine;

/// common function
int Plus(int x, int y) { return x + y; }
/// Declare remote function
RAY_REMOTE(Plus);

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

  // read all bytes from wasm file
  ifstream infile(wasm_file, ios::binary | ios::ate);
  if (!infile.is_open()) {
    cout << "Failed to open wasm file" << endl;
    return 0;
  }

  // get length of file
  infile.seekg(0, std::ios::end);
  size_t length = infile.tellg();
  infile.seekg(0, std::ios::beg);

  // allocate memory:
  char *buffer = new char[length];
  infile.read(buffer, length);
  infile.close();

  Span<uint8_t> wasm_bytes = Span<uint8_t>(reinterpret_cast<uint8_t *>(buffer), length);

  auto engine = std::make_unique<WasmEngine>();
  auto store = std::make_unique<WasmStore>(*engine);
  auto linker = std::make_unique<WasmLinker>(*engine);

  auto module = WasmModule::compile(*engine, wasm_bytes);

  ray::Init();

  /// common task
  auto task_object = ray::Task(Plus).Remote(1, 2);
  int task_result = *(ray::Get(task_object));
  cout << "task_result = " << task_result << endl;

  ray::Shutdown();

  return 0;
}
