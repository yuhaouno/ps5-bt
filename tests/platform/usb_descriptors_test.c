/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "usb_descriptors.h"

#include <assert.h>
#include <stdio.h>
#include <string.h>

static const uint8_t fixture[] = {
    9, 2, 48,   0, 2,    1, 0,  0x80, 50, 9, 4, 0,    0,    3,  0xe0, 1,
    1, 0, 7,    5, 0x81, 3, 16, 0,    1,  7, 5, 0x82, 2,    64, 0,    0,
    7, 5, 0x02, 2, 64,   0, 0,  9,    4,  1, 0, 0,    0xff, 0,  0,    0};

static void rejected(const uint8_t *data, size_t len,
                     enum ps5_usb_status expected)
{
    struct ps5_usb_config out, before;
    memset(&out, 0xa5, sizeof(out));
    memcpy(&before, &out, sizeof(out));
    assert(ps5_usb_parse_config(data, len, &out) == expected);
    assert(memcmp(&out, &before, sizeof(out)) == 0);
}

static void total(uint8_t *data, size_t len)
{
    assert(len <= UINT16_MAX);
    data[2] = (uint8_t)len;
    data[3] = (uint8_t)(len >> 8);
}

static void valid_and_truncations(void)
{
    struct ps5_usb_config out;
    uint8_t data[sizeof(fixture) + 2];
    assert(ps5_usb_parse_config(fixture, sizeof(fixture), &out) == PS5_USB_OK);
    assert(out.total_length == sizeof(fixture) && out.configuration_value == 1);
    assert(out.group_count == 1 && out.endpoint_count == 3 &&
           out.interface_count == 2);
    assert(out.groups[0].interface_number == 0 &&
           out.groups[0].alternate_setting == 0);
    assert(out.groups[0].first_endpoint == 0 &&
           out.groups[0].endpoint_count == 3);
    assert(out.endpoints[0].address == 0x81 &&
           out.endpoints[0].attributes == 3);
    assert(out.endpoints[1].max_packet_size == 64 &&
           out.endpoints[2].address == 2);
    for (size_t len = 0; len < sizeof(fixture); ++len) {
        rejected(fixture, len, PS5_USB_MALFORMED);
        memcpy(data, fixture, sizeof(fixture));
        total(data, len);
        /* Some rewritten prefixes are complete configurations only when their
         * advertised interfaces/endpoints are also present; fixture has none.
         */
        rejected(data, len, PS5_USB_MALFORMED);
    }
    memcpy(data, fixture, sizeof(fixture));
    data[sizeof(fixture)] = 2;
    data[sizeof(fixture) + 1] = 0xff;
    rejected(data, sizeof(data), PS5_USB_MALFORMED);
    total(data, sizeof(data));
    assert(ps5_usb_parse_config(data, sizeof(data), &out) == PS5_USB_OK);
    data[sizeof(fixture)] = 0;
    rejected(data, sizeof(data), PS5_USB_MALFORMED);
    data[sizeof(fixture)] = 1;
    rejected(data, sizeof(data), PS5_USB_MALFORMED);
    total(data, sizeof(data) - 1);
    rejected(data, sizeof(data) - 1, PS5_USB_MALFORMED);
    rejected(fixture, SIZE_MAX, PS5_USB_CAPACITY);
    rejected(NULL, 0, PS5_USB_INVALID);
    assert(ps5_usb_parse_config(fixture, sizeof(fixture), NULL) ==
           PS5_USB_INVALID);
}

