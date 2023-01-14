#include "stdio.h"

__attribute__((import_module("ray"), import_name("call"))) void rcall(void *, ...);

#define REMOTE(p, ...) rcall((void *)(p), __VA_ARGS__)

// add function
int add(int a, int b) { return a + b; }

// main function
int main() {
  // call remote function
  fprintf(stderr, "call remote function: %p\n", add);
  REMOTE(add, 1, 2);
  return 0;
}
