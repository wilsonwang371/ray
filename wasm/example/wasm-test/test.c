#include "stdio.h"

__attribute__((import_module("ray"), import_name("call"))) void rcall(void *, ...);

// macro to call remote function
#define REMOTE(p, ...) rcall((void *)(p), ##__VA_ARGS__)

// add function
int add(int a, int b) {
  fprintf(stderr, "inside function add\n");
  return a + b;
}

// add function
void dummy() { fprintf(stderr, "inside function dummy\n"); }

// main function
int main() {
  // call remote function
  fprintf(stderr, "register remote function: add %p\n", add);
  REMOTE(add, (uint8_t)2, 3, 4, 5, 6, 7, 8, 9, 10);
  fprintf(stderr, "register remote function: dummy %p\n", dummy);
  REMOTE(dummy);
  return 0;
}
