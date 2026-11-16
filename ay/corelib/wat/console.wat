;; Core module of `world console`: ay:corelib/host@0.1.0.console-put over WASI stdout.
;; Canonical ABI: a `list<u8>` argument is (pointer, length) in linear memory; the `result<_, stream-error>`
;; return value is written through a trailing pointer.
(module
  (import "wasi:cli/stdout@0.2.12" "get-stdout" (func $get_stdout (result i32)))
  (import "wasi:io/streams@0.2.12" "[method]output-stream.blocking-write-and-flush"
    (func $write (param i32 i32 i32 i32)))

  (memory (export "memory") 1)
  (global $out (mut i32) (i32.const -1))   ;; the stream handle, fetched on first use

  (func (export "ay:corelib/host@0.1.0#console-put") (param $byte i32)
    (if (i32.lt_s (global.get $out) (i32.const 0))
      (then (global.set $out (call $get_stdout))))
    (i32.store8 (i32.const 0) (local.get $byte))
    (call $write (global.get $out) (i32.const 0) (i32.const 1) (i32.const 16))))   ;; result at 16
