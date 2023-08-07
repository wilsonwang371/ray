#include "assert.h"
#include "stdio.h"
#include "war.h"

// export buffer free() to host
ENABLE_WAR_BUFFER_AUTO_FREE

// export buffer alloc() to host
ENABLE_MALLOC_BY_HOST

float task_func(char a, short b, float c, float d, int e) {
  float res = a + b + c + d;
  return res;
}

int main() {
  float result;

  printf("call task_func directly\n");
  result = task_func('a', 1, 2.0, 3.0, 4);
  printf("result: %f\n", result);

  int res;
  size_t res_len = sizeof(float);
  warbuffer *wbuf;
  // initialize ray engine
  WAR_INIT();

  // allocate buffer to store remote invocation ref handle
  WAR_OBJID_ALLOC(wbuf);

  printf("call task_func remotely\n");
  // invoke function remotely
  res = rcall(wbuf, task_func, 'a', 1, 2.0, 3.0, 4);
  assert(res == 0);
  // get function result from ref handle
  res = rget(wbuf, &result, &res_len);
  assert(res == 0);
  printf("result: %f\n", result);

  WAR_OBJID_FREE(wbuf);
  return 0;
}
