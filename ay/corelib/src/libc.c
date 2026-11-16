// The slice of libc a freestanding build still calls: the compiler lowers struct copies and zeroing to these.
#include <stddef.h>

void *memset(void *dest, int byte, size_t n) {
  unsigned char *d = dest;
  while (n--) *d++ = (unsigned char)byte;
  return dest;
}

void *memcpy(void *restrict dest, const void *restrict src, size_t n) {
  unsigned char *d = dest;
  const unsigned char *s = src;
  while (n--) *d++ = *s++;
  return dest;
}

void *memmove(void *dest, const void *src, size_t n) {
  unsigned char *d = dest;
  const unsigned char *s = src;
  if (d < s) {
    while (n--) *d++ = *s++;
  } else {
    d += n;
    s += n;
    while (n--) *--d = *--s;
  }
  return dest;
}

int memcmp(const void *a, const void *b, size_t n) {
  const unsigned char *x = a, *y = b;
  for (; n; n--, x++, y++) {
    if (*x != *y) return *x - *y;
  }
  return 0;
}
