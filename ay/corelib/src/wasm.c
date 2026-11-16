// WebAssembly platform layer. Host contract:
//   import  ay_host.console_put(byte)   the serial port of this machine
//   exports ay_boot, ay_spawn, ay_sched_next, ay_sched_report, ay_syscall   the platform driver
//   apps are separate modules importing ay.syscall (the host wires it to ay_syscall) and exporting step.
// Built with -DAY_COMPONENT the same functions carry the canonical-ABI names of wit/ay-corelib.wit instead.
#include "core.h"

#ifdef AY_COMPONENT
#define IMPORT_MODULE "ay:corelib/host@0.1.0"
#define IMPORT_NAME "console-put"
#define NAME(plain, canonical) canonical
#else
#define IMPORT_MODULE "ay_host"
#define IMPORT_NAME "console_put"
#define NAME(plain, canonical) plain
#endif
#define EXPORT(plain, canonical) [[clang::export_name(NAME(plain, canonical))]]

[[clang::import_module(IMPORT_MODULE), clang::import_name(IMPORT_NAME)]] extern void host_console_put(uint32_t byte);

static void put(void *, uint8_t byte) { host_console_put(byte); }

static const ay_console host = {put, nullptr};
static ay_core core = {.cursor = AY_MAX_TASKS - 1};

/// Run the boot self-test; 0 when every check held.
EXPORT("ay_boot", "ay:corelib/scheduler@0.1.0#boot") int32_t ay_boot(void) { return ay_selftest(&host) ? 0 : -1; }

/// Create the boot task; its id or a negative error.
EXPORT("ay_spawn", "ay:corelib/scheduler@0.1.0#spawn") int32_t ay_spawn(void) {
  return ay_core_spawn(&core, ay_boot_caps, 2);
}

/// Next task to run, or -1 when none is runnable.
EXPORT("ay_sched_next", "ay:corelib/scheduler@0.1.0#sched-next") int32_t ay_sched_next(void) {
  return ay_core_schedule(&core, &host);
}

/// Report what `step` returned for task `id` (0 done, 1 yield, 2 fault).
EXPORT("ay_sched_report", "ay:corelib/scheduler@0.1.0#sched-report") void ay_sched_report(uint32_t id, int32_t code) {
  ay_core_report(&core, &host, id, code);
}

/// The syscall ABI: what an app's `ay.syscall` import resolves to.
EXPORT("ay_syscall", "ay:corelib/syscall@0.1.0#call")
int32_t ay_syscall(uint32_t nr, uint32_t a0, uint32_t a1, uint32_t a2) {
  return ay_core_syscall(&core, &host, nr, a0, a1, a2);
}

#ifndef AY_COMPONENT
static uint32_t scratch[16];

/// Address of a 16-word buffer in this module's memory, for host calls that take a pointer (ay_checksum, ay_sum).
EXPORT("ay_scratch", "") uint32_t *ay_scratch(void) { return scratch; }
#endif
