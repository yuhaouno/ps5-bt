/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "usb_probe.h"
#include <limits.h>

static size_t text_length(const char *text) {
  size_t length = 0;
  while (text[length])
    ++length;
  return length;
}

static void emit(const struct ps5_usb_probe_ops *ops, const char *text) {
  ops->log(ops->context, text, text_length(text));
}

static void number(const struct ps5_usb_probe_ops *ops, long value) {
  char buffer[1 + sizeof(long) * 3];
  size_t used = 0;
  unsigned long magnitude = (unsigned long)value;
  if (value < 0)
    magnitude = 0ul - magnitude;
  do {
    buffer[used++] = (char)('0' + magnitude % 10);
    magnitude /= 10;
  } while (magnitude);
  if (value < 0)
    buffer[used++] = '-';
  for (size_t i = 0; i < used / 2; ++i) {
    char temporary = buffer[i];
    buffer[i] = buffer[used - i - 1];
    buffer[used - i - 1] = temporary;
  }
  ops->log(ops->context, buffer, used);
}

static void outcome(const struct ps5_usb_probe_ops *ops, const char *operation,
                    long result) {
  emit(ops, "usb_probe operation=");
  emit(ops, operation);
  emit(ops, " raw_result=");
  number(ops, result);
  emit(ops, "\n");
}

static void enumeration_outcome(const struct ps5_usb_probe_ops *ops,
                                long result) {
  const char *reason = NULL;
  switch (result) {
  case PS5_USB_PROBE_DIRECTORY_MALFORMED:
    reason = "malformed_directory";
    break;
  case PS5_USB_PROBE_DIRECTORY_READ_LIMIT:
    reason = "directory_read_limit";
    break;
  case PS5_USB_PROBE_NAME_TOO_LONG:
    reason = "overlong_ugen_name";
    break;
  default:
    outcome(ops, "enumerate", result);
    return;
  }
  emit(ops, "usb_probe operation=enumerate local_error=");
  number(ops, result);
  emit(ops, " incomplete=");
  emit(ops, reason);
  emit(ops, "\n");
}

/* Only actual ugen<decimal>.<decimal> directory entries are device paths. */
static int ugen_name(const char *name, size_t capacity) {
  if (capacity < 8 || name[0] != 'u' || name[1] != 'g' || name[2] != 'e' ||
      name[3] != 'n')
    return 0;
  size_t length = 0;
  while (length < capacity && name[length])
    ++length;
  if (length == capacity)
    return (int)PS5_USB_PROBE_NAME_TOO_LONG;
  size_t i = 4;
  size_t first = i;
  while (i < capacity && name[i] >= '0' && name[i] <= '9')
    ++i;
  if (i == first || i >= capacity || name[i++] != '.')
    return 0;
  first = i;
  while (i < capacity && name[i] >= '0' && name[i] <= '9')
    ++i;
  return i > first && i < capacity && name[i] == '\0';
}

static int binding_report(const struct ps5_usb_probe_ops *ops, int fd,
                          uint8_t count) {
  int incomplete = 0;
  if (count > 32) {
    emit(ops, "usb_probe incomplete=interface_limit\n");
    return 1;
  }
  for (unsigned index = 0; index < count; ++index) {
    char driver[128] = {0};
    size_t length = 0;
    emit(ops, "usb_probe interface_index=");
    number(ops, (long)index);
    emit(ops, "\n");
    long result = ops->interface_driver(ops->context, fd, (uint8_t)index,
                                        driver, sizeof driver, &length);
    outcome(ops, "interface_driver", result);
    if (result != 0 || length > sizeof driver) {
      incomplete = 1;
      emit(ops, "usb_probe driver=Unknown\n");
      continue;
    }
    /* Never log the returned description. Only a bounded nameunit token
     * before ':' or NUL is eligible; embedded controls reject the token.
     */
    size_t token = 0;
    while (token < length && driver[token] != ':' && driver[token] != 0) {
      char byte = driver[token];
      if (!((byte >= 'a' && byte <= 'z') || (byte >= 'A' && byte <= 'Z') ||
            (byte >= '0' && byte <= '9') || byte == '_' || byte == '.' ||
            byte == '-'))
        break;
      ++token;
    }
    if (token == 0) {
      emit(ops, "usb_probe driver=Unknown binding=unknown\n");
    } else if (token == length ||
               (driver[token] != ':' && driver[token] != 0)) {
      emit(ops, "usb_probe driver=Unknown incomplete=invalid_driver_token\n");
      incomplete = 1;
    } else {
      emit(ops, "usb_probe driver=");
      ops->log(ops->context, driver, token);
      emit(ops, " ownership=Unknown\n");
    }
  }
  return incomplete;
}

