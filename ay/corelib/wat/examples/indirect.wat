;; call_indirect checks the callee's type at the call, not at link time: the table is untyped data.
(module
  (type $nullary (func (result i32)))
  (type $unary (func (param i32) (result i32)))

  (func $seven (type $nullary) (i32.const 7))
  (func $double (type $unary) (i32.mul (local.get 0) (i32.const 2)))
  (table $slots funcref (elem $seven $double))

  ;; slot 0 matches the expected type; slot 1 has another type and traps; slot 2 is out of bounds and traps.
  (func (export "call_nullary") (param $slot i32) (result i32)
    (call_indirect $slots (type $nullary) (local.get $slot))))
