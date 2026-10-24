/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "usb_probe.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

static const uint8_t valid_config[] = {9, 2, 25,   0, 1,  1,    0, 0x80, 50,
                                       9, 4, 0,    0, 1,  0xe0, 1, 1,    0,
                                       7, 5, 0x82, 3, 64, 0,    1};

struct fake {
  unsigned names;
  unsigned cursor;
  unsigned opened;
  unsigned closed;
  unsigned configurations;
  unsigned devices;
  long directory_result;
  long open_result;
  long enumeration_result;
  long device_result;
  long config_result;
  long close_result;
  int invalid_device;
  int invalid_config;
  int oversized_config;
  int irrelevant_names;
  unsigned configuration_count;
  char output[65536];
  size_t output_length;
  int active_index;
  long active_result;
  long driver_result;
  const char *driver_text;
  unsigned driver_calls;
  int oversized_driver;
  int bindings;
  const char *name_override;
  int unterminated_name;
};

static long fake_open(void *opaque, const char *path) {
  struct fake *fake = opaque;
  if (strcmp(path, "/dev") == 0)
    return fake->directory_result;
  assert(strncmp(path, "/dev/ugen0.", 11) == 0);
  ++fake->opened;
  return fake->open_result;
}

static long fake_next(void *opaque, int fd, char *name, size_t capacity) {
  struct fake *fake = opaque;
  assert(fd == fake->directory_result);
  if (fake->cursor == fake->names)
    return fake->enumeration_result;
  if (fake->unterminated_name) {
    memset(name, '0', capacity);
    memcpy(name, "ugen", 4);
    ++fake->cursor;
    return 1;
  }
  int count;
  if (fake->name_override)
    count = snprintf(name, capacity, "%s", fake->name_override);
  else if (fake->irrelevant_names)
    count = snprintf(name, capacity, "other%u", fake->cursor);
  else
    count = snprintf(name, capacity, "ugen0.%u", fake->cursor);
  assert(count >= 0 && (size_t)count < capacity);
  ++fake->cursor;
  return 1;
}

static long fake_device(void *opaque, int fd, uint8_t *bytes, size_t capacity) {
  struct fake *fake = opaque;
  assert(fd == fake->open_result && capacity == 18);
  ++fake->devices;
  memset(bytes, 0, capacity);
  bytes[0] = fake->invalid_device ? 0 : 18;
  bytes[1] = 1;
  bytes[8] = 0x86;
  bytes[9] = 0x12;
  bytes[10] = 0x59;
  bytes[11] = 0x20;
  bytes[17] = (uint8_t)fake->configuration_count;
  return fake->device_result;
}

static long fake_configuration(void *opaque, int fd, uint8_t index,
                               uint8_t *bytes, size_t capacity,
                               size_t *length) {
  struct fake *fake = opaque;
  assert(fd == fake->open_result);
  assert(index < fake->configuration_count);
  assert(capacity == PS5_USB_MAX_CONFIG_BYTES);
  ++fake->configurations;
  memcpy(bytes, valid_config, sizeof valid_config);
  if (fake->invalid_config)
    bytes[2] = 0;
  *length = fake->oversized_config ? capacity + 1 : sizeof valid_config;
  return fake->config_result;
}

static long fake_close(void *opaque, int fd) {
  struct fake *fake = opaque;
  assert(fd == fake->directory_result || fd == fake->open_result);
  ++fake->closed;
  return fake->close_result;
}

static void fake_log(void *opaque, const char *bytes, size_t length) {
  struct fake *fake = opaque;
  assert(fake->output_length + length < sizeof fake->output);
  memcpy(fake->output + fake->output_length, bytes, length);
  fake->output_length += length;
  fake->output[fake->output_length] = 0;
}

static long fake_active(void *opaque, int fd, int *index) {
  struct fake *fake = opaque;
  assert(fd == fake->open_result);
  *index = fake->active_index;
  return fake->active_result;
}