int ps5_usb_probe_with_ops(const struct ps5_usb_probe_ops *ops,
                           struct ps5_usb_probe_workspace *workspace) {
  if (!ops || !workspace || !ops->open_readonly || !ops->next_name ||
      !ops->device_descriptor || !ops->configuration || !ops->close_fd ||
      !ops->log || (!!ops->active_configuration != !!ops->interface_driver))
    return -1;
  emit(ops, "usb_probe_v1 access=readonly transport=Unsupported\n");
  long directory = ops->open_readonly(ops->context, "/dev");
  outcome(ops, "open_directory", directory);
  if (directory < 0 || directory > INT_MAX)
    return 1;
  int incomplete = 0;
  size_t nodes = 0;
  size_t entries = 0;
  for (;;) {
    char name[PS5_USB_PROBE_NAME_CAPACITY] = {0};
    long next = ops->next_name(ops->context, (int)directory, name, sizeof name);
    if (next == 0)
      break;
    if (next != 1) {
      enumeration_outcome(ops, next);
      incomplete = 1;
      break;
    }
    if (++entries > 4096) {
      emit(ops, "usb_probe incomplete=directory_entry_limit\n");
      incomplete = 1;
      break;
    }
    int candidate = ugen_name(name, sizeof name);
    if (candidate < 0) {
      enumeration_outcome(ops, candidate);
      incomplete = 1;
      break;
    }
    if (!candidate)
      continue;
    if (nodes == PS5_USB_PROBE_MAX_NODES) {
      emit(ops, "usb_probe incomplete=ugen_node_limit\n");
      incomplete = 1;
      break;
    }
    ++nodes;
    char path[5 + PS5_USB_PROBE_NAME_CAPACITY] = "/dev/";
    size_t name_length = text_length(name);
    for (size_t i = 0; i <= name_length; ++i)
      path[5 + i] = name[i];
    emit(ops, "usb_probe node=");
    emit(ops, path);
    emit(ops, "\n");
    long device = ops->open_readonly(ops->context, path);
    outcome(ops, "open_device", device);
    if (device < 0 || device > INT_MAX) {
      incomplete = 1;
      continue;
    }
    uint8_t descriptor[18] = {0};
    long result = ops->device_descriptor(ops->context, (int)device, descriptor,
                                         sizeof descriptor);
    outcome(ops, "device_descriptor", result);
    if (result != 0 || descriptor[0] != sizeof descriptor ||
        descriptor[1] != 1 || descriptor[17] == 0) {
      incomplete = 1;
      if (result == 0)
        emit(ops, "usb_probe incomplete=invalid_device_descriptor\n");
    } else {
      uint16_t vid = (uint16_t)(descriptor[8] | (uint16_t)descriptor[9] << 8);
      uint16_t pid = (uint16_t)(descriptor[10] | (uint16_t)descriptor[11] << 8);
      int active_index = -1;
      if (ops->active_configuration) {
        result =
            ops->active_configuration(ops->context, (int)device, &active_index);
        outcome(ops, "active_configuration", result);
        emit(ops, "usb_probe active_configuration_index=");
        number(ops, active_index);
        emit(ops, "\n");
        if (result != 0 || active_index < 0 || active_index >= descriptor[17]) {
          active_index = -1;
          incomplete = 1;
          emit(ops, "usb_probe binding=Unknown no_interface_queries=1\n");
        }
      }
      for (unsigned index = 0; index < descriptor[17]; ++index) {
        size_t length = 0;
        emit(ops, "usb_probe configuration_index=");
        number(ops, (long)index);
        emit(ops, "\n");
        result = ops->configuration(ops->context, (int)device, (uint8_t)index,
                                    workspace->descriptor,
                                    sizeof workspace->descriptor, &length);
        outcome(ops, "full_descriptor", result);
        if (result != 0 || length > sizeof workspace->descriptor) {
          incomplete = 1;
          if (result == 0)
            emit(ops, "usb_probe incomplete=returned_length\n");
          continue;
        }
        enum ps5_usb_status status = ps5_usb_parse_config(
            workspace->descriptor, length, &workspace->config);
        outcome(ops, "parse_configuration", status);
        if (status != PS5_USB_OK) {
          incomplete = 1;
          continue;
        }
        size_t report_length = 0;
        status = ps5_usb_encode_report(
            vid, pid, &workspace->config, workspace->report,
            sizeof workspace->report, &report_length);
        if (status != PS5_USB_OK) {
          outcome(ops, "encode_report", status);
          incomplete = 1;
          continue;
        }
        ops->log(ops->context, workspace->report, report_length);
        if ((int)index == active_index)
          incomplete |= binding_report(ops, (int)device,
                                       workspace->config.interface_count);
      }
    }
    result = ops->close_fd(ops->context, (int)device);
    outcome(ops, "close_device", result);
    if (result != 0)
      incomplete = 1;
  }
  long result = ops->close_fd(ops->context, (int)directory);
  outcome(ops, "close_directory", result);
  if (result != 0)
    incomplete = 1;
  emit(ops, "usb_probe nodes=");
  number(ops, (long)nodes);
  emit(ops, incomplete ? " inventory=incomplete\n" : " inventory=complete\n");
  return incomplete;
}

