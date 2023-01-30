#include "stdio.h"

__attribute__((import_module("ray"), import_name("call"))) uint32_t rcall(void *, ...);

// macro to call remote function
#define REMOTE(p, ...) rcall((void *)(p), ##__VA_ARGS__)

// add function
int add(int a, int b, int c) {
  fprintf(stderr, "inside function add(%d, %d, %d)\n", a, b, c);
  return a + b + c;
}

// add function
void dummy() { fprintf(stderr, "inside function dummy\n"); }

// main function
int main() {
  // call remote function
  fprintf(stderr, "register remote function: add %p\n", add);
  REMOTE(add, 122, 233, 344);
  fprintf(stderr, "register remote function: dummy %p\n", dummy);
  REMOTE(dummy);
  sleep(5);
  return 0;
}
