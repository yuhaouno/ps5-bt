/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_USB_DESCRIPTORS_H
#define PS5_USB_DESCRIPTORS_H

#include <stddef.h>
#include <stdint.h>

/* Project-owned metadata API, independent of ps5_bt.h and transport permission.
 */
#define PS5_USB_MAX_CONFIG_BYTES 65535u
#define PS5_USB_MAX_DESCRIPTORS 512u
#define PS5_USB_MAX_GROUPS 32u
#define PS5_USB_MAX_ENDPOINTS 128u

enum ps5_usb_status {
    PS5_USB_OK = 0,
    PS5_USB_INVALID = -1,
    PS5_USB_MALFORMED = -2,
    PS5_USB_CAPACITY = -3
};

struct ps5_usb_endpoint {
    uint8_t address;
    uint8_t attributes;
    uint16_t
        max_packet_size; /* Raw USB wMaxPacketSize, not inferred throughput. */
    uint8_t interval;
};

struct ps5_usb_group {
    uint8_t interface_number;
    uint8_t alternate_setting;
    uint16_t first_endpoint;
    uint16_t endpoint_count;
};

struct ps5_usb_config {
    uint16_t total_length;
    uint8_t configuration_value;
    uint8_t interface_count;
    uint16_t group_count;
    uint16_t endpoint_count;
    struct ps5_usb_group groups[PS5_USB_MAX_GROUPS];
    struct ps5_usb_endpoint endpoints[PS5_USB_MAX_ENDPOINTS];
};

/* Complete single configuration only: len must equal wTotalLength. Every
 * descriptor is validated, including non-HCI interfaces. Every e0/01/01 group
 * and all its endpoints are retained, including nonzero alternate settings.
 * No endpoint is selected. On error, *out is unchanged. Input and output must
 * be distinct valid objects. No allocation, I/O, global state or retained data.
 */
enum ps5_usb_status ps5_usb_parse_config(const uint8_t *data, size_t len,
                                         struct ps5_usb_config *out);

/* VID labels are reporting hints only. Unknown VID never selects a backend. */
const char *ps5_usb_vendor_label(uint16_t vendor_id);

/* Deterministic ASCII v1 report, no serial/address/user strings, no trailing
 * NUL. Requires a parser-produced config. On error the destination is unchanged
 * and *out_len is zero. Query required length with data=NULL, capacity=0; this
 * returns CAPACITY and sets *out_len to the required length. Other capacity
 * errors also return the required length. Config/output/out_len must not alias.
 */
enum ps5_usb_status ps5_usb_encode_report(uint16_t vendor_id,
                                          uint16_t product_id,
                                          const struct ps5_usb_config *config,
                                          char *data, size_t capacity,
                                          size_t *out_len);

#endif
