/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_USB_PROBE_H
#define PS5_USB_PROBE_H

#include "usb_descriptors.h"

#define PS5_USB_PROBE_MAX_NODES 64u
#define PS5_USB_PROBE_NAME_CAPACITY 32u
#define PS5_USB_PROBE_REPORT_CAPACITY 16384u

/* Reserved next_name results: project-local bounds/validation, never errno. */
#define PS5_USB_PROBE_DIRECTORY_MALFORMED (-10001L)
#define PS5_USB_PROBE_DIRECTORY_READ_LIMIT (-10002L)
#define PS5_USB_PROBE_NAME_TOO_LONG (-10003L)

/* Diagnostic-only callbacks. Negative results preserve raw normalized errno,
 * except the three explicitly reserved project-local directory results above.
 * next_name: 1 = bounded name, 0 = complete directory, negative = incomplete.
 * All callbacks are synchronous; no pointer may be retained. open_readonly
 * must never fall back to write access. Directory and device fds may be zero.
 */
struct ps5_usb_probe_ops {
  void *context;
  long (*open_readonly)(void *, const char *);
  long (*next_name)(void *, int, char *, size_t);
  long (*device_descriptor)(void *, int, uint8_t *, size_t);
  long (*configuration)(void *, int, uint8_t, uint8_t *, size_t, size_t *);
  long (*close_fd)(void *, int);
  void (*log)(void *, const char *, size_t);
  /* Optional paired read-only binding metadata. Active value is a zero-based
   * configuration index, not bConfigurationValue. Driver returns actual
   * bytes; zero length with a zero byte is the FreeBSD empty-binding case.
   */
  long (*active_configuration)(void *, int, int *);
  long (*interface_driver)(void *, int, uint8_t, char *, size_t, size_t *);
};

struct ps5_usb_probe_workspace {
  uint8_t descriptor[PS5_USB_MAX_CONFIG_BYTES];
  char report[PS5_USB_PROBE_REPORT_CAPACITY];
  struct ps5_usb_config config;
};

/* Returns 0 only for complete enumeration, valid descriptor reads and cleanup;
 * returns 1 for incomplete/error and -1 for invalid arguments. No transport is
 * activated, and even 0 is not HCI ownership or noninterference evidence.
 */
int ps5_usb_probe_with_ops(const struct ps5_usb_probe_ops *,
                           struct ps5_usb_probe_workspace *);

/* SDK target adapter: compile usb_probe.c with PS5_BT_USB_PROBE_TARGET. */
int ps5_usb_probe_run(void);

#endif
