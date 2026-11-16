;; Memory64: the memory index type is i64, so addresses, sizes and memory.grow take and return i64.
;; Values (i32/i64 data) are unchanged; only address operands widen.
(module
  (memory (export "memory") i64 1 4)
  (data (i64.const 0) "hello from ay app\n")

  (func (export "load8") (param $addr i64) (result i32)
    (i32.load8_u (local.get $addr)))

  (func (export "store32") (param $addr i64) (param $v i32)
    (i32.store (local.get $addr) (local.get $v)))

  (func (export "size") (result i64) (memory.size))
  (func (export "grow") (param $pages i64) (result i64) (memory.grow (local.get $pages)))

  ;; An address above 4 GiB is expressible but out of bounds for a 1-page memory: the load traps.
  (func (export "far") (result i32) (i32.load8_u (i64.const 0x1_0000_0000))))