static void malformed_fields(void)
{
    uint8_t data[sizeof(fixture)];
    const size_t lengths[] = {0, 9, 18, 25, 32, 39};
    for (size_t i = 0; i < sizeof(lengths) / sizeof(lengths[0]); ++i) {
        for (unsigned length = 0; length < 256; ++length) {
            struct ps5_usb_config out, before;
            memcpy(data, fixture, sizeof(data));
            data[lengths[i]] = (uint8_t)length;
            memset(&out, 0xa5, sizeof(out));
            memcpy(&before, &out, sizeof(out));
            enum ps5_usb_status status =
                ps5_usb_parse_config(data, sizeof(data), &out);
            if (status != PS5_USB_OK)
                assert(memcmp(&before, &out, sizeof(out)) == 0);
            if (length < (i == 0 || i == 1 || i == 5 ? 9u : 7u))
                assert(status == PS5_USB_MALFORMED);
        }
    }
    for (unsigned declared = 0; declared <= UINT16_MAX; ++declared) {
        memcpy(data, fixture, sizeof(data));
        total(data, declared);
        if (declared != sizeof(data))
            rejected(data, sizeof(data), PS5_USB_MALFORMED);
    }
    for (unsigned address = 0; address < 256; ++address) {
        struct ps5_usb_config out;
        memcpy(data, fixture, sizeof(data));
        data[20] = (uint8_t)address;
        enum ps5_usb_status status =
            ps5_usb_parse_config(data, sizeof(data), &out);
        if ((address & 0x70) || !(address & 0x0f) || address == 0x82 ||
            address == 2)
            assert(status == PS5_USB_MALFORMED);
        else
            assert(status == PS5_USB_OK);
    }
    memcpy(data, fixture, sizeof(data));
    data[13] = 4;
    rejected(data, sizeof(data), PS5_USB_MALFORMED); /* Missing endpoint. */
    memcpy(data, fixture, sizeof(data));
    data[13] = 2;
    rejected(data, sizeof(data), PS5_USB_MALFORMED); /* Excess endpoint. */
    memcpy(data, fixture, sizeof(data));
    data[4] = 1;
    rejected(data, sizeof(data), PS5_USB_MALFORMED);
    memcpy(data, fixture, sizeof(data));
    data[41] = 0;
    rejected(data, sizeof(data),
             PS5_USB_MALFORMED); /* Duplicate interface/alt. */
    memcpy(data, fixture, sizeof(data));
    data[10] = 5;
    rejected(data, sizeof(data),
             PS5_USB_MALFORMED); /* Endpoint outside interface. */
    memcpy(data, fixture, sizeof(data));
    data[40] = 2;
    rejected(data, sizeof(data), PS5_USB_MALFORMED);
    memcpy(data, fixture, sizeof(data));
    data[12] = 1;
    rejected(data, sizeof(data),
             PS5_USB_MALFORMED); /* Missing alternate zero. */
}

static size_t append_group(uint8_t *data, size_t len, uint8_t number,
                           uint8_t alternate, unsigned endpoints)
{
    const uint8_t interface[] = {9,    4, number, alternate, (uint8_t)endpoints,
                                 0xe0, 1, 1,      0};
    memcpy(data + len, interface, sizeof(interface));
    len += sizeof(interface);
    for (unsigned i = 0; i < endpoints; ++i) {
        const uint8_t ep[] = {
            7,    5, (uint8_t)(1 + i % 15 + (i >= 15 ? 0x80 : 0)), 2, 0x34,
            0x12, 4};
        memcpy(data + len, ep, sizeof(ep));
        len += sizeof(ep);
    }
    total(data, len);
    return len;
}

static void groups_and_bounds(void)
{
    uint8_t data[4096] = {9, 2, 9, 0, 1, 7, 0, 0x80, 0};
    struct ps5_usb_config out;
    size_t len = 9;
    for (unsigned i = 0; i < PS5_USB_MAX_GROUPS; ++i)
        len = append_group(data, len, 0, (uint8_t)i, 3);
    assert(ps5_usb_parse_config(data, len, &out) == PS5_USB_OK);
    assert(out.group_count == PS5_USB_MAX_GROUPS && out.endpoint_count == 96);
    assert(out.groups[31].alternate_setting == 31 &&
           out.groups[31].first_endpoint == 93);
    assert(out.endpoints[95].max_packet_size == 0x1234);
    len = append_group(data, len, 0, 32, 0);
    rejected(data, len, PS5_USB_CAPACITY);
    len = 9;
    data[4] = 2;
    len = append_group(data, len, 0, 0, 3);
    len = append_group(data, len, 1, 0, 3);
    len = append_group(data, len, 1, 1, 0);
    assert(ps5_usb_parse_config(data, len, &out) == PS5_USB_OK);
    assert(out.group_count == 3 && out.groups[1].first_endpoint == 3);
    assert(out.groups[2].alternate_setting == 1 &&
           out.groups[2].endpoint_count == 0);
    len = 9;
    data[4] = 1;
    for (unsigned i = 0; i < 4; ++i)
        len = append_group(data, len, 0, (uint8_t)i, 30);
    len = append_group(data, len, 0, 4, 8);
    assert(ps5_usb_parse_config(data, len, &out) == PS5_USB_OK);
    assert(out.endpoint_count == PS5_USB_MAX_ENDPOINTS);
    len = append_group(data, len, 0, 5, 1);
    rejected(data, len, PS5_USB_CAPACITY);
    len = 9;
    data[4] = 0;
    for (unsigned i = 0; i < PS5_USB_MAX_DESCRIPTORS - 1; ++i) {
        data[len++] = 2;
        data[len++] = 0xff;
    }
    total(data, len);
    assert(ps5_usb_parse_config(data, len, &out) == PS5_USB_OK);
    data[len++] = 2;
    data[len++] = 0xff;
    total(data, len);
    rejected(data, len, PS5_USB_CAPACITY);
}

