// The reference app: the same state machine as wat/hello.wat, written against the syscall ABI.
// The bare-metal image runs this one; the WebAssembly corelib runs the hand-written module.
#include "core.h"

enum : uint32_t { CONSOLE = 0, ENDPOINT = 1 };

/// Print `text` four bytes per LOG syscall, little-endian packed.
static void print(const ay_sys *sys, const char *text, size_t len) {
  for (size_t at = 0; at < len; at += 4) {
    uint32_t n = len - at < 4 ? (uint32_t)(len - at) : 4;
    uint32_t word = 0;
    for (uint32_t i = 0; i < n; i++) word |= (uint32_t)(uint8_t)text[at + i] << (8 * i);
    sys->call(sys->ctx, AY_SYS_LOG, CONSOLE, word, n);
  }
}

#define PRINT(sys, literal) print(sys, literal, sizeof(literal) - 1)

ay_step ay_hello_step(ay_hello *app, const ay_sys *sys) {
  switch (app->state) {
  case 0:
    PRINT(sys, "hello from ay app\n");
    app->state = 1;
    return AY_STEP_YIELD;
  case 1:
    for (uint32_t i = 0; i < 4; i++) sys->call(sys->ctx, AY_SYS_MSG_SET, i, i + 1, 0);
    if (sys->call(sys->ctx, AY_SYS_SEND, ENDPOINT, 0, 0) < 0) return AY_STEP_FAULT;
    app->state = 2;
    return AY_STEP_YIELD;
  default: {
    if (sys->call(sys->ctx, AY_SYS_RECV, ENDPOINT, 0, 0) < 0) return AY_STEP_FAULT;
    int32_t sum = 0;
    for (uint32_t i = 0; i < 4; i++) sum += sys->call(sys->ctx, AY_SYS_MSG_GET, i, 0, 0);
    if (sum == 10) PRINT(sys, "ipc ok\n");
    else PRINT(sys, "ipc bad\n");
    return AY_STEP_DONE;
  }
  }
}
