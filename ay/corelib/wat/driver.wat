;; Core module of `world driver`: the platform driver of the Wasm OS as a WASI command.
;; Same loop as host.ts: boot, spawn the boot task, run `step` under the corelib's scheduler.
;; One app only: the single task id spawned is the id of the single `task` instance.
(module
  (import "ay:corelib/scheduler@0.1.0" "boot" (func $boot (result i32)))
  (import "ay:corelib/scheduler@0.1.0" "spawn" (func $spawn (result i32)))
  (import "ay:corelib/scheduler@0.1.0" "sched-next" (func $sched_next (result i32)))
  (import "ay:corelib/scheduler@0.1.0" "sched-report" (func $sched_report (param i32 i32)))
  (import "ay:corelib/task@0.1.0" "step" (func $step (result i32)))

  ;; wasi:cli/run.run returns result<_, _>: 0 is ok, 1 is error.
  (func (export "wasi:cli/run@0.2.12#run") (result i32)
    (local $id i32)
    (if (call $boot) (then (return (i32.const 1))))
    (if (i32.lt_s (call $spawn) (i32.const 0)) (then (return (i32.const 1))))
    (loop $next
      (local.set $id (call $sched_next))
      (if (i32.ge_s (local.get $id) (i32.const 0))
        (then
          (call $sched_report (local.get $id) (call $step))
          (br $next))))
    (i32.const 0)))
