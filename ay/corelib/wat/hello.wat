;; The boot app, hand-written. Same state machine as ay_hello_step in corelib/src/apps.c.
;; ABI: ay.syscall(nr, a0, a1, a2) -> i32. Capability handle 0 is the console, 1 an endpoint.
(module
  (import "ay" "syscall" (func $sys (param i32 i32 i32 i32) (result i32)))

  (memory (export "memory") 1)
  (data (i32.const 0)  "hello from ay app\n")   ;; 18 bytes
  (data (i32.const 32) "ipc ok\n")               ;; 7 bytes
  (data (i32.const 48) "ipc bad\n")              ;; 8 bytes

  (global $state (mut i32) (i32.const 0))

  ;; print(ptr, len): one LOG syscall (nr 2) per little-endian 4-byte word.
  (func $print (param $ptr i32) (param $len i32)
    (local $n i32)
    (block $done
      (loop $next
        (br_if $done (i32.eqz (local.get $len)))
        (local.set $n (select (local.get $len) (i32.const 4) (i32.lt_u (local.get $len) (i32.const 4))))
        (drop (call $sys (i32.const 2) (i32.const 0) (i32.load align=1 (local.get $ptr)) (local.get $n)))
        (local.set $ptr (i32.add (local.get $ptr) (i32.const 4)))
        (local.set $len (i32.sub (local.get $len) (local.get $n)))
        (br $next))))

  ;; step() -> 0 done, 1 yield, 2 fault
  (func (export "step") (result i32)
    (local $i i32)
    (local $sum i32)
    (if (i32.eqz (global.get $state))
      (then
        (call $print (i32.const 0) (i32.const 18))
        (global.set $state (i32.const 1))
        (return (i32.const 1))))
    (if (i32.eq (global.get $state) (i32.const 1))
      (then
        (loop $set                                   ;; MSG_SET(i, i + 1) for i in 0..4
          (drop (call $sys (i32.const 3) (local.get $i) (i32.add (local.get $i) (i32.const 1)) (i32.const 0)))
          (br_if $set (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (i32.const 4))))
        (if (i32.lt_s (call $sys (i32.const 5) (i32.const 1) (i32.const 0) (i32.const 0)) (i32.const 0))   ;; SEND
          (then (return (i32.const 2))))
        (global.set $state (i32.const 2))
        (return (i32.const 1))))
    (if (i32.lt_s (call $sys (i32.const 6) (i32.const 1) (i32.const 0) (i32.const 0)) (i32.const 0))       ;; RECV
      (then (return (i32.const 2))))
    (loop $get                                       ;; sum of MSG_GET(i)
      (local.set $sum (i32.add (local.get $sum) (call $sys (i32.const 4) (local.get $i) (i32.const 0) (i32.const 0))))
      (br_if $get (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (i32.const 4))))
    (if (i32.eq (local.get $sum) (i32.const 10))
      (then (call $print (i32.const 32) (i32.const 7)))
      (else (call $print (i32.const 48) (i32.const 8))))
    (i32.const 0)))
