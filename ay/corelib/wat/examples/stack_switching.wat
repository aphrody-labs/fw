;; Stack switching (typed continuations): a task suspends with `suspend` and its caller resumes it later.
;; A cooperative `step()` state machine, as in the corelib's apps (corelib/src/apps.c), is this written by hand; with the
;; proposal the app would keep its own stack across a yield instead.
(module
  (type $step (func (result i32)))
  (type $task (cont $step))
  (tag $yield (param i32))

  (func $body (type $step)
    (suspend $yield (i32.const 1))
    (suspend $yield (i32.const 2))
    (i32.const 7))
  (elem declare func $body)

  ;; Run the task to completion and return the number of times it yielded.
  (func (export "yields") (result i32)
    (local $k (ref null $task))
    (local $n i32)
    (local.set $k (cont.new $task (ref.func $body)))
    (loop $again
      (block $on_yield (result i32 (ref $task))
        (drop (resume $task (on $yield $on_yield) (local.get $k)))
        (return (local.get $n)))
      (local.set $k)
      (drop)
      (local.set $n (i32.add (local.get $n) (i32.const 1)))
      (br $again))
    (unreachable)))
