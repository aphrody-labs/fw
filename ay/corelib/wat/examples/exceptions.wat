;; Exception handling, final form: a tag declares the payload, throw raises, try_table catches.
;; This is the form standardised in Wasm 3.0; the older `try`/`catch`/`delegate` encoding is a different feature.
(module
  (tag $overflow (param i32))

  (func $checked_add (param $a i32) (param $b i32) (result i32)
    (local $sum i64)
    (local.set $sum (i64.add (i64.extend_i32_s (local.get $a)) (i64.extend_i32_s (local.get $b))))
    (if (i64.ne (local.get $sum) (i64.extend_i32_s (i32.wrap_i64 (local.get $sum))))
      (then (throw $overflow (local.get $a))))
    (i32.wrap_i64 (local.get $sum)))

  ;; Returns the sum, or -1 when the add overflowed; the catch branch receives the tag's payload.
  (func (export "add_or_minus_one") (param $a i32) (param $b i32) (result i32)
    (block $caught (result i32)
      (try_table (result i32) (catch $overflow $caught)
        (call $checked_add (local.get $a) (local.get $b)))
      (return))
    (drop)
    (i32.const -1))

  ;; An exception nobody catches leaves the instance as a WebAssembly.Exception.
  (func (export "uncaught") (throw $overflow (i32.const 99))))
