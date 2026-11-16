;; Threads proposal: shared memory plus atomic read-modify-write, wait and notify.
;; The spin lock is ay_try_acquire in corelib/src/sync.c; the futex-style pair parks a thread instead of spinning.
(module
  (import "env" "memory" (memory 1 1 shared))

  ;; ay_try_acquire: flag 0 -> 1 with one atomic compare-and-swap; 1 when this caller got the lock.
  (func (export "try_acquire") (param $flag i32) (result i32)
    (i32.eqz (i32.atomic.rmw.cmpxchg (local.get $flag) (i32.const 0) (i32.const 1))))

  (func (export "release") (param $flag i32)
    (i32.atomic.store (local.get $flag) (i32.const 0)))

  (func (export "add") (param $addr i32) (param $n i32) (result i32)
    (i32.atomic.rmw.add (local.get $addr) (local.get $n)))

  ;; wait(addr, expected, timeout_ns): 0 woken, 1 value differed, 2 timed out.
  (func (export "wait") (param $addr i32) (param $expected i32) (param $timeout i64) (result i32)
    (memory.atomic.wait32 (local.get $addr) (local.get $expected) (local.get $timeout)))

  ;; notify(addr, count): number of waiters woken.
  (func (export "notify") (param $addr i32) (param $count i32) (result i32)
    (memory.atomic.notify (local.get $addr) (local.get $count)))

  (func (export "fence") (atomic.fence)))
