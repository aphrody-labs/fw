;; JS Promise Integration: plain synchronous Wasm that calls an import which, on the JS side, is async.
;; The module contains no mention of promises; the embedder wraps the import in WebAssembly.Suspending
;; and the export in WebAssembly.promising, so the call suspends the whole Wasm stack until the promise settles.
(module
  (import "ay" "fetch_word" (func $fetch_word (param i32) (result i32)))

  ;; Sum of fetch_word(0..n): reads like a blocking loop.
  (func (export "total") (param $n i32) (result i32)
    (local $i i32)
    (local $acc i32)
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (local.set $acc (i32.add (local.get $acc) (call $fetch_word (local.get $i))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (local.get $acc)))
