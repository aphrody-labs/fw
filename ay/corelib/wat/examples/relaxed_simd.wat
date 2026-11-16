;; Relaxed SIMD: operations whose result is implementation-defined for some inputs.
;; The module is valid wherever the feature exists, but results may differ between engines and CPUs.
;; Each function reads vectors at ptr, ptr+16, ptr+32 and stores its result at ptr+48.
(module
  (memory (export "memory") 1)

  ;; Fused multiply-add a*b+c: rounded once (fused) or twice (separate multiply and add), at the engine's choice.
  (func (export "madd") (param $p i32)
    (v128.store offset=48 (local.get $p)
      (f32x4.relaxed_madd
        (v128.load (local.get $p))
        (v128.load offset=16 (local.get $p))
        (v128.load offset=32 (local.get $p)))))

  ;; Bitwise select: bit-exact when every mask byte (at ptr+32) is all ones or all zeros,
  ;; implementation-defined for any other mask byte.
  (func (export "laneselect") (param $p i32)
    (v128.store offset=48 (local.get $p)
      (i8x16.relaxed_laneselect
        (v128.load (local.get $p))
        (v128.load offset=16 (local.get $p))
        (v128.load offset=32 (local.get $p)))))

  ;; Float to int: in-range lanes are exact; NaN and out-of-range lanes are implementation-defined.
  (func (export "trunc") (param $p i32)
    (v128.store offset=48 (local.get $p)
      (i32x4.relaxed_trunc_f32x4_s (v128.load (local.get $p))))))