static void reports_and_vendors(void)
{
    struct ps5_usb_config out;
    char report[4096], short_report[4096], second[4096];
    size_t len, second_len;
    assert(ps5_usb_parse_config(fixture, sizeof(fixture), &out) == PS5_USB_OK);
    assert(ps5_usb_encode_report(0x1234, 0xabcd, &out, report, sizeof(report),
                                 &len) == PS5_USB_OK);
    assert(len < sizeof(report));
    report[len] = 0;
    assert(strstr(report, "vid=1234 pid=abcd vendor=Unknown") != NULL);
    assert(strstr(report, "transport=Unsupported\n") != NULL);
    assert(strstr(report, "endpoint=82 attributes=02 max-packet-raw=0040") !=
           NULL);
    assert(ps5_usb_encode_report(0x1234, 0xabcd, &out, NULL, 0, &second_len) ==
           PS5_USB_CAPACITY);
    assert(second_len == len);
    for (size_t capacity = 0; capacity < len; ++capacity) {
        memset(short_report, 0xa5, sizeof(short_report));
        assert(ps5_usb_encode_report(0x1234, 0xabcd, &out, short_report,
                                     capacity,
                                     &second_len) == PS5_USB_CAPACITY);
        assert(second_len == len);
        for (size_t i = 0; i < sizeof(short_report); ++i)
            assert((unsigned char)short_report[i] == 0xa5);
    }
    memset(second, 0xa5, sizeof(second));
    assert(ps5_usb_encode_report(0x1234, 0xabcd, &out, second, len,
                                 &second_len) == PS5_USB_OK);
    assert(second_len == len && memcmp(report, second, len) == 0);
    assert((unsigned char)second[len] == 0xa5); /* No hidden NUL write. */
    out.groups[0].first_endpoint = UINT16_MAX;
    assert(ps5_usb_encode_report(0, 0, &out, second, sizeof(second),
                                 &second_len) == PS5_USB_INVALID);
    assert(second_len == 0);
    out.group_count = UINT16_MAX;
    assert(ps5_usb_encode_report(0, 0, &out, second, sizeof(second),
                                 &second_len) == PS5_USB_INVALID);
    out.group_count = 0;
    out.endpoint_count = UINT16_MAX;
    assert(ps5_usb_encode_report(0, 0, &out, second, sizeof(second),
                                 &second_len) == PS5_USB_INVALID);
    assert(ps5_usb_encode_report(0, 0, NULL, second, sizeof(second),
                                 &second_len) == PS5_USB_INVALID);
    assert(ps5_usb_encode_report(0, 0, &out, second, sizeof(second), NULL) ==
           PS5_USB_INVALID);
    for (unsigned vid = 0; vid <= UINT16_MAX; ++vid) {
        const char *label = ps5_usb_vendor_label((uint16_t)vid);
        assert(strcmp(label, vid == 0x1286   ? "Marvell/NXP"
                             : vid == 0x0e8d ? "MediaTek"
                                             : "Unknown") == 0);
    }
}

static void deterministic_mutations(void)
{
    uint32_t state = 0x12345678;
    for (unsigned i = 0; i < 20000; ++i) {
        uint8_t data[sizeof(fixture)];
        struct ps5_usb_config out, before;
        memcpy(data, fixture, sizeof(data));
        state = state * UINT32_C(1664525) + UINT32_C(1013904223);
        data[state % sizeof(data)] ^= (uint8_t)(state >> 24);
        memset(&out, 0xa5, sizeof(out));
        memcpy(&before, &out, sizeof(out));
        enum ps5_usb_status status =
            ps5_usb_parse_config(data, sizeof(data), &out);
        if (status != PS5_USB_OK)
            assert(memcmp(&out, &before, sizeof(out)) == 0);
        else
            assert(out.group_count <= PS5_USB_MAX_GROUPS &&
                   out.endpoint_count <= PS5_USB_MAX_ENDPOINTS);
    }
}

int main(void)
{
    valid_and_truncations();
    malformed_fields();
    groups_and_bounds();
    reports_and_vendors();
    deterministic_mutations();
    puts("USB metadata parser/report: all host checks passed (synthetic data "
         "only)");
    return 0;
}
