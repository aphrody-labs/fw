// Internal header of the SDK corelib: freestanding C23, no libc. The public ABI is include/ay_abi.h.
#ifndef AY_CORE_H
#define AY_CORE_H

#define AY_CORELIB_IMPL
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "ay_abi.h"

/// Symbols that leave the module: kept and exported (the build links wasm with --export-dynamic).
#define AY_API [[gnu::visibility("default"), gnu::used]]

enum : unsigned {
  AY_MAX_TASKS = 8,
  AY_MAX_CAPS = 8,
  AY_MAX_ENDPOINTS = 4,
  AY_QUEUE_LEN = 8,
  AY_FRAMES = 64,
};

/// Byte sink the platform provides: a UART on bare metal, a host import on WebAssembly.
typedef struct ay_console {
  void (*put)(void *ctx, uint8_t byte);
  void *ctx;
} ay_console;

/// What a task reports after one run of its `step` (the i32 an app module returns).
typedef enum ay_step : int32_t { AY_STEP_DONE = 0, AY_STEP_YIELD = 1, AY_STEP_FAULT = 2 } ay_step;

// out.c: formatted output on a console.
void ay_puts(const ay_console *con, const char *text);
void ay_put_i32(const ay_console *con, int32_t value);
void ay_put_hex32(const ay_console *con, uint32_t value);

// cap.c: capabilities are per-task handles that name an object and carry rights.
typedef enum ay_object : uint8_t { AY_OBJ_NONE = 0, AY_OBJ_CONSOLE, AY_OBJ_ENDPOINT } ay_object;
typedef struct ay_cap {
  ay_object object;
  uint8_t endpoint;
  uint32_t rights;
} ay_cap;
typedef struct ay_cap_table {
  ay_cap slots[AY_MAX_CAPS];
} ay_cap_table;
int32_t ay_cap_get(const ay_cap_table *table, uint32_t handle, uint32_t need, ay_cap *out);
int32_t ay_cap_install(ay_cap_table *table, ay_cap cap);
int32_t ay_cap_derive(ay_cap_table *table, uint32_t handle, uint32_t mask);
int32_t ay_cap_drop(ay_cap_table *table, uint32_t handle);

// ipc.c: bounded queues of four-word messages.
typedef struct ay_msg {
  uint32_t w[AY_MSG_WORDS];
} ay_msg;
typedef struct ay_queue {
  ay_msg buf[AY_QUEUE_LEN];
  uint8_t head, len;
} ay_queue;
int32_t ay_queue_send(ay_queue *queues, uint32_t endpoint, ay_msg msg);
int32_t ay_queue_recv(ay_queue *queues, uint32_t endpoint, ay_msg *out);

// mem.c: 64 frames in a bitmap with per-frame ownership.
typedef struct ay_frames {
  uint64_t bits;
  uint8_t owner[AY_FRAMES];
} ay_frames;
int32_t ay_frames_alloc(ay_frames *frames, uint8_t task);
int32_t ay_frames_free(ay_frames *frames, uint8_t task, uint32_t frame);
uint32_t ay_frames_free_count(const ay_frames *frames);

// engine.c: the task table, the round-robin scheduler and the syscall dispatcher.
typedef enum ay_task_state : uint8_t { AY_TASK_FREE = 0, AY_TASK_READY, AY_TASK_EXITED } ay_task_state;
typedef struct ay_task {
  ay_task_state state;
  ay_cap_table caps;
  ay_msg regs; // message registers: staged by MSG_SET, filled by RECV, read by MSG_GET
} ay_task;
typedef struct ay_core {
  ay_task tasks[AY_MAX_TASKS];
  ay_queue queues[AY_MAX_ENDPOINTS];
  ay_frames frames;
  uint32_t current;
  uint32_t cursor;
} ay_core;
void ay_core_init(ay_core *core);
int32_t ay_core_spawn(ay_core *core, const ay_cap *caps, size_t count);
/// Next ready task, round-robin, made current for syscalls; -1 (after a console line) when none can run.
int32_t ay_core_schedule(ay_core *core, const ay_console *con);
void ay_core_report(ay_core *core, const ay_console *con, uint32_t id, int32_t step_code);
int32_t ay_core_syscall(ay_core *core, const ay_console *con, uint32_t nr, uint32_t a0, uint32_t a1, uint32_t a2);

// apps.c and demo.c: the reference app and the boot transcript.
typedef struct ay_sys {
  int32_t (*call)(void *ctx, uint32_t nr, uint32_t a0, uint32_t a1, uint32_t a2);
  void *ctx;
} ay_sys;
typedef struct ay_hello {
  uint32_t state;
} ay_hello;
ay_step ay_hello_step(ay_hello *app, const ay_sys *sys);

extern const ay_cap ay_boot_caps[2];
bool ay_selftest(const ay_console *con);
bool ay_run_reference(const ay_console *con);

#endif
