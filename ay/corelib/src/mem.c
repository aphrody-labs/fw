// Physical frame allocator: a 64-bit bitmap with per-frame ownership.
// A frame is the corelib's unit of memory; on WebAssembly it is carved out of linear memory.
#include "core.h"

/// Index of the lowest clear bit, or -1 when all 64 are set: the first free frame.
AY_API int32_t ay_first_free(uint64_t bits) {
  if (!~bits) return -1;
  return __builtin_ctzll(~bits);
}

int32_t ay_frames_alloc(ay_frames *frames, uint8_t task) {
  int32_t frame = ay_first_free(frames->bits);
  if (frame < 0) return AY_ENOMEM;
  frames->bits |= 1ull << frame;
  frames->owner[frame] = task;
  return frame;
}

/// Only the owner may free a frame.
int32_t ay_frames_free(ay_frames *frames, uint8_t task, uint32_t frame) {
  if (frame >= AY_FRAMES || !((frames->bits >> frame) & 1)) return AY_EINVAL;
  if (frames->owner[frame] != task) return AY_EPERM;
  frames->bits &= ~(1ull << frame);
  return 0;
}

uint32_t ay_frames_free_count(const ay_frames *frames) { return (uint32_t)__builtin_popcountll(~frames->bits); }
