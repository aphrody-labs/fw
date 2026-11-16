;; One function per trap kind, plus a try_table with catch_all to show a trap is not an exception.
(module
  (memory 1)
  (func (export "unreachable") (unreachable))
  (func (export "div") (param i32 i32) (result i32) (i32.div_s (local.get 0) (local.get 1)))
  (func (export "trunc") (param f32) (result i32) (i32.trunc_f32_s (local.get 0)))
  (func (export "load") (param i32) (result i32) (i32.load (local.get 0)))
  (func (export "load_offset") (param i32) (result i32) (i32.load offset=0xffffffff (local.get 0)))
  (func $recurse (export "recurse") (result i32) (i32.add (i32.const 1) (call $recurse)))

  (func (export "catch_all_does_not_catch_traps") (result i32)
    (block $caught
      (try_table (catch_all $caught)
        (unreachable))
      (return (i32.const 0)))
    (i32.const 1)))