#ifdef PS5_BT_USB_PROBE_TARGET
#include "ps5_bt.h"
#include <dev/usb/usb.h>
#include <dev/usb/usb_ioctl.h>
#include <sys/dirent.h>
#include <sys/fcntl.h>
#include <sys/syscall.h>
#include <sys/types.h>

extern long __crt_syscall(long, ...);

struct target_context {
  _Alignas(struct dirent) uint8_t directory_bytes[4096];
  size_t length;
  size_t offset;
  size_t reads;
};

static long target_open(void *context, const char *path) {
  (void)context;
  return __crt_syscall(SYS_open, path, O_RDONLY, 0);
}

static long target_next(void *opaque, int fd, char *name, size_t capacity) {
  struct target_context *context = opaque;
  for (;;) {
    if (context->offset == context->length) {
      if (context->reads++ == 64)
        return PS5_USB_PROBE_DIRECTORY_READ_LIMIT;
      long result = __crt_syscall(SYS_getdents, fd, context->directory_bytes,
                                  sizeof context->directory_bytes);
      if (result <= 0)
        return result;
      if ((unsigned long)result > sizeof context->directory_bytes)
        return PS5_USB_PROBE_DIRECTORY_MALFORMED;
      context->length = (size_t)result;
      context->offset = 0;
    }
    size_t remaining = context->length - context->offset;
    const uint8_t *record = context->directory_bytes + context->offset;
    const size_t header = offsetof(struct dirent, d_name);
    if (remaining < header)
      return PS5_USB_PROBE_DIRECTORY_MALFORMED;
    struct dirent entry = {0};
    /* Record alignment need not be trusted; copy only the fixed header. */
    for (size_t i = 0; i < header; ++i)
      ((uint8_t *)&entry)[i] = record[i];
    if (entry.d_reclen < header + (size_t)entry.d_namlen + 1 ||
        entry.d_reclen > remaining || record[header + entry.d_namlen] != 0)
      return PS5_USB_PROBE_DIRECTORY_MALFORMED;
    context->offset += entry.d_reclen;
    if (entry.d_fileno == 0)
      continue;
    if ((size_t)entry.d_namlen + 1 > capacity) {
      if (entry.d_namlen >= 4 && record[header] == 'u' &&
          record[header + 1] == 'g' && record[header + 2] == 'e' &&
          record[header + 3] == 'n')
        return PS5_USB_PROBE_NAME_TOO_LONG;
      name[0] = 0; /* Long non-ugen entries are irrelevant, not truncated. */
      return 1;
    }
    for (size_t i = 0; i <= entry.d_namlen; ++i)
      name[i] = (char)record[header + i];
    return 1;
  }
}