static long fake_driver(void *opaque, int fd, uint8_t index, char *bytes,
                        size_t capacity, size_t *length) {
  struct fake *fake = opaque;
  assert(fd == fake->open_result && index == 0 && capacity == 128);
  ++fake->driver_calls;
  const char *text = fake->driver_text ? fake->driver_text : "";
  size_t count = strlen(text) + 1;
  assert(count <= capacity);
  memcpy(bytes, text, count);
  *length = fake->oversized_driver ? capacity + 1 : (count == 1 ? 0 : count);
  return fake->driver_result;
}

static int run(struct fake *fake) {
  static struct ps5_usb_probe_workspace workspace;
  const struct ps5_usb_probe_ops ops = {fake,
                                        fake_open,
                                        fake_next,
                                        fake_device,
                                        fake_configuration,
                                        fake_close,
                                        fake_log,
                                        fake->bindings ? fake_active : NULL,
                                        fake->bindings ? fake_driver : NULL};
  return ps5_usb_probe_with_ops(&ops, &workspace);
}

static struct fake fresh(void) {
  return (struct fake){.names = 1,
                       .directory_result = 4,
                       .open_result = 0,
                       .configuration_count = 1};
}

int main(void) {
  struct fake fake = fresh();
  assert(run(&fake) == 0);
  assert(fake.opened == 1 && fake.devices == 1 && fake.configurations == 1);
  assert(fake.closed == 2); /* fd zero is valid and closed exactly once. */
  assert(strstr(fake.output, "inventory=complete"));
  assert(strstr(fake.output, "vid=1286 pid=2059"));
  assert(strstr(fake.output, "transport=Unsupported"));

  fake = fresh();
  fake.bindings = 1;
  fake.driver_text = "ubt0: <must not log description>";
  assert(run(&fake) == 0 && fake.driver_calls == 1 && fake.closed == 2);
  assert(strstr(fake.output, "driver=ubt0 ownership=Unknown"));
  assert(!strstr(fake.output, "must not log"));
  fake = fresh();
  fake.bindings = 1;
  assert(run(&fake) == 0 && fake.driver_calls == 1);
  assert(strstr(fake.output, "driver=Unknown binding=unknown"));
  fake = fresh();
  fake.bindings = 1;
  fake.active_result = -25;
  assert(run(&fake) == 1 && fake.driver_calls == 0 && fake.closed == 2);
  fake = fresh();
  fake.bindings = 1;
  fake.active_index = 1;
  assert(run(&fake) == 1 && fake.driver_calls == 0);
  fake = fresh();
  fake.bindings = 1;
  fake.driver_result = -13;
  assert(run(&fake) == 1 && fake.driver_calls == 1 && fake.closed == 2);
  fake = fresh();
  fake.bindings = 1;
  fake.driver_text = "ubt0\nprivate-data";
  assert(run(&fake) == 1 && !strstr(fake.output, "private-data"));
  fake = fresh();
  fake.bindings = 1;
  fake.oversized_driver = 1;
  assert(run(&fake) == 1 && fake.closed == 2);

  fake = fresh();
  fake.configuration_count = 2;
  assert(run(&fake) == 0 && fake.configurations == 2 && fake.closed == 2);

  fake = fresh();
  fake.directory_result = -13;
  assert(run(&fake) == 1 && fake.opened == 0 && fake.closed == 0);
  assert(strstr(fake.output, "raw_result=-13"));

  fake = fresh();
  fake.open_result = -6;
  assert(run(&fake) == 1 && fake.devices == 0 && fake.closed == 1);
  assert(strstr(fake.output, "raw_result=-6"));

  fake = fresh();
  fake.device_result = -1;
  assert(run(&fake) == 1 && fake.configurations == 0 && fake.closed == 2);
  fake = fresh();
  fake.invalid_device = 1;
  assert(run(&fake) == 1 && fake.configurations == 0 && fake.closed == 2);
  fake = fresh();
  fake.configuration_count = 0;
  assert(run(&fake) == 1 && fake.configurations == 0 && fake.closed == 2);
  fake = fresh();
  fake.config_result = -25;
  assert(run(&fake) == 1 && fake.closed == 2);
  assert(strstr(fake.output, "raw_result=-25"));
  fake = fresh();
  fake.invalid_config = 1;
  assert(run(&fake) == 1 && fake.closed == 2);
  assert(strstr(fake.output, "parse_configuration raw_result=-2"));
  fake = fresh();
  fake.oversized_config = 1;
  assert(run(&fake) == 1 && fake.closed == 2);
  assert(strstr(fake.output, "incomplete=returned_length"));
  fake = fresh();
  fake.close_result = -5;
  assert(run(&fake) == 1 && fake.closed == 2);
  fake = fresh();
  fake.enumeration_result = -5;
  assert(run(&fake) == 1 && fake.closed == 2);
  assert(strstr(fake.output, "operation=enumerate raw_result=-5"));
  assert(!strstr(fake.output, "local_error="));

  const long local_errors[] = {PS5_USB_PROBE_DIRECTORY_MALFORMED,
                               PS5_USB_PROBE_DIRECTORY_READ_LIMIT,
                               PS5_USB_PROBE_NAME_TOO_LONG};
  const char *local_labels[] = {"malformed_directory", "directory_read_limit",
                                "overlong_ugen_name"};
  for (size_t i = 0; i < sizeof local_errors / sizeof local_errors[0]; ++i) {
    fake = fresh();
    fake.names = 0;
    fake.enumeration_result = local_errors[i];
    assert(run(&fake) == 1 && fake.closed == 1 && fake.opened == 0);
    assert(strstr(fake.output, "operation=enumerate local_error="));
    assert(strstr(fake.output, local_labels[i]));
    assert(!strstr(fake.output, "operation=enumerate raw_result="));
  }
  fake = fresh();
  fake.unterminated_name = 1;
  assert(run(&fake) == 1 && fake.closed == 1 && fake.opened == 0);
  assert(
      strstr(fake.output, "local_error=-10003 incomplete=overlong_ugen_name"));
  const char *invalid_names[] = {"",
                                 "ugen",
                                 "ugen.1",
                                 "ugen0.",
                                 "ugen0..1",
                                 "ugen-1.1",
                                 "ugen0.1/../../etc",
                                 "ugen0.1x"};
  for (size_t i = 0; i < sizeof invalid_names / sizeof invalid_names[0]; ++i) {
    fake = fresh();
    fake.name_override = invalid_names[i];
    assert(run(&fake) == 0 && fake.closed == 1 && fake.opened == 0);
  }

  fake = fresh();
  fake.names = PS5_USB_PROBE_MAX_NODES;
  assert(run(&fake) == 0 && fake.closed == 65 && fake.opened == 64);
  fake = fresh();
  fake.names = PS5_USB_PROBE_MAX_NODES + 1;
  assert(run(&fake) == 1 && fake.closed == 65 && fake.opened == 64);
  assert(strstr(fake.output, "incomplete=ugen_node_limit"));
  fake = fresh();
  fake.names = 4097;
  fake.irrelevant_names = 1;
  assert(run(&fake) == 1 && fake.closed == 1 && fake.opened == 0);
  assert(strstr(fake.output, "incomplete=directory_entry_limit"));
  fake = fresh();
  fake.names = 4096;
  fake.irrelevant_names = 1;
  assert(run(&fake) == 0 && fake.closed == 1 && fake.opened == 0);

  static struct ps5_usb_probe_workspace workspace;
  struct ps5_usb_probe_ops invalid = {0};
  assert(ps5_usb_probe_with_ops(NULL, &workspace) == -1);
  assert(ps5_usb_probe_with_ops(&invalid, &workspace) == -1);
  assert(ps5_usb_probe_with_ops(&invalid, NULL) == -1);
  puts("USB read-only probe callback tests passed");
  return 0;
}
