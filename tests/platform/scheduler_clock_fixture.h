/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_SCHEDULER_CLOCK_FIXTURE_H
#define PS5_SCHEDULER_CLOCK_FIXTURE_H
#include "scheduler_clock.h"
void ps5_scheduler_test_reset(void);
void ps5_scheduler_test_lookup_failure(uint32_t which, int64_t raw,
                                       uint32_t null_symbol);
void ps5_scheduler_test_clock(int32_t raw, int64_t seconds,
                              int64_t nanoseconds);
void ps5_scheduler_test_wait_status(int32_t raw);
uint32_t ps5_scheduler_test_count(uint32_t which);
uint32_t ps5_scheduler_test_requested(void);
void ps5_scheduler_test_reenter(void *storage);
int64_t ps5_scheduler_test_lookup(const char *, void **);
#endif
