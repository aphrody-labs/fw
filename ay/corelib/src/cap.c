// Capabilities: unforgeable per-task handles that name a kernel object and carry rights.
#include "core.h"

int32_t ay_cap_get(const ay_cap_table *table, uint32_t handle, uint32_t need, ay_cap *out) {
  if (handle >= AY_MAX_CAPS || table->slots[handle].object == AY_OBJ_NONE) return AY_EBADCAP;
  if (!ay_rights_allow(table->slots[handle].rights, need)) return AY_EPERM;
  *out = table->slots[handle];
  return 0;
}

/// Put `cap` in the first free slot and return its handle.
int32_t ay_cap_install(ay_cap_table *table, ay_cap cap) {
  for (uint32_t i = 0; i < AY_MAX_CAPS; i++) {
    if (table->slots[i].object == AY_OBJ_NONE) {
      table->slots[i] = cap;
      return (int32_t)i;
    }
  }
  return AY_ENOMEM;
}

/// New handle to the same object with `rights & mask`; needs the DERIVE right, so rights only shrink.
int32_t ay_cap_derive(ay_cap_table *table, uint32_t handle, uint32_t mask) {
  ay_cap cap;
  int32_t err = ay_cap_get(table, handle, AY_RIGHT_DERIVE, &cap);
  if (err < 0) return err;
  cap.rights &= mask;
  return ay_cap_install(table, cap);
}

int32_t ay_cap_drop(ay_cap_table *table, uint32_t handle) {
  if (handle >= AY_MAX_CAPS || table->slots[handle].object == AY_OBJ_NONE) return AY_EBADCAP;
  table->slots[handle] = (ay_cap){};
  return 0;
}
