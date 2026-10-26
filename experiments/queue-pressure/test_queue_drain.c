/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Include the real experimental queue to exercise admission and drain without
 * production test hooks. Compile against the patched reference and its deps. */
#include "btlink.c"
#include <assert.h>
#include <limits.h>

static long test_ms;
static unsigned sent_count;
static unsigned char sent_ids[16];
static long clock_value(void) { return test_ms; }
void log_line(const char *fmt, ...) { (void)fmt; }
static int send_packet(void *ctx, const unsigned char *packet, int bytes) {
  (void)ctx;
  assert(bytes == 9 && sent_count < sizeof sent_ids);
  sent_ids[sent_count++] = packet[8];
  return 1;
}
static const hci_ops test_ops = {.acl_send = send_packet};

static btlink *filled_queue(void) {
  btlink *l = btlink_create((hci_t){NULL, &test_ops}, 1021, 7);
  assert(l);
  test_ms = 100;
  l->connected = 1;
  l->pool.limit = 0; /* Fake transport initially grants no sends. */
  l->tx_is_media = 1;
  btlink_set_media_cap(l, 10);
  for (unsigned char i = 0; i < 8; ++i)
    assert(l2_send_raw(l, 0x42, &i, 1));
  assert(l->txq_media == 8 && l->pool.dropped == 0);
  return l;
}

static void real_queue_regression(void) {
  const unsigned char next = 8;
  btlink *old = filled_queue();
  /* Reproduce the previous immediate setter: next admission loses packet 0. */
  old->media_cap = old->media_requested_cap = 8;
  assert(l2_send_raw(old, 0x42, &next, 1));
  assert(old->pool.media_dropped == 1 && old->txq_media == 8);
  assert(old->txq[old->txq_head].d[8] == 1);
  free(old); /* Fake link: do not issue hardware disconnect commands. */

  btlink *l = filled_queue();
  btlink_set_media_cap(l, 8);
  assert(l->media_cap == 10 && l->media_drain.pending);
  assert(l2_send_raw(l, 0x42, &next, 1));
  assert(l->pool.media_dropped == 0 && l->txq_media == 9);
  assert(l->pool.sent == 0 && l->pool.cred_fb == 0);
  test_ms = 120;
  sent_count = 0;
  l->pool.limit = 3; /* Exactly three known fake transport allowances. */
  tx_flush(l);
  assert(sent_count == 3 && sent_ids[0] == 0 && sent_ids[1] == 1 &&
         sent_ids[2] == 2);
  assert(l->txq_media == 6 && l->media_cap == 8 && !l->media_drain.pending);
  assert(l->pool.outstanding == 3 && l->pool.cred_fb == 0);
  assert(l2_send_raw(l, 0x42, &next, 1));
  assert(l->txq_media == 7 && l->pool.media_dropped == 0);
  free(l);

  l = filled_queue();
  btlink_set_media_cap(l, 8);
  test_ms = 599;
  btlink_set_media_cap(l,
                       8); /* Repeated tuner requests cannot extend deadline. */
  assert(l->media_cap == 10);
  test_ms = 600;
  assert(l2_send_raw(l, 0x42, &next, 1));
  assert(l->media_cap == 8 && !l->media_drain.pending);
  assert(l->pool.media_dropped ==
         1); /* Honest fallback, not hidden buffering. */
  assert(l->pool.sent == 0 && l->pool.cred_fb == 0);
  free(l);
}

static void policy_bounds(void) {
  ps5bt_queue_drain s;
  ps5bt_queue_drain_result r;
  ps5bt_queue_drain_init(&s);
  r = ps5bt_queue_drain_step(&s, 100, 12, 10, 10);
  assert(r.reason == PS5BT_DRAIN_WAIT && r.cap == 12);
  r = ps5bt_queue_drain_step(&s, 599, 12, 8, 9);
  assert(r.cap == 12 && r.waited_ms == 499);
  r = ps5bt_queue_drain_step(&s, 600, 12, 8, 9);
  assert(r.reason == PS5BT_DRAIN_TIMEOUT && r.cap == 8 && !s.pending);
  r = ps5bt_queue_drain_step(&s, 601, 8, 8, 9);
  assert(r.reason == PS5BT_DRAIN_KEEP && !s.pending);
  r = ps5bt_queue_drain_step(&s, 602, 8, 6, 6);
  assert(r.reason == PS5BT_DRAIN_WAIT);
  r = ps5bt_queue_drain_step(&s, 603, 8, 12, 6);
  assert(r.reason == PS5BT_DRAIN_CANCEL && r.cap == 12 && !s.pending);
  (void)ps5bt_queue_drain_step(&s, 604, 12, 8, 9);
  r = ps5bt_queue_drain_step(&s, 603, 12, 8, 9);
  assert(r.reason == PS5BT_DRAIN_CLOCK && r.cap == 8 && !s.pending);
  r = ps5bt_queue_drain_step(&s, -1, 8, 6, 8);
  assert(r.reason == PS5BT_DRAIN_CLOCK && r.cap == 6);
  r = ps5bt_queue_drain_step(&s, INT64_MAX - 500, 12, 8, 9);
  assert(r.reason == PS5BT_DRAIN_WAIT);
  r = ps5bt_queue_drain_step(&s, INT64_MAX, 12, 8, 9);
  assert(r.reason == PS5BT_DRAIN_TIMEOUT && r.waited_ms == 500);
  r = ps5bt_queue_drain_step(&s, INT64_MAX, 8, 6, 4);
  assert(r.reason == PS5BT_DRAIN_APPLY && r.cap == 6);
  r = ps5bt_queue_drain_step(&s, INT64_MAX, 6, 0, 4);
  assert(r.reason == PS5BT_DRAIN_INVALID && r.cap == 6);
}

int main(void) {
  ph_clock_hook = clock_value;
  real_queue_regression();
  policy_bounds();
  puts("queue drain: real admission/order, no-credit mutation, timeout, "
       "retarget and clock bounds passed");
  return 0;
}
