;; A syscall shim: typed wrappers over the one raw `ay.syscall` import, plus a funcref table so a
;; caller can pick a wrapper at run time (call_indirect is type-checked: a wrong signature traps).
(module
  (import "ay" "syscall" (func $sys (param i32 i32 i32 i32) (result i32)))

  (type $nullary (func (result i32)))

  (func $version (type $nullary) (call $sys (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0)))
  (func $yield (type $nullary)   (call $sys (i32.const 1) (i32.const 0) (i32.const 0) (i32.const 0)))
  (func $frame_alloc (type $nullary) (call $sys (i32.const 9) (i32.const 0) (i32.const 0) (i32.const 0)))

  (func $log (param $cap i32) (param $word i32) (param $len i32) (result i32)
    (call $sys (i32.const 2) (local.get $cap) (local.get $word) (local.get $len)))

  (table $nullary_calls funcref (elem $version $yield $frame_alloc))

  (func $invoke (param $slot i32) (result i32)
    (call_indirect $nullary_calls (type $nullary) (local.get $slot)))

  (export "version" (func $version))
  (export "yield" (func $yield))
  (export "frame_alloc" (func $frame_alloc))
  (export "log" (func $log))
  (export "invoke" (func $invoke)))
