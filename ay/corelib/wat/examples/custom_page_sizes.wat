;; Custom page sizes: a memory may declare a page of 1 byte instead of 64 KiB, so its size is exact.
;; This is the form for tiny embedded targets: a 1 KiB scratch memory without wasting a 64 KiB page.
(module
  (memory (export "memory") 1024 (pagesize 1))
  (data (i32.const 0) "ay")

  (func (export "size") (result i32) (memory.size))
  (func (export "byte") (param $addr i32) (result i32) (i32.load8_u (local.get $addr))))
