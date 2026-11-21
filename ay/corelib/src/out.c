#include "core.h"

void ay_puts(const ay_console *con, const char *text) {
  for (; *text; text++) con->put(con->ctx, (uint8_t)*text);
}

void ay_put_i32(const ay_console *con, int32_t value) {
  char digits[10];
  uint32_t magnitude = value < 0 ? 0u - (uint32_t)value : (uint32_t)value;
  size_t n = 0;
  do {
    digits[n++] = (char)('0' + magnitude % 10);
    magnitude /= 10;
  } while (magnitude);
  if (value < 0) con->put(con->ctx, '-');
  while (n) con->put(con->ctx, (uint8_t)digits[--n]);
}

/// 0x followed by eight lowercase hex digits.
void ay_put_hex32(const ay_console *con, uint32_t value) {
  ay_puts(con, "0x");
  for (int shift = 28; shift >= 0; shift -= 4) con->put(con->ctx, (uint8_t)"0123456789abcdef"[(value >> shift) & 0xF]);
}
