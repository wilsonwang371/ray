#include <ray/api.h>

// https://github.com/CLIUtils/CLI11
#include <cmd/CLI11.hpp>

using namespace std;

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

  ray::Init();

  /// common task
  auto task_object = ray::Task(Plus).Remote(1, 2);
  int task_result = *(ray::Get(task_object));
  cout << "task_result = " << task_result << endl;

  ray::Shutdown();

  cout << "wasm_file: " << wasm_file << ", " << wasm_file_opt->count() << endl;
  return 0;
}
