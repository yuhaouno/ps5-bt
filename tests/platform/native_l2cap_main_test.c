/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "ps5_bt.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

#define main diagnostic_main
#include "../../platform/ps5/main.c"
#undef main

static unsigned rust_calls, lookups, sockets, closes, phases, reports;
static int native_errno, scenario;
static char report[PS5_NATIVE_L2CAP_REPORT_CAPACITY + 1];

int32_t ps5_rust_probe(void) {
  assert(++rust_calls == 1);
  return 2;
}

static int fixture_socket(int domain, int type, int protocol) {
  assert(lookups == 3 && native_errno == 0);
  if (++sockets == 1) {
    assert(domain == 2 && type == 2 && protocol == 17);
    if (scenario == 2) {
      native_errno = 13;
      return -1;
    }
    return 0;
  }
  assert(sockets == 2 && closes == 1);
  assert(domain == 36 && type == 5 && protocol == 135);
  if (scenario == 1) {
    native_errno = 47;
    return -1;
  }
  return 7;
}

static int fixture_close(int fd) {
  assert(native_errno == 0);
  assert(fd == (++closes == 1 ? 0 : 7));
  if (scenario == 3) {
    native_errno = 4;
    return -1;
  }
  return 0;
}

static int *fixture_errno(void) { return &native_errno; }

int64_t ps5_bootstrap_lookup(const char *name, void **out) {
  assert(rust_calls == 1 && *out == NULL);
  ++lookups;
  if (strcmp(name, "socket") == 0) {
    ps5_native_l2cap_socket_fn value = fixture_socket;
    memcpy(out, &value, sizeof value);
  } else if (strcmp(name, "close") == 0) {
    ps5_native_l2cap_close_fn value = fixture_close;
    memcpy(out, &value, sizeof value);
  } else {
    assert(strcmp(name, "__error") == 0);
    ps5_native_l2cap_errno_fn value = fixture_errno;
    memcpy(out, &value, sizeof value);
  }
  return 0;
}

void ps5_platform_log(const uint8_t *data, uint32_t length) {
  assert(rust_calls == 1 && data && length < sizeof report);
  char text[PS5_NATIVE_L2CAP_REPORT_CAPACITY + 1];
  memcpy(text, data, length);
  text[length] = 0;
  if (strstr(text, "event=native_l2cap_phase ")) {
    ++phases;
    assert(length < 192 && text[length - 1] == '\n');
  } else if (strstr(text, "native_l2cap_v1 ")) {
    ++reports;
    memcpy(report, text, length + 1);
  } else {
    assert(strcmp(text, "event=native_l2cap_begin\n") == 0);
    assert(!lookups && !sockets && !closes && !phases);
  }
  native_errno =
      999; /* The probe must capture before logging and clear before calls. */
}

int main(void) {
  for (scenario = 0; scenario < 4; ++scenario) {
    rust_calls = lookups = sockets = closes = phases = reports = 0;
    native_errno = 999;
    assert(diagnostic_main() == (scenario >= 2 ? 1 : 2));
    assert(lookups == 3 && reports == 1);
    assert(strstr(report, "transport=unsupported controller=unknown"));
    if (scenario == 0) {
      assert(sockets == 2 && closes == 2 && phases == 16);
      assert(strstr(report, "status=local_socket_available"));
    } else if (scenario == 1) {
      assert(sockets == 2 && closes == 1 && phases == 14);
      assert(strstr(report, "l2cap_socket(36,5,135)=-1 errno=47"));
    } else if (scenario == 2) {
      assert(sockets == 1 && closes == 0 && phases == 10);
      assert(strstr(report, "status=control_failed"));
    } else {
      assert(sockets == 1 && closes == 1 && phases == 12);
      assert(strstr(report, "status=cleanup_uncertain"));
    }
  }
  puts("PASS native L2CAP real main ordering, phase logs, errno and final "
       "status");
  return 0;
}
