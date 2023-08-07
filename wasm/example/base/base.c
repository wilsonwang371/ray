#include "assert.h"
#include "stdio.h"
#include "war.h"

ENABLE_WAR_BUFFER_AUTO_FREE
ENABLE_MALLOC_BY_HOST

int test_call();
int test_put();
int test_struct();
int test_bufptr();

typedef int (*test_func)();

test_func tests[] = {test_call, test_call, test_put, test_struct, test_bufptr};

int test_put() {
  unsigned char data[32] = {0x1, 0x2, 0x3, 0x4, 0x5, 0x6, 0x7, 0x8, 0x9, 0xa, 0xb,
                            0xc, 0xd, 0xe, 0xf, 0x0, 0x1, 0x2, 0x3, 0x4, 0x5, 0x6,
                            0x7, 0x8, 0x9, 0xa, 0xb, 0xc, 0xd, 0xe, 0xf, 0x0};
  unsigned char result_buf[100];
  unsigned int len = sizeof(result_buf);

  warbuffer *wbuf;
  WAR_OBJID_ALLOC(wbuf);

  // put data into object store
  WAR_PUT(wbuf, data, sizeof(data));

  // get data from object store
  WAR_GET(wbuf, result_buf, &len);

  // verify data matches
  for (size_t i = 0; i < sizeof(data); i++) {
    assert(result_buf[i] == data[i]);
  }

  WAR_OBJID_FREE(wbuf);
  return 0;
}

float task_func(char a, short b, float c, float d, int e) {
  float res = a + b + c + d;
  LOG_INFO("task_func: 0x%x, 0x%x, %f, %lf, 0x%x", a, b, c, d, e);
  LOG_INFO("task_func: result: %f", res);
  // print result in hex format
  LOG_INFO("task_func: result in hex: ");
  LOG_INFO(fmt_hex((unsigned char *)&res, sizeof(float)));
  return res;
}

int test_call() {
  unsigned char result_buf[100];
  unsigned int len = sizeof(result_buf);
  float result;

  warbuffer *wbuf;
  WAR_OBJID_ALLOC(wbuf);

  // remote execution
  WAR_ASYNC_CALL(wbuf, task_func, 'A', 0x1234, 1.0f, 3.1415926f, 0x6789abcd);
  WAR_GET(wbuf, result_buf, &len);

  if (len != sizeof(float)) {
    LOG_ERROR("rget len mismatch: %d\n", len);
    return 1;
  }

  result = BUF2FLOAT(result_buf);
  LOG_INFO("remote execution result: %f", result);
  // print result in byte hex format
  LOG_INFO("remote execution result in hex: ");
  LOG_INFO(fmt_hex(result_buf, sizeof(float)));
  LOG_INFO("***remote execution done***");

  // local execution
  LOG_INFO("***local execution start***");
  result = task_func('A', 0x1234, 1.0, 3.1415926, 0x6789abcd);
  LOG_INFO("local execution result: %f", result);
  // print result in byte hex format
  LOG_INFO("local execution result in hex: ");
  LOG_INFO(fmt_hex((unsigned char *)&result, sizeof(float)));
  LOG_INFO("***local execution done***");

  WAR_OBJID_FREE(wbuf);
  return 0;
}

typedef struct test_struct_type {
  int a;
  float b;
  double c;
  int d;
} test_struct_type;

int task_struct(struct test_struct_type s) {
  LOG_INFO("task_struct: %d, %f, %lf, %d", s.a, s.b, s.c, s.d);
  return 0;
}

int test_struct() {
  unsigned char result_buf[100];
  unsigned int len = sizeof(result_buf);

  warbuffer *wbuf;
  WAR_OBJID_ALLOC(wbuf);

  struct test_struct_type param = {1, 2.0f, 3.0, 4};

  WAR_ASYNC_CALL(wbuf, task_struct, WAR_STRUCT_VAR(param));
  WAR_GET(wbuf, result_buf, &len);

  WAR_OBJID_FREE(wbuf);
  return 0;
}

int task_buffer(unsigned char *s, unsigned int len) {
  for (size_t i = 0; i < len - 1; i++) {
    if (s[i] != i % 256) {
      LOG_ERROR("task_buffer: mismatch at %zu: 0x%x\n", i, s[i]);
      return 1;
    }
  }
  return 0;
}

int test_bufptr() {
  unsigned char result_buf[100];
  unsigned int len = sizeof(result_buf);

#define BUF_SIZE 4096
  unsigned char *param_buf = walloc(BUF_SIZE);

  warbuffer *wbuf;
  WAR_OBJID_ALLOC(wbuf);

  // initialize param_buf
  for (size_t i = 0; i < BUF_SIZE; i++) {
    param_buf[i] = i % 256;
  }

  WAR_ASYNC_CALL(wbuf, task_buffer, param_buf, BUF_SIZE);
  WAR_GET(wbuf, result_buf, &len);

  wfree(param_buf);
  WAR_OBJID_FREE(wbuf);

  // convert result_buf to int
  int res = *(int *)(result_buf);
  if (res != 0) {
    LOG_ERROR("task_buffer failed: %d\n", res);
    return 1;
  }
  return 0;
}

int main() {
  WAR_INIT();

  for (size_t i = 0; i < sizeof(tests) / sizeof(tests[0]); i++) {
    if (tests[i]() != 0) {
      LOG_ERROR("test %zu failed\n", i);
      return 1;
    }
    LOG_INFO("test %zu ok\n", i);
  }

  return 0;
}