static long target_device(void *context, int fd, uint8_t *bytes,
                          size_t capacity) {
  (void)context;
  struct usb_device_descriptor descriptor = {0};
  if (capacity != sizeof descriptor)
    return -1;
  long result = __crt_syscall(SYS_ioctl, fd, USB_GET_DEVICE_DESC, &descriptor);
  if (result == 0)
    for (size_t i = 0; i < capacity; ++i)
      bytes[i] = ((const uint8_t *)&descriptor)[i];
  return result;
}

static long target_configuration(void *context, int fd, uint8_t index,
                                 uint8_t *bytes, size_t capacity,
                                 size_t *length) {
  (void)context;
  *length = 0;
  if (capacity > UINT16_MAX)
    return -1;
  struct usb_gen_descriptor descriptor = {0};
  descriptor.ugd_data = bytes;
  descriptor.ugd_maxlen = (uint16_t)capacity;
  descriptor.ugd_config_index = index;
  long result = __crt_syscall(SYS_ioctl, fd, USB_GET_FULL_DESC, &descriptor);
  if (result == 0)
    *length = descriptor.ugd_actlen;
  return result;
}

static long target_close(void *context, int fd) {
  (void)context;
  return __crt_syscall(SYS_close, fd);
}

static long target_active_configuration(void *context, int fd, int *index) {
  (void)context;
  return __crt_syscall(SYS_ioctl, fd, USB_GET_CONFIG, index);
}

static long target_interface_driver(void *context, int fd, uint8_t index,
                                    char *bytes, size_t capacity,
                                    size_t *length) {
  (void)context;
  *length = 0;
  if (capacity != 128)
    return -1;
  struct usb_gen_descriptor descriptor = {0};
  descriptor.ugd_data = bytes;
  descriptor.ugd_maxlen = (uint16_t)capacity;
  descriptor.ugd_iface_index = index;
  long result = __crt_syscall(SYS_ioctl, fd, USB_GET_IFACE_DRIVER, &descriptor);
  if (result == 0)
    *length = descriptor.ugd_actlen;
  return result;
}

static void target_log(void *context, const char *bytes, size_t length) {
  (void)context;
  ps5_platform_log((const uint8_t *)bytes, (uint32_t)length);
}

int ps5_usb_probe_run(void) {
  /* One synchronous invocation only; BSS avoids a ~84 KiB loader stack use.
   * No reentrant/background invocation is allowed by the diagnostic main.
   */
  static struct ps5_usb_probe_workspace workspace;
  struct target_context context = {0};
  const struct ps5_usb_probe_ops ops = {&context,
                                        target_open,
                                        target_next,
                                        target_device,
                                        target_configuration,
                                        target_close,
                                        target_log,
                                        target_active_configuration,
                                        target_interface_driver};
  return ps5_usb_probe_with_ops(&ops, &workspace);
}
#endif
