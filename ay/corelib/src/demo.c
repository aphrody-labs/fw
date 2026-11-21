// The boot transcript: a self-test of every corelib subsystem plus one app run.
// Bare metal, the WebAssembly corelib and the Bun test must all print tests/golden/boot.txt.
#include "core.h"

/// Capabilities of a boot task: handle 0 the console, handle 1 an endpoint it may send, receive and derive.
const ay_cap ay_boot_caps[2] = {
    {.object = AY_OBJ_CONSOLE, .rights = AY_RIGHT_SEND},
    {.object = AY_OBJ_ENDPOINT, .endpoint = 0, .rights = AY_RIGHT_SEND | AY_RIGHT_RECV | AY_RIGHT_DERIVE},
};


/// Exercise scheduler, frame allocator, capabilities and IPC on a private core; true when every check held.
bool ay_selftest(const ay_console *con) {
  ay_core core;
  ay_core_init(&core);
  bool ok = true;
  ay_puts(con, "ay-corelib abi ");
  ay_put_i32(con, AY_ABI_VERSION);
  ay_puts(con, "\n");

  int32_t a = ay_core_spawn(&core, ay_boot_caps, 2);
  int32_t b = ay_core_spawn(&core, ay_boot_caps, 2);
  if (a < 0 || b < 0) return false;
  int32_t order[4];
  for (int i = 0; i < 4; i++) {
    int32_t id = ay_core_schedule(&core, con);
    if (id < 0) return false;
    order[i] = id;
    ay_core_report(&core, con, (uint32_t)id, AY_STEP_YIELD);
  }
  ok &= order[0] == a && order[1] == b && order[2] == a && order[3] == b;
  ay_puts(con, "sched: order");
  for (int i = 0; i < 4; i++) {
    ay_puts(con, " ");
    ay_put_i32(con, order[i]);
  }
  ay_puts(con, "\n");

  // Task a runs: frame allocator.
  ay_core_schedule(&core, con);
  int32_t frames[3];
  for (int i = 0; i < 3; i++) frames[i] = ay_core_syscall(&core, con, AY_SYS_FRAME_ALLOC, 0, 0, 0);
  int32_t freed = ay_core_syscall(&core, con, AY_SYS_FRAME_FREE, 1, 0, 0);
  int32_t again = ay_core_syscall(&core, con, AY_SYS_FRAME_ALLOC, 0, 0, 0);
  ok &= frames[0] == 0 && frames[1] == 1 && frames[2] == 2 && freed == 0 && again == 1;
  ay_puts(con, "mem: task ");
  ay_put_i32(con, a);
  ay_puts(con, " frames");
  for (int i = 0; i < 3; i++) {
    ay_puts(con, " ");
    ay_put_i32(con, frames[i]);
  }
  ay_puts(con, ", freed 1 (");
  ay_put_i32(con, freed);
  ay_puts(con, "), realloc ");
  ay_put_i32(con, again);
  ay_puts(con, "\n");

  // Task b runs: a foreign free is refused, rights only shrink.
  ay_core_schedule(&core, con);
  int32_t stolen = ay_core_syscall(&core, con, AY_SYS_FRAME_FREE, 0, 0, 0);
  ok &= stolen == AY_EPERM;
  ay_puts(con, "mem: task ");
  ay_put_i32(con, b);
  ay_puts(con, " free of foreign frame 0 -> ");
  ay_put_i32(con, stolen);
  ay_puts(con, ", ");
  ay_put_i32(con, (int32_t)ay_frames_free_count(&core.frames));
  ay_puts(con, " frames free\n");
  int32_t console_derive = ay_core_syscall(&core, con, AY_SYS_CAP_DERIVE, 0, AY_RIGHT_SEND, 0);
  int32_t send_only = ay_core_syscall(&core, con, AY_SYS_CAP_DERIVE, 1, AY_RIGHT_SEND, 0);
  int32_t recv_via_send_only = ay_core_syscall(&core, con, AY_SYS_RECV, (uint32_t)send_only, 0, 0);
  ok &= console_derive == -1 && send_only == 2 && recv_via_send_only == -1;
  ay_puts(con, "cap: derive console ");
  ay_put_i32(con, console_derive);
  ay_puts(con, ", derive endpoint as send-only ");
  ay_put_i32(con, send_only);
  ay_puts(con, ", recv through it ");
  ay_put_i32(con, recv_via_send_only);
  ay_puts(con, "\n");

  // IPC: b sends through the attenuated handle, a receives.
  const uint32_t words[4] = {1, 2, 3, 4};
  for (uint32_t i = 0; i < 4; i++) ay_core_syscall(&core, con, AY_SYS_MSG_SET, i, words[i], 0);
  int32_t sent = ay_core_syscall(&core, con, AY_SYS_SEND, (uint32_t)send_only, 0, 0);
  ay_puts(con, "ipc: task ");
  ay_put_i32(con, b);
  ay_puts(con, " send ");
  ay_put_i32(con, sent);
  ay_puts(con, ", checksum ");
  ay_put_hex32(con, ay_checksum(words, 4));
  ay_puts(con, "\n");
  ay_core_schedule(&core, con);
  int32_t got = ay_core_syscall(&core, con, AY_SYS_RECV, 1, 0, 0);
  uint32_t back[4];
  for (uint32_t i = 0; i < 4; i++) back[i] = (uint32_t)ay_core_syscall(&core, con, AY_SYS_MSG_GET, i, 0, 0);
  int32_t empty = ay_core_syscall(&core, con, AY_SYS_RECV, 1, 0, 0);
  ok &= sent == 0 && got == 0 && ay_checksum(back, 4) == ay_checksum(words, 4) && empty == AY_EAGAIN;
  for (int i = 0; i < 4; i++) ok &= back[i] == words[i];
  ay_puts(con, "ipc: task ");
  ay_put_i32(con, (int32_t)core.current);
  ay_puts(con, " recv ");
  ay_put_i32(con, got);
  ay_puts(con, ", checksum ");
  ay_put_hex32(con, ay_checksum(back, 4));
  ay_puts(con, ", then empty -> ");
  ay_put_i32(con, empty);
  ay_puts(con, "\n");

  // LOG through the console capability.
  ay_puts(con, "log: ");
  ay_core_syscall(&core, con, AY_SYS_LOG, 0, 0x000A6B6Fu, 3); // "ok\n"

  ay_puts(con, ok ? "selftest: ok\n" : "selftest: FAILED\n");
  return ok;
}

/// Syscall access for a task running inside the same image.
typedef struct local {
  ay_core *core;
  const ay_console *console;
} local;

static int32_t local_call(void *ctx, uint32_t nr, uint32_t a0, uint32_t a1, uint32_t a2) {
  local *l = ctx;
  return ay_core_syscall(l->core, l->console, nr, a0, a1, a2);
}

/// The whole demo with the app implemented in C (what the bare-metal image runs).
bool ay_run_reference(const ay_console *con) {
  if (!ay_selftest(con)) return false;
  ay_core core;
  ay_core_init(&core);
  if (ay_core_spawn(&core, ay_boot_caps, 2) < 0) return false;
  local l = {&core, con};
  ay_sys sys = {local_call, &l};
  ay_hello app = {};
  for (int32_t id; (id = ay_core_schedule(&core, con)) >= 0;) {
    ay_core_report(&core, con, (uint32_t)id, ay_hello_step(&app, &sys));
  }
  return true;
}
