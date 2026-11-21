;; Fixed-width SIMD: one v128 holds four i32 lanes. Same job as ay_sum: wrapping sum of n words.
(module
  (memory (export "memory") 1)

  ;; sum4(ptr): add the four lanes loaded at ptr into one scalar.
  (func (export "sum4") (param $ptr i32) (result i32)
    (local $v v128)
    (local.set $v (v128.load (local.get $ptr)))
    (i32.add
      (i32.add (i32x4.extract_lane 0 (local.get $v)) (i32x4.extract_lane 1 (local.get $v)))
      (i32.add (i32x4.extract_lane 2 (local.get $v)) (i32x4.extract_lane 3 (local.get $v)))))

  ;; sum(ptr, n): n words, 4 per iteration in a vector accumulator, n a multiple of 4.
  (func (export "sum") (param $ptr i32) (param $n i32) (result i32)
    (local $acc v128)
    (local $end i32)
    (local.set $end (i32.add (local.get $ptr) (i32.shl (local.get $n) (i32.const 2))))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $ptr) (local.get $end)))
        (local.set $acc (i32x4.add (local.get $acc) (v128.load (local.get $ptr))))
        (local.set $ptr (i32.add (local.get $ptr) (i32.const 16)))
        (br $next)))
    (i32.add
      (i32.add (i32x4.extract_lane 0 (local.get $acc)) (i32x4.extract_lane 1 (local.get $acc)))
      (i32.add (i32x4.extract_lane 2 (local.get $acc)) (i32x4.extract_lane 3 (local.get $acc)))))

  ;; A v128 constant and a lane-wise compare: bitmask packs one bit per lane.
  (func (export "any_equal") (param $x i32) (result i32)
    (i32x4.bitmask (i32x4.eq (i32x4.splat (local.get $x)) (v128.const i32x4 10 20 30 40)))))
