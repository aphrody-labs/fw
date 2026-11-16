;; ay_arg_count written by hand with the control-flow shape LLVM did not pick: a br_table dispatch.
;; Nested blocks are the targets; br_table picks one by index (the last label is the default).
;; Same answer as ay_arg_count in corelib/src/abi.c: syscall numbers 0..10 map to 0..3 arguments.
(module
  (func (export "arg_count") (param $nr i32) (result i32)
    (block $three                       ;; LOG
      (block $two                       ;; MSG_SET, CAP_DERIVE
        (block $one                     ;; MSG_GET, SEND, RECV, CAP_DROP, FRAME_FREE
          (block $zero                  ;; VERSION, YIELD, FRAME_ALLOC and every unknown number
            (br_table $zero $zero $three $two $one $one $one $two $one $zero $one $zero
              (local.get $nr)))
          (return (i32.const 0)))
        (return (i32.const 1)))
      (return (i32.const 2)))
    (i32.const 3))

  ;; No goto: a loop is a `loop` block whose label is at its top; `br` to it jumps backwards.
  ;; countdown(n) returns how many times the body ran.
  (func (export "countdown") (param $n i32) (result i32)
    (local $steps i32)
    (block $exit
      (loop $top
        (br_if $exit (i32.eqz (local.get $n)))
        (local.set $n (i32.sub (local.get $n) (i32.const 1)))
        (local.set $steps (i32.add (local.get $steps) (i32.const 1)))
        (br $top)))
    (local.get $steps)))
