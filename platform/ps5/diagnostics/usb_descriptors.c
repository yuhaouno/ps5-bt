/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "usb_descriptors.h"

static uint16_t le16(const uint8_t *p)
{
    return (uint16_t)((uint16_t)p[0] | ((uint16_t)p[1] << 8));
}

static int repeated_interface(const uint8_t *data, size_t end, uint8_t number,
                              uint8_t alternate)
{
    size_t offset = data[0];
    /* Earlier descriptors have already been checked by the caller. */
    while (offset < end) {
        if (data[offset + 1] == 4 && data[offset + 2] == number &&
            data[offset + 3] == alternate)
            return 1;
        offset += data[offset];
    }
    return 0;
}

enum ps5_usb_status ps5_usb_parse_config(const uint8_t *data, size_t len,
                                         struct ps5_usb_config *out)
{
    struct ps5_usb_config result = {0};
    uint8_t interfaces[256] = {0};
    uint8_t default_alternates[256] = {0};
    size_t offset, descriptors = 1, unique_interfaces = 0;
    unsigned expected_endpoints = 0, seen_endpoints = 0;
    uint32_t endpoint_addresses = 0;
    int have_interface = 0, hci = 0;

    if (!data || !out)
        return PS5_USB_INVALID;
    if (len > PS5_USB_MAX_CONFIG_BYTES)
        return PS5_USB_CAPACITY;
    if (len < 9 || data[0] < 9 || data[0] > len || data[1] != 2 ||
        le16(data + 2) != len)
        return PS5_USB_MALFORMED;
    result.total_length = (uint16_t)len;
    result.configuration_value = data[5];
    result.interface_count = data[4];
    offset = data[0];
    while (offset < len) {
        const uint8_t *d = data + offset;
        size_t remaining = len - offset;
        if (remaining < 2 || d[0] < 2 || d[0] > remaining)
            return PS5_USB_MALFORMED;
        if (++descriptors > PS5_USB_MAX_DESCRIPTORS)
            return PS5_USB_CAPACITY;
        if (d[1] == 2) /* Nested configuration is never a valid tail. */
            return PS5_USB_MALFORMED;
        if (d[1] == 4) {
            if (d[0] < 9 ||
                (have_interface && seen_endpoints != expected_endpoints))
                return PS5_USB_MALFORMED;
            if (repeated_interface(data, offset, d[2], d[3]))
                return PS5_USB_MALFORMED;
            if (!interfaces[d[2]]) {
                interfaces[d[2]] = 1;
                ++unique_interfaces;
            }
            if (d[3] == 0)
                default_alternates[d[2]] = 1;
            have_interface = 1;
            expected_endpoints = d[4];
            seen_endpoints = 0;
            endpoint_addresses = 0;
            hci = d[5] == 0xe0 && d[6] == 1 && d[7] == 1;
            if (hci) {
                struct ps5_usb_group *group;
                if (result.group_count == PS5_USB_MAX_GROUPS)
                    return PS5_USB_CAPACITY;
                group = &result.groups[result.group_count++];
                group->interface_number = d[2];
                group->alternate_setting = d[3];
                group->first_endpoint = result.endpoint_count;
            }
        } else if (d[1] == 5) {
            unsigned address_bit;
            if (d[0] < 7 || !have_interface ||
                seen_endpoints >= expected_endpoints || (d[2] & 0x70) ||
                !(d[2] & 0x0f))
                return PS5_USB_MALFORMED;
            address_bit = (d[2] & 0x0f) + ((d[2] & 0x80) ? 16u : 0u);
            if (endpoint_addresses & (UINT32_C(1) << address_bit))
                return PS5_USB_MALFORMED;
            endpoint_addresses |= UINT32_C(1) << address_bit;
            ++seen_endpoints;
            if (hci) {
                struct ps5_usb_endpoint *endpoint;
                if (result.endpoint_count == PS5_USB_MAX_ENDPOINTS)
                    return PS5_USB_CAPACITY;
                endpoint = &result.endpoints[result.endpoint_count++];
                endpoint->address = d[2];
                endpoint->attributes = d[3];
                endpoint->max_packet_size = le16(d + 4);
                endpoint->interval = d[6];
                ++result.groups[result.group_count - 1].endpoint_count;
            }
        }
        offset += d[0];
    }
    if ((have_interface && seen_endpoints != expected_endpoints) ||
        unique_interfaces != result.interface_count)
        return PS5_USB_MALFORMED;
    for (size_t i = 0; i < sizeof(interfaces); ++i)
        if (interfaces[i] && !default_alternates[i])
            return PS5_USB_MALFORMED;
    *out = result;
    return PS5_USB_OK;
}

