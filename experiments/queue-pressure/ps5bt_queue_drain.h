/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5BT_QUEUE_DRAIN_H
#define PS5BT_QUEUE_DRAIN_H

#include <stdint.h>

#define PS5BT_QUEUE_DRAIN_ABI_VERSION 1u
#define PS5BT_QUEUE_DRAIN_MAX_MS 500
#define PS5BT_QUEUE_DRAIN_ROOM 2u

/* Single-owner advisory state. No queue storage, I/O or credit authority.
 * Call before admission and on pump progress. The caller supplies monotonic
 * milliseconds and applies the returned cap. Repeated/changed shrink requests
 * share one deadline until applied or explicitly cancelled by a non-shrink. */
typedef struct {
  uint32_t abi_version;
  int64_t last_ms, since_ms;
  int clock_known, pending;
} ps5bt_queue_drain;

enum ps5bt_queue_drain_reason {
  PS5BT_DRAIN_KEEP,
  PS5BT_DRAIN_WAIT,
  PS5BT_DRAIN_APPLY,
  PS5BT_DRAIN_TIMEOUT,
  PS5BT_DRAIN_CANCEL,
  PS5BT_DRAIN_CLOCK,
  PS5BT_DRAIN_INVALID
};

typedef struct {
  unsigned cap;
  enum ps5bt_queue_drain_reason reason;
  int64_t waited_ms;
} ps5bt_queue_drain_result;

static inline void ps5bt_queue_drain_init(ps5bt_queue_drain *s) {
  *s = (ps5bt_queue_drain){0};
  s->abi_version = PS5BT_QUEUE_DRAIN_ABI_VERSION;
}

static inline ps5bt_queue_drain_result
ps5bt_queue_drain_step(ps5bt_queue_drain *s, int64_t now, unsigned current,
                       unsigned requested, unsigned queued) {
  ps5bt_queue_drain_result r = {current, PS5BT_DRAIN_KEEP, 0};
  if (!s || s->abi_version != PS5BT_QUEUE_DRAIN_ABI_VERSION ||
      current < PS5BT_QUEUE_DRAIN_ROOM || requested < PS5BT_QUEUE_DRAIN_ROOM) {
    r.reason = PS5BT_DRAIN_INVALID;
    return r;
  }
  /* A bad clock must not extend deferred capacity indefinitely. */
  if (now < 0 || (s->clock_known && now < s->last_ms)) {
    s->pending = 0;
    s->clock_known = now >= 0;
    s->last_ms = now >= 0 ? now : 0;
    r.cap = requested;
    r.reason = PS5BT_DRAIN_CLOCK;
    return r;
  }
  s->clock_known = 1;
  s->last_ms = now;
  if (s->pending)
    r.waited_ms = now - s->since_ms;
  if (requested >= current) {
    r.reason = s->pending ? PS5BT_DRAIN_CANCEL : PS5BT_DRAIN_KEEP;
    r.cap = requested;
    s->pending = 0;
  } else if (queued <= requested - PS5BT_QUEUE_DRAIN_ROOM) {
    r.cap = requested;
    r.reason = PS5BT_DRAIN_APPLY;
    s->pending = 0;
  } else if (!s->pending) {
    s->pending = 1;
    s->since_ms = now;
    r.reason = PS5BT_DRAIN_WAIT;
  } else if (r.waited_ms >= PS5BT_QUEUE_DRAIN_MAX_MS) {
    r.cap = requested;
    r.reason = PS5BT_DRAIN_TIMEOUT;
    s->pending = 0;
  }
  return r;
}

#endif
