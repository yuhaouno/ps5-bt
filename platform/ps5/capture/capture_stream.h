/* SPDX-License-Identifier: GPL-3.0-only */
#ifndef PS5_BT_CAPTURE_STREAM_H
#define PS5_BT_CAPTURE_STREAM_H
#include <stddef.h>
#include <stdint.h>

#define PS5_CAPTURE_STREAM_ABI UINT32_C(0x00010000)
#define PS5_CAPTURE_STREAM_MAX_BYTES UINT32_C(8192)
#define PS5_CAPTURE_STREAM_DISCONTINUITY UINT32_C(1)
#define PS5_CAPTURE_STREAM_STOP_FAILED UINT32_C(1)
#define PS5_CAPTURE_STREAM_CLOSE_FAILED UINT32_C(2)
#define PS5_CAPTURE_STREAM_TERMINATE_FAILED UINT32_C(4)

/* Independent experimental ABI. No native handle, format or time-unit claim. */
struct ps5_capture_stream_ops {
  uint32_t abi_version;
  uint32_t struct_bytes;
  int64_t (*lookup_current)(const char *, void **);
  int64_t (*lookup_module)(int32_t, const char *, void **);
};
struct ps5_capture_stream_report {
  uint32_t abi_version;
  uint32_t stage; /* 1 validation, 2 resolution/load, 3 init, 4 open,
                     5 start, 6 read, 7 stop, 8 close */
  int32_t status; /* 0 success, -1 unavailable, -2 invalid, -3 busy, -5 native failure */
  int32_t raw_status;
  uint32_t cleanup_flags;
  int32_t raw_stop;
  int32_t raw_close;
  int32_t raw_terminate;
  int64_t raw_lookup; /* Preserve the full resolver result independently. */
};
struct ps5_capture_stream_read {
  uint32_t abi_version;
  uint32_t kind; /* 1 Data, 2 Zero, 3 Empty, 4 Overrun, 5 Ended, 6 Error */
  int32_t raw_status;
  uint32_t bytes;
  uint32_t flags;
  uint8_t metadata[16]; /* Opaque; never decoded into timestamps or logged. */
};
uint32_t ps5_capture_stream_abi_version(void);
uint32_t ps5_capture_stream_storage_size(void);
uint32_t ps5_capture_stream_storage_alignment(void);
/* Storage remains exclusively borrowed and address-stable until close. Failed
 * open consumes any definitely acquired lifecycle resources before returning.
 * The process's loaded modules remain loaded; unload ownership is unknown.
 * All calls are single-threaded, synchronous and non-reentrant. */
int32_t ps5_capture_stream_open(void *, uint32_t,
    const struct ps5_capture_stream_ops *, struct ps5_capture_stream_report *);
int32_t ps5_capture_stream_start(void *, struct ps5_capture_stream_report *);
int32_t ps5_capture_stream_read_once(void *, uint8_t *, uint32_t,
    struct ps5_capture_stream_read *, struct ps5_capture_stream_report *);
/* Stop and close consume their phases even on native failure, never retry. */
int32_t ps5_capture_stream_stop(void *, struct ps5_capture_stream_report *);
int32_t ps5_capture_stream_close(void *, struct ps5_capture_stream_report *);
#endif