const char *ps5_usb_vendor_label(uint16_t vendor_id)
{
    switch (vendor_id) {
    case 0x1286:
        return "Marvell/NXP";
    case 0x0e8d:
        return "MediaTek";
    default:
        return "Unknown";
    }
}

struct writer {
    char *data;
    size_t len;
};

static void put(struct writer *w, const char *text)
{
    while (*text) {
        if (w->data)
            w->data[w->len] = *text;
        ++w->len;
        ++text;
    }
}

static void hex(struct writer *w, uint16_t value, unsigned digits)
{
    static const char alphabet[] = "0123456789abcdef";
    while (digits) {
        char text[2] = {alphabet[(value >> (--digits * 4)) & 15], 0};
        put(w, text);
    }
}

static void encode(struct writer *w, uint16_t vid, uint16_t pid,
                   const struct ps5_usb_config *config)
{
    put(w, "ps5-usb-metadata-v1 vid=");
    hex(w, vid, 4);
    put(w, " pid=");
    hex(w, pid, 4);
    put(w, " vendor=");
    put(w, ps5_usb_vendor_label(vid));
    put(w, " config=");
    hex(w, config->configuration_value, 2);
    put(w, " bytes=");
    hex(w, config->total_length, 4);
    put(w, " interfaces=");
    hex(w, config->interface_count, 2);
    put(w, " groups=");
    hex(w, config->group_count, 4);
    put(w, " transport=Unsupported\n");
    for (size_t i = 0; i < config->group_count; ++i) {
        const struct ps5_usb_group *group = &config->groups[i];
        put(w, "hci-interface=");
        hex(w, group->interface_number, 2);
        put(w, " alternate=");
        hex(w, group->alternate_setting, 2);
        put(w, " endpoints=");
        hex(w, group->endpoint_count, 4);
        put(w, "\n");
        for (size_t j = 0; j < group->endpoint_count; ++j) {
            const struct ps5_usb_endpoint *ep =
                &config->endpoints[group->first_endpoint + j];
            put(w, " endpoint=");
            hex(w, ep->address, 2);
            put(w, " attributes=");
            hex(w, ep->attributes, 2);
            put(w, " max-packet-raw=");
            hex(w, ep->max_packet_size, 4);
            put(w, " interval=");
            hex(w, ep->interval, 2);
            put(w, "\n");
        }
    }
}

enum ps5_usb_status ps5_usb_encode_report(uint16_t vendor_id,
                                          uint16_t product_id,
                                          const struct ps5_usb_config *config,
                                          char *data, size_t capacity,
                                          size_t *out_len)
{
    struct writer w = {0};
    if (!out_len)
        return PS5_USB_INVALID;
    *out_len = 0;
    if (!config || (!data && capacity) ||
        config->group_count > PS5_USB_MAX_GROUPS ||
        config->endpoint_count > PS5_USB_MAX_ENDPOINTS)
        return PS5_USB_INVALID;
    for (size_t i = 0; i < config->group_count; ++i) {
        const struct ps5_usb_group *g = &config->groups[i];
        if (g->first_endpoint > config->endpoint_count ||
            g->endpoint_count > config->endpoint_count - g->first_endpoint)
            return PS5_USB_INVALID;
    }
    encode(&w, vendor_id, product_id, config);
    *out_len = w.len;
    if (capacity < w.len || !data)
        return PS5_USB_CAPACITY;
    w.data = data;
    w.len = 0;
    encode(&w, vendor_id, product_id, config);
    return PS5_USB_OK;
}
