#include "stdio.h"

__attribute__((import_module("ray"), import_name("call"))) void rcall(void *);

#define REMOTE(p) rcall((void *)(p))

// add function
int add(int a, int b) {
  fprintf(stderr, "inside function add\n");
  return a + b;
}

// add function
int add2(int a, int b) {
  fprintf(stderr, "inside function add2\n");
  return a + b;
}

// main function
int main() {
  // call remote function
  fprintf(stderr, "register remote function: add %p\n", add);
  REMOTE(add);
  fprintf(stderr, "register remote function: add2 %p\n", add2);
  REMOTE(add2);
  return 0;
}
