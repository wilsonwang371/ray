#include "assert.h"
#include "stdio.h"
#include "war.h"

ENABLE_WAR_BUFFER_AUTO_FREE
ENABLE_MALLOC_BY_HOST

#define DATA_SIZE 32

void task_func(unsigned char *mem, size_t len) {
  // print result in hex format
  LOG_INFO(fmt_hex(mem, len));
}

int main() {
  unsigned char *data = walloc(DATA_SIZE);
  unsigned char result_buf[100];

  WAR_INIT();
  for (size_t i = 0; i < 32; i++) {
    data[i] = i;
  }

  printf("call task_func directly\n");
  task_func(data, DATA_SIZE);

  int res;
  size_t res_len = sizeof(result_buf);
  warbuffer *wbuf;
  WAR_OBJID_ALLOC(wbuf);

  printf("call task_func remotely\n");
  res = rcall(wbuf, task_func, data, DATA_SIZE);
  assert(res == 0);
  res = rget(wbuf, &result_buf, &res_len);
  assert(res == 0);
  assert(res_len == 0);

  WAR_OBJID_FREE(wbuf);
  wfree(data);
  return 0;
}