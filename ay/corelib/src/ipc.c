// IPC message bus: fixed-size four-word messages through bounded endpoint queues.
#include "core.h"

/// Rotate-and-add checksum of a message; the loop shows how each ISA spells a rotate.
AY_API uint32_t ay_checksum(const uint32_t *words, size_t len) {
  uint32_t acc = 0x811C9DC5u;
  for (size_t i = 0; i < len; i++) {
    acc = (((acc << 5) | (acc >> 27)) + words[i]) ^ 0x9E3779B9u;
  }
  return acc;
}

/// Wrapping sum of words: the loop a compiler turns into SIMD when the target has vector registers.
AY_API uint32_t ay_sum(const uint32_t *words, size_t len) {
  uint32_t acc = 0;
  for (size_t i = 0; i < len; i++) acc += words[i];
  return acc;
}

int32_t ay_queue_send(ay_queue *queues, uint32_t endpoint, ay_msg msg) {
  if (endpoint >= AY_MAX_ENDPOINTS) return AY_EINVAL;
  ay_queue *q = &queues[endpoint];
  if (q->len == AY_QUEUE_LEN) return AY_EAGAIN;
  q->buf[(q->head + q->len) % AY_QUEUE_LEN] = msg;
  q->len++;
  return 0;
}

int32_t ay_queue_recv(ay_queue *queues, uint32_t endpoint, ay_msg *out) {
  if (endpoint >= AY_MAX_ENDPOINTS) return AY_EINVAL;
  ay_queue *q = &queues[endpoint];
  if (q->len == 0) return AY_EAGAIN;
  *out = q->buf[q->head];
  q->head = (q->head + 1) % AY_QUEUE_LEN;
  q->len--;
  return 0;
}
