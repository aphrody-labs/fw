// The one synchronisation primitive the corelib needs: a lock word taken 0 -> 1 with compare-and-swap.
#include <stdatomic.h>

#include "core.h"

/// Take the lock word 0 -> 1; true when this caller now holds it.
AY_API uint32_t ay_try_acquire(uint32_t *flag) {
  uint32_t expected = 0;
  return atomic_compare_exchange_strong_explicit((_Atomic uint32_t *)flag, &expected, 1u, memory_order_acquire,
                                                 memory_order_relaxed);
}
