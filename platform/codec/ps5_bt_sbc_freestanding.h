/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_BT_SBC_FREESTANDING_H
#define PS5_BT_SBC_FREESTANDING_H

/* Minimal hosted-header replacement for the audited SBC_STATIC_ONLY build.
 * Integer, size, Boolean and limit types remain compiler-provided standard
 * headers. These declarations cover only the byte operations supplied by the
 * existing freestanding runtime and the FreeBSD errno values returned by the
 * selected upstream code. No allocator or I/O interface is declared. */
#include <limits.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef __INTPTR_TYPE__ ssize_t;

#define EIO 5
#define ENOMEM 12
#define EINVAL 22
#define ENOSPC 28

#define __LITTLE_ENDIAN 1234
#define __BIG_ENDIAN 4321
#if defined(__BYTE_ORDER__) && __BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__
#define __BYTE_ORDER __LITTLE_ENDIAN
#elif defined(__BYTE_ORDER__) && __BYTE_ORDER__ == __ORDER_BIG_ENDIAN__
#define __BYTE_ORDER __BIG_ENDIAN
#else
#error "SBC target byte order is unavailable"
#endif

void *memcpy(void *destination, const void *source, size_t count);
void *memset(void *destination, int value, size_t count);

#endif /* PS5_BT_SBC_FREESTANDING_H */
