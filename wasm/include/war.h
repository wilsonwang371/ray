
#ifndef __RAY_H__
#define __RAY_H__

#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define RAY_OBJECT_ID_MAGIC 0xc0de550a

#define RAY_TYPE_INVALID 0x0
#define RAY_TYPE_OBJECT_ID 0x1
#define RAY_TYPE_DATA 0x2

#define RAY_FLAG_NEED_DEALLOC 0x1 << 31

typedef struct {
  unsigned int magic;     // offset 0
  unsigned int type;      // offset 4
  unsigned int flags;     // offset 8
  unsigned char *buf;     // offset 12
  unsigned int len;       // offset 16
  unsigned int cap;       // offset 20
  unsigned int checksum;  // offset 24
} warbuffer;

__attribute__((import_module("ray"), import_name("log_write"))) int log_write(char *,
                                                                              size_t);

__attribute__((import_module("ray"), import_name("memregion_validate"))) int
memregion_validate(void *, size_t);

__attribute__((import_module("ray"), import_name("sleep"))) void sleep(int);

__attribute__((import_module("ray"), import_name("init"))) int rinit();

__attribute__((import_module("ray"), import_name("call"))) int rcall(warbuffer *id,
                                                                     void *func,
                                                                     ...);

__attribute__((import_module("ray"), import_name("get"))) int rget(warbuffer *id,
                                                                   unsigned char *buf,
                                                                   unsigned int *len);

__attribute__((import_module("ray"), import_name("put"))) int rput(warbuffer *id,
                                                                   unsigned char *buf,
                                                                   unsigned int len);

__attribute__((import_module("ray"), import_name("track_malloc"))) int track_malloc(
    void *ptr, size_t size);
__attribute__((import_module("ray"), import_name("track_free"))) int track_free(
    void *ptr);

void *walloc(size_t size) {
  void *ptr = malloc(size);
  if (ptr == NULL) {
    return NULL;
  }
  track_malloc(ptr, size);
  return ptr;
}

void wfree(void *ptr) {
  track_free(ptr);
  free(ptr);
}

static inline unsigned int calculate_checksum(warbuffer *);

static inline warbuffer *alloc_warbuffer(unsigned int type, unsigned int cap) {
  warbuffer *buf = (warbuffer *)malloc(sizeof(warbuffer));
  if (buf == NULL) {
    return NULL;
  }

  unsigned char *ibuf = (unsigned char *)malloc(cap);
  if (ibuf == NULL) {
    free(buf);
    return NULL;
  }

  buf->magic = RAY_OBJECT_ID_MAGIC;
  buf->type = type;
  buf->buf = ibuf;
  buf->len = 0;
  buf->cap = cap;
  buf->checksum = calculate_checksum(buf);
  return buf;
}

static inline unsigned int calculate_checksum(warbuffer *buf) {
  return (unsigned int)(buf->magic ^ buf->type ^ buf->flags ^ buf->len ^ buf->cap ^
                        (unsigned int)buf->buf);
}

static inline void free_warbuffer(warbuffer *buf) {
  if (buf != NULL) {
    if (buf->buf != NULL && buf->flags & RAY_FLAG_NEED_DEALLOC) {
      free(buf->buf);
    }
    free(buf);
  }
}

static inline int validate_warbuffer(warbuffer *buf) {
  if (buf == NULL) {
    return -1;
  }
  if (memregion_validate(buf, sizeof(warbuffer))) {
    return -1;
  }
  if (buf->magic != RAY_OBJECT_ID_MAGIC) {
    return -2;
  }
  if (buf->checksum != calculate_checksum(buf)) {
    return -3;
  }
  return 0;
}

static inline int copy_data_to_warbuffer(warbuffer *buf,
                                         unsigned char *data,
                                         unsigned int len) {
  if (buf == NULL) {
    return -1;
  }
  if (validate_warbuffer(buf)) {
    return -2;
  }
  if (buf->cap < len) {
    return -3;
  } else {
    memcpy(buf->buf, data, len);
    buf->len = len;
    buf->checksum = calculate_checksum(buf);
  }
  return 0;
}

static inline void *_alloc_warbuffer_with_data(unsigned int type,
                                               unsigned char *data,
                                               unsigned int len) {
  warbuffer *buf = alloc_warbuffer(type, len);
  if (buf == NULL) {
    return NULL;
  }
  memcpy(buf->buf, data, len);
  buf->len = len;
  buf->flags = RAY_FLAG_NEED_DEALLOC;
  buf->checksum = calculate_checksum(buf);
  return buf;
}

static inline unsigned char *fmt_hex(unsigned char *buf, unsigned int len) {
  static unsigned char hex[] = "0123456789abcdef";
  static unsigned char hexbuf[4096];
  unsigned char *ptr = hexbuf;
  if (len * 3 > sizeof(hexbuf)) {
    return NULL;
  }
  for (unsigned int i = 0; i < len; i++) {
    *ptr++ = hex[buf[i] >> 4];
    *ptr++ = hex[buf[i] & 0xf];
    // add space every byte
    *ptr++ = ' ';
  }
  *ptr = '\0';
  return hexbuf;
}

#define PRINTF_BUF_SIZE 4096

