;; Tail calls: return_call reuses the caller's frame, so a loop written as recursion runs in constant stack.
(module
  (func $go (param $n i32) (param $acc i64) (result i64)
    (if (result i64) (i32.eqz (local.get $n))
      (then (local.get $acc))
      (else (return_call $go (i32.sub (local.get $n) (i32.const 1)) (i64.add (local.get $acc) (i64.extend_i32_u (local.get $n)))))))

  (func (export "sum_to") (param $n i32) (result i64)
    (return_call $go (local.get $n) (i64.const 0)))

  ;; return_call_indirect: the dispatch loop of a cooperative scheduler needs no growing stack either.
  (type $step (func (param i32) (result i32)))
  (table $steps funcref (elem $even $odd))
  (func $even (type $step) (if (result i32) (i32.eqz (local.get 0)) (then (i32.const 1)) (else (return_call_indirect $steps (type $step) (i32.sub (local.get 0) (i32.const 1)) (i32.const 1)))))
  (func $odd (type $step) (if (result i32) (i32.eqz (local.get 0)) (then (i32.const 0)) (else (return_call_indirect $steps (type $step) (i32.sub (local.get 0) (i32.const 1)) (i32.const 0)))))
  (func (export "is_even") (param i32) (result i32) (return_call $even (local.get 0))))
