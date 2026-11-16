// The corelib proper: task table, round-robin scheduler and the syscall dispatcher.
#include "core.h"

void ay_core_init(ay_core *core) {
  *core = (ay_core){};
  core->cursor = AY_MAX_TASKS - 1;
}

/// Create a task holding `caps` (handle i = caps[i]); returns its id.
int32_t ay_core_spawn(ay_core *core, const ay_cap *caps, size_t count) {
  for (uint32_t id = 0; id < AY_MAX_TASKS; id++) {
    if (core->tasks[id].state != AY_TASK_FREE) continue;
    ay_task task = {.state = AY_TASK_READY};
    for (size_t i = 0; i < count; i++) {
      int32_t handle = ay_cap_install(&task.caps, caps[i]);
      if (handle < 0) return handle;
    }
    core->tasks[id] = task;
    return (int32_t)id;
  }
  return AY_ENOMEM;
}

int32_t ay_core_schedule(ay_core *core, const ay_console *con) {
  for (uint32_t i = 1; i <= AY_MAX_TASKS; i++) {
    uint32_t id = (core->cursor + i) % AY_MAX_TASKS;
    if (core->tasks[id].state == AY_TASK_READY) {
      core->cursor = id;
      core->current = id;
      return (int32_t)id;
    }
  }
  ay_puts(con, "core: idle, no runnable task\n");
  return -1;
}

/// Record the outcome of a task's step; a finished or faulted task is retired.
void ay_core_report(ay_core *core, const ay_console *con, uint32_t id, int32_t step_code) {
  if (id >= AY_MAX_TASKS || step_code == AY_STEP_YIELD) return;
  core->tasks[id].state = AY_TASK_EXITED;
  ay_puts(con, "core: task ");
  ay_put_i32(con, (int32_t)id);
  ay_puts(con, step_code == AY_STEP_DONE ? " exited\n" : " faulted\n");
}

/// The single entry point of the syscall ABI, on behalf of the current task.
int32_t ay_core_syscall(ay_core *core, const ay_console *con, uint32_t nr, uint32_t a0, uint32_t a1, uint32_t a2) {
  uint8_t id = (uint8_t)core->current;
  ay_task *task = &core->tasks[core->current];
  ay_cap cap;
  int32_t err;
  switch (nr) {
  case AY_SYS_VERSION:
    return AY_ABI_VERSION;
  case AY_SYS_YIELD:
    return 0;
  case AY_SYS_LOG:
    if ((err = ay_cap_get(&task->caps, a0, AY_RIGHT_SEND, &cap)) < 0) return err;
    if (cap.object != AY_OBJ_CONSOLE) return AY_EBADCAP;
    if (a2 < 1 || a2 > 4) return AY_EINVAL;
    for (uint32_t i = 0; i < a2; i++) con->put(con->ctx, (uint8_t)(a1 >> (8 * i)));
    return (int32_t)a2;
  case AY_SYS_MSG_SET:
    if (a0 >= AY_MSG_WORDS) return AY_EINVAL;
    task->regs.w[a0] = a1;
    return 0;
  case AY_SYS_MSG_GET:
    if (a0 >= AY_MSG_WORDS) return AY_EINVAL;
    return (int32_t)task->regs.w[a0];
  case AY_SYS_SEND:
    if ((err = ay_cap_get(&task->caps, a0, AY_RIGHT_SEND, &cap)) < 0) return err;
    if (cap.object != AY_OBJ_ENDPOINT) return AY_EBADCAP;
    return ay_queue_send(core->queues, cap.endpoint, task->regs);
  case AY_SYS_RECV:
    if ((err = ay_cap_get(&task->caps, a0, AY_RIGHT_RECV, &cap)) < 0) return err;
    if (cap.object != AY_OBJ_ENDPOINT) return AY_EBADCAP;
    return ay_queue_recv(core->queues, cap.endpoint, &task->regs);
  case AY_SYS_CAP_DERIVE:
    return ay_cap_derive(&task->caps, a0, a1);
  case AY_SYS_CAP_DROP:
    return ay_cap_drop(&task->caps, a0);
  case AY_SYS_FRAME_ALLOC:
    return ay_frames_alloc(&core->frames, id);
  case AY_SYS_FRAME_FREE:
    return ay_frames_free(&core->frames, id, a0);
  default:
    return AY_ENOSYS;
  }
}
