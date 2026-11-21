;; WasmGC: managed structs and arrays, i31 scalars and subtyping, with no linear memory involved.
(module
  (rec
    (type $shape (sub (struct (field $sides i32))))
    (type $rect (sub $shape (struct (field $sides i32) (field $w i32) (field $h i32)))))
  (type $words (array (mut i32)))

  (func $rect (param $w i32) (param $h i32) (result (ref $rect))
    (struct.new $rect (i32.const 4) (local.get $w) (local.get $h)))

  ;; Upcast is free; the downcast is checked at run time and traps when the value is not a $rect.
  (func (export "area") (param $w i32) (param $h i32) (result i32)
    (local $s (ref $shape))
    (local.set $s (call $rect (local.get $w) (local.get $h)))
    (i32.mul
      (struct.get $rect $w (ref.cast (ref $rect) (local.get $s)))
      (struct.get $rect $h (ref.cast (ref $rect) (local.get $s)))))

  (func (export "sides_of_plain") (result i32)
    (struct.get $shape $sides (struct.new $shape (i32.const 3))))

  (func (export "bad_cast") (result i32)
    (struct.get $rect $w (ref.cast (ref $rect) (struct.new $shape (i32.const 3)))))

  ;; An array of i32 allocated and indexed with bounds checks (out of range traps).
  (func (export "array_sum") (param $n i32) (result i32)
    (local $a (ref $words))
    (local $i i32)
    (local $acc i32)
    (local.set $a (array.new $words (i32.const 7) (local.get $n)))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (local.set $acc (i32.add (local.get $acc) (array.get $words (local.get $a) (local.get $i))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (local.get $acc))

  ;; i31ref: a 31-bit integer stored in the reference itself, no allocation.
  (func (export "i31_roundtrip") (param $x i32) (result i32)
    (i31.get_s (ref.i31 (local.get $x))))

  (func (export "is_i31") (param $x i32) (result i32)
    (ref.test (ref i31) (ref.i31 (local.get $x)))))
