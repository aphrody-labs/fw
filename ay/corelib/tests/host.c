// Hosted entry: run the same boot transcript as a normal process and print it on stdout.
// Proves the corelib builds and behaves the same through the platform C library: gnu driver or clang-cl, ELF, Mach-O or PE.
#include <stdio.h>

#include "core.h"

#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif

static void put(void *, uint8_t byte) { fputc(byte, stdout); }

int main(void) {
#ifdef _WIN32
  _setmode(_fileno(stdout), _O_BINARY);
#endif
  const ay_console con = {put, nullptr};
  return ay_run_reference(&con) ? 0 : 1;
}
