/* SPDX-License-Identifier: GPL-3.0-only */
#ifndef CAPTURE_STREAM_FIXTURE_H
#define CAPTURE_STREAM_FIXTURE_H
#include "capture_stream.h"
void ps5_capture_test_reset(void);
void ps5_capture_test_fail(uint32_t stage, int32_t raw);
void ps5_capture_test_read(int32_t raw);
uint32_t ps5_capture_test_calls(uint32_t phase);
int64_t ps5_capture_test_current(const char *, void **);
int64_t ps5_capture_test_module(int32_t, const char *, void **);
#endif