#define WAR_LOG(fmt, ...)                                                     \
  do {                                                                        \
    char *buf = malloc(PRINTF_BUF_SIZE);                                      \
    buf[0] = '\0';                                                            \
    snprintf(buf, sizeof(PRINTF_BUF_SIZE), (const char *)fmt, ##__VA_ARGS__); \
    log_write(buf, PRINTF_BUF_SIZE);                                          \
    free(buf);                                                                \
  } while (0)

#define WARN_STR "[WARN]: "
#define LOG_WARN(fmt, ...)                                            \
  do {                                                                \
    char *buf = malloc(PRINTF_BUF_SIZE);                              \
    char *buf2 = malloc(PRINTF_BUF_SIZE + strlen(WARN_STR));          \
    buf[0] = '\0';                                                    \
    buf2[0] = '\0';                                                   \
    snprintf(buf, PRINTF_BUF_SIZE, (const char *)fmt, ##__VA_ARGS__); \
    strcpy(buf2, WARN_STR);                                           \
    strncat(buf2, buf, PRINTF_BUF_SIZE + strlen(WARN_STR) - 1);       \
    log_write(buf2, strlen(buf2));                                    \
    free(buf);                                                        \
    free(buf2);                                                       \
  } while (0)

#define ERR_STR "[ERROR]: "
#define LOG_ERROR(fmt, ...)                                           \
  do {                                                                \
    char *buf = malloc(PRINTF_BUF_SIZE);                              \
    char *buf2 = malloc(PRINTF_BUF_SIZE + strlen(ERR_STR));           \
    buf[0] = '\0';                                                    \
    buf2[0] = '\0';                                                   \
    snprintf(buf, PRINTF_BUF_SIZE, (const char *)fmt, ##__VA_ARGS__); \
    strcpy(buf2, ERR_STR);                                            \
    strncat(buf2, buf, PRINTF_BUF_SIZE + strlen(ERR_STR) - 1);        \
    log_write(buf2, strlen(buf2));                                    \
    free(buf);                                                        \
    free(buf2);                                                       \
  } while (0)

#define INFO_STR "[INFO]: "
#define LOG_INFO(fmt, ...)                                            \
  do {                                                                \
    char *buf = malloc(PRINTF_BUF_SIZE);                              \
    char *buf2 = malloc(PRINTF_BUF_SIZE + strlen(INFO_STR));          \
    buf[0] = '\0';                                                    \
    buf2[0] = '\0';                                                   \
    snprintf(buf, PRINTF_BUF_SIZE, (const char *)fmt, ##__VA_ARGS__); \
    strcpy(buf2, INFO_STR);                                           \
    strncat(buf2, buf, PRINTF_BUF_SIZE + strlen(ERR_STR) - 1);        \
    log_write(buf2, strlen(buf2));                                    \
    free(buf);                                                        \
    free(buf2);                                                       \
  } while (0)

#define WAR_ASYNC_CALL(id, func, ...)               \
  do {                                              \
    int res = rcall(id, func, __VA_ARGS__);         \
    if (res != 0) {                                 \
      LOG_ERROR(stderr, "rcall failed: %d\n", res); \
      exit(res);                                    \
    }                                               \
  } while (0)

#define ENABLE_WAR_BUFFER_AUTO_FREE               \
  int __war_free_warbuffer(warbuffer *buf) {      \
    if (buf != NULL) {                            \
      if (validate_warbuffer(buf) != 0) {         \
        LOG_ERROR("validate_warbuffer failed\n"); \
        return -1;                                \
      }                                           \
      free_warbuffer(buf);                        \
    }                                             \
    return 0;                                     \
  }

#define ENABLE_MALLOC_BY_HOST                              \
  void *__war_malloc(size_t size) { return malloc(size); } \
  void __war_free(void *ptr) { free(ptr); }

#define WAR_STRUCT_VAR(struct_var) \
  _alloc_warbuffer_with_data(      \
      RAY_TYPE_DATA, (unsigned char *)&(struct_var), sizeof(struct_var))

#define WAR_BUFFER_VAR(ptr, len) \
  _alloc_warbuffer_with_data(RAY_TYPE_DATA, (unsigned char *)(ptr), (len))

#define WAR_GET(id, buf, len)              \
  do {                                     \
    int res = rget((id), (buf), (len));    \
    if (res != 0) {                        \
      LOG_ERROR("rget failed: %d\n", res); \
      exit(res);                           \
    }                                      \
  } while (0)

#define WAR_PUT(id, buf, len)              \
  do {                                     \
    int res = rput((id), (buf), (len));    \
    if (res != 0) {                        \
      LOG_ERROR("rput failed: %d\n", res); \
      exit(res);                           \
    }                                      \
  } while (0)

#define WAR_INIT()                          \
  do {                                      \
    int res = rinit();                      \
    if (res != 0) {                         \
      LOG_ERROR("rinit failed: %d\n", res); \
      exit(res);                            \
    }                                       \
  } while (0)

#define WAR_OBJID_ALLOC(ptr)                       \
  do {                                             \
    ptr = alloc_warbuffer(RAY_TYPE_OBJECT_ID, 32); \
    if (ptr == NULL) {                             \
      LOG_ERROR("alloc_warbuffer failed\n");       \
      exit(-1);                                    \
    }                                              \
  } while (0)

#define WAR_OBJID_FREE(ptr) free_warbuffer(ptr)

#define BUF2FLOAT(buf) (*(float *)buf)

#endif  // __RAY_H__
