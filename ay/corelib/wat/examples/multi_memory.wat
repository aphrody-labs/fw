;; Multiple memories: memory 0 is the "kernel" (corelib) memory, memory 1 an app's private memory.
;; Every load, store and bulk operation names its memory; memory.copy moves bytes across the boundary.
(module
  (memory $kernel (export "kernel") 1)
  (memory $app (export "app") 1)
  (data $greeting (memory $app) (i32.const 0) "hello from ay app\n")

  ;; The kernel copies a message out of the app's memory into its own, after a bounds check of its choosing.
  (func (export "copy_in") (param $src i32) (param $dst i32) (param $len i32)
    (memory.copy $kernel $app (local.get $dst) (local.get $src) (local.get $len)))

  (func (export "kernel_byte") (param $addr i32) (result i32)
    (i32.load8_u $kernel (local.get $addr)))

  (func (export "poke_app") (param $addr i32) (param $v i32)
    (i32.store8 $app (local.get $addr) (local.get $v)))

  (func (export "grow_app") (param $pages i32) (result i32)
    (memory.grow $app (local.get $pages)))

  (func (export "sizes") (result i32 i32)
    (memory.size $kernel) (memory.size $app)))
