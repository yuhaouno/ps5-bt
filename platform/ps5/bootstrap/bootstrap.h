/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_BOOTSTRAP_H
#define PS5_BOOTSTRAP_H
#include <stdint.h>

/* Synchronous libkernel-only lookup while the bootstrap's main is running.
 * Success returns 0 and a non-NULL symbol. Failure clears *out and preserves
 * the last raw resolver error, or returns -2 invalid/-5 unavailable locally.
 * No module loading, permission change, retained pointer or transport access. */
int64_t ps5_bootstrap_lookup(const char *name, void **out);
#endif
