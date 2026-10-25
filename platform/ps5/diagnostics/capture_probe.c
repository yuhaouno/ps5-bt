/* SPDX-License-Identifier: GPL-3.0-only */
#include "capture_probe.h"

#define AVCAP2_EMPTY UINT32_C(0x81950002)
#define AVCAP2_OVERRUN UINT32_C(0x81950004)
#define AVCAP2_ENDED UINT32_C(0x81950008)
#define AVCAP2_INVALID UINT32_C(0x81950001)
#define AVCAP2_READ_NOWAIT UINT32_C(1)

#define SYMBOL_INITIALIZE (UINT32_C(1) << 0)
#define SYMBOL_TERMINATE (UINT32_C(1) << 1)
#define SYMBOL_OPEN (UINT32_C(1) << 2)
#define SYMBOL_CLOSE (UINT32_C(1) << 3)
#define SYMBOL_START (UINT32_C(1) << 4)
#define SYMBOL_STOP (UINT32_C(1) << 5)
#define SYMBOL_READ (UINT32_C(1) << 6)
#define SYMBOL_ALL ((UINT32_C(1) << PS5_CAPTURE_PROBE_SYMBOL_COUNT) - 1u)

typedef int (*load_module_fn)(const char *, size_t, const void *, uint32_t,
                              void *, int *);
typedef int (*initialize_fn)(void);
typedef int (*open_audio_fn)(void **, const void *, uint8_t);
typedef int (*handle_fn)(void *);
typedef int (*read_audio_fn)(void *, void *, size_t, void *, uint32_t);

_Static_assert(sizeof(load_module_fn) == sizeof(void *),
               "PS5 function/data pointer representation");
_Static_assert(sizeof(initialize_fn) == sizeof(void *),
               "PS5 function/data pointer representation");
_Static_assert(sizeof(open_audio_fn) == sizeof(void *),
               "PS5 function/data pointer representation");
_Static_assert(sizeof(handle_fn) == sizeof(void *),
               "PS5 function/data pointer representation");
_Static_assert(sizeof(read_audio_fn) == sizeof(void *),
               "PS5 function/data pointer representation");

struct avcap2_open_param {
  uint64_t size;
  uint64_t unknown_08;
  uint32_t unknown_10;
  uint32_t source;
  uint32_t mode;
  uint32_t unknown_1c;
  uint64_t unknown_20;
  uint32_t bitrate;
  uint32_t padding;
  uint64_t unknown_30;
};

_Static_assert(sizeof(struct avcap2_open_param) == 0x38,
               "Avcap2 OpenAudio parameter size");
_Static_assert(offsetof(struct avcap2_open_param, source) == 0x14,
               "Avcap2 OpenAudio source offset");
_Static_assert(offsetof(struct avcap2_open_param, mode) == 0x18,
               "Avcap2 OpenAudio mode offset");
_Static_assert(offsetof(struct avcap2_open_param, unknown_1c) == 0x1c,
               "Avcap2 OpenAudio unknown_1c offset");

struct resolved_functions {
  initialize_fn initialize;
  initialize_fn terminate;
  open_audio_fn open_audio;
  handle_fn close;
  handle_fn start;
  handle_fn stop;
  read_audio_fn read_audio;
};

enum probe_mode {
  PROBE_PREFLIGHT,
  PROBE_ONCE,
  PROBE_OBSERVE,
};

struct observe_read_context {
  const struct ps5_capture_probe_ops *ops;
  read_audio_fn read_audio;
  void *handle;
  unsigned char *metadata;
  struct ps5_capture_probe_result *result;
};

static _Alignas(16) unsigned char probe_audio[PS5_CAPTURE_PROBE_READ_BYTES];
static int probe_busy;

static void clear_bytes(void *storage, size_t size) {
  unsigned char *bytes = storage;
  for (size_t i = 0; i < size; ++i)
    bytes[i] = 0;
}

static void wipe_bytes(void *storage, size_t size) {
  volatile unsigned char *bytes = storage;
  for (size_t i = 0; i < size; ++i)
    bytes[i] = 0;
}

static void trace(const struct ps5_capture_probe_ops *ops,
                  enum ps5_capture_probe_phase phase, uint32_t ordinal,
                  int entering, int64_t raw_status) {
  if (ops->trace)
    ops->trace(phase, ordinal, entering, raw_status);
}

#define COPY_SYMBOL(destination, symbol)                                       \
  do {                                                                         \
    _Static_assert(sizeof(destination) == sizeof(symbol),                      \
                   "function/data pointer representation");                    \
    const unsigned char *source_bytes = (const unsigned char *)&(symbol);      \
    unsigned char *destination_bytes = (unsigned char *)&(destination);        \
    for (size_t copy_i = 0; copy_i < sizeof(destination); ++copy_i)            \
      destination_bytes[copy_i] = source_bytes[copy_i];                        \
  } while (0)

static enum ps5_capture_probe_status
resolve_one(const struct ps5_capture_probe_ops *ops, int32_t module,
            const char *name, void *function, size_t function_size,
            uint32_t bit, uint32_t ordinal,
            struct ps5_capture_probe_result *result) {
  void *symbol = 0;
  trace(ops, PS5_CAPTURE_PHASE_RESOLVE_AVCAP2, ordinal, 1, 0);
  int64_t raw = ops->lookup_module(module, name, &symbol);
  trace(ops, PS5_CAPTURE_PHASE_RESOLVE_AVCAP2, ordinal, 0, raw);
  result->raw_module_lookup_status = raw;
  if (raw != 0 || !symbol)
    return PS5_CAPTURE_PROBE_UNAVAILABLE;
  const unsigned char *source = (const unsigned char *)&symbol;
  unsigned char *destination = function;
  for (size_t i = 0; i < function_size; ++i)
    destination[i] = source[i];
  result->resolved_mask |= bit;
  return PS5_CAPTURE_PROBE_OK;
}

static enum ps5_capture_probe_status
resolve_avcap2(const struct ps5_capture_probe_ops *ops, int32_t module,
               struct resolved_functions *functions,
               struct ps5_capture_probe_result *result) {
#define RESOLVE(field, name, bit, ordinal)                                     \
  do {                                                                         \
    enum ps5_capture_probe_status resolve_status =                             \
        resolve_one(ops, module, name, &functions->field,                      \
                    sizeof functions->field, bit, ordinal, result);            \
    if (resolve_status != PS5_CAPTURE_PROBE_OK)                                \
      return resolve_status;                                                   \
  } while (0)
  RESOLVE(initialize, "sceAvcap2Initialize", SYMBOL_INITIALIZE, 0);
  RESOLVE(terminate, "sceAvcap2Terminate", SYMBOL_TERMINATE, 1);
  RESOLVE(open_audio, "sceAvcap2OpenAudio", SYMBOL_OPEN, 2);
  RESOLVE(close, "sceAvcap2Close", SYMBOL_CLOSE, 3);
  RESOLVE(start, "sceAvcap2Start", SYMBOL_START, 4);
  RESOLVE(stop, "sceAvcap2Stop", SYMBOL_STOP, 5);
  RESOLVE(read_audio, "sceAvcap2ReadAudio", SYMBOL_READ, 6);
#undef RESOLVE
  return PS5_CAPTURE_PROBE_OK;
}

static void set_primary(struct ps5_capture_probe_result *result,
                        enum ps5_capture_probe_status status,
                        enum ps5_capture_probe_phase phase) {
  if (result->primary_phase == PS5_CAPTURE_PHASE_NONE) {
    result->status = status;
    result->primary_phase = phase;
  }
}

static void classify_read(struct ps5_capture_probe_result *result) {
  uint32_t raw = (uint32_t)result->raw_read_status;
  if (result->raw_read_status > 0) {
    uint32_t bytes = (uint32_t)result->raw_read_status;
    if (bytes > result->read_capacity_bytes ||
        bytes % result->requested_frame_bytes != 0) {
      result->read_kind = PS5_CAPTURE_READ_INVALID_LENGTH;
      set_primary(result, PS5_CAPTURE_PROBE_INVALID_LENGTH,
                  PS5_CAPTURE_PHASE_READ);
      return;
    }
    result->read_kind = PS5_CAPTURE_READ_DATA;
    result->read_bytes = bytes;
    result->whole_requested_frames = 1;
    result->requested_frames = bytes / result->requested_frame_bytes;
  } else if (result->raw_read_status == 0) {
    result->read_kind = PS5_CAPTURE_READ_ZERO;
  } else if (raw == AVCAP2_EMPTY) {
    result->read_kind = PS5_CAPTURE_READ_EMPTY;
  } else if (raw == AVCAP2_OVERRUN) {
    result->read_kind = PS5_CAPTURE_READ_OVERRUN;
    result->discontinuity = 1;
  } else if (raw == AVCAP2_ENDED) {
    result->read_kind = PS5_CAPTURE_READ_ENDED;
    set_primary(result, PS5_CAPTURE_PROBE_FAILED, PS5_CAPTURE_PHASE_READ);
  } else if (raw == AVCAP2_INVALID) {
    result->read_kind = PS5_CAPTURE_READ_INVALID;
    set_primary(result, PS5_CAPTURE_PROBE_FAILED, PS5_CAPTURE_PHASE_READ);
  } else {
    result->read_kind = PS5_CAPTURE_READ_ERROR;
    set_primary(result, PS5_CAPTURE_PROBE_FAILED, PS5_CAPTURE_PHASE_READ);
  }
}

static int32_t observe_read_once(void *raw_context, void *buffer,
                                 size_t capacity) {
  struct observe_read_context *context = raw_context;
  uint32_t ordinal = context->result->observation_read_calls;
  clear_bytes(context->metadata, PS5_CAPTURE_PROBE_METADATA_BYTES);
  context->metadata[0] = PS5_CAPTURE_PROBE_METADATA_BYTES;
  context->result->called_mask |= PS5_CAPTURE_CALL_READ;
  trace(context->ops, PS5_CAPTURE_PHASE_READ, ordinal, 1, 0);
  int32_t raw = context->read_audio(context->handle, buffer, capacity,
                                    context->metadata, AVCAP2_READ_NOWAIT);
  trace(context->ops, PS5_CAPTURE_PHASE_READ, ordinal, 0, raw);
  context->result->raw_read_status = raw;
  return raw;
}

static int observe_now(const struct ps5_capture_probe_ops *ops,
                       struct ps5_capture_probe_result *result,
                       uint32_t ordinal, uint64_t *out) {
  result->called_mask |= PS5_CAPTURE_CALL_OBSERVE_CLOCK;
  trace(ops, PS5_CAPTURE_PHASE_OBSERVE_CLOCK, ordinal, 1, 0);
  int raw = ops->now_us(out);
  trace(ops, PS5_CAPTURE_PHASE_OBSERVE_CLOCK, ordinal, 0, raw);
  result->raw_observation_clock_status = raw;
  return raw;
}

static void copy_observation(struct ps5_capture_probe_result *result,
                             const struct ps5_capture_observe *observation,
                             uint64_t start_us, uint64_t last_read_start_us) {
  result->observation_stop = observation->result.stop;
  result->observation_read_calls = observation->result.read_calls;
  result->observation_empty_calls = observation->result.empty_calls;
  result->observation_overrun_calls = observation->result.overrun_calls;
  result->observation_last_read_start_elapsed_us =
      last_read_start_us - start_us;
  result->discontinuity = observation->result.discontinuity;
  result->raw_read_status = observation->result.raw_last_status;
  switch (observation->result.last_kind) {
  case PS5_CAPTURE_OBSERVE_NOT_READ:
    result->read_kind = PS5_CAPTURE_READ_NOT_CALLED;
    break;
  case PS5_CAPTURE_OBSERVE_EMPTY:
    result->read_kind = PS5_CAPTURE_READ_EMPTY;
    break;
  case PS5_CAPTURE_OBSERVE_DATA:
    result->read_kind = PS5_CAPTURE_READ_DATA;
    result->read_bytes = observation->result.data_bytes;
    break;
  case PS5_CAPTURE_OBSERVE_OVERRUN:
    result->read_kind = PS5_CAPTURE_READ_OVERRUN;
    break;
  case PS5_CAPTURE_OBSERVE_OTHER_ERROR:
    result->read_kind =
        observation->result.stop == PS5_CAPTURE_OBSERVE_STOP_INVALID_LENGTH
            ? PS5_CAPTURE_READ_INVALID_LENGTH
            : PS5_CAPTURE_READ_ERROR;
    break;
  }
}

static enum ps5_capture_probe_status
execute_probe(const struct ps5_capture_probe_ops *ops,
              struct ps5_capture_probe_result *out, enum probe_mode mode) {
  struct ps5_capture_probe_result result = {
      .status = PS5_CAPTURE_PROBE_OK,
      .read_capacity_bytes = PS5_CAPTURE_PROBE_READ_BYTES,
      .requested_frame_bytes = PS5_CAPTURE_PROBE_REQUESTED_FRAME_BYTES,
  };
  struct resolved_functions functions = {0};
  struct avcap2_open_param param = {0};
  _Alignas(16) unsigned char metadata[PS5_CAPTURE_PROBE_METADATA_BYTES];
  load_module_fn load_module = 0;
  void *symbol = 0;
  void *handle = 0;
  int initialized = 0, started = 0;

  if (!out)
    return PS5_CAPTURE_PROBE_INVALID;
  if (!ops || !ops->lookup_current || !ops->lookup_module ||
      (mode == PROBE_OBSERVE && (!ops->wait_before_read || !ops->now_us))) {
    result.status = PS5_CAPTURE_PROBE_INVALID;
    *out = result;
    return result.status;
  }
  if (probe_busy) {
    result.status = PS5_CAPTURE_PROBE_BUSY;
    *out = result;
    return result.status;
  }
  probe_busy = 1;
  clear_bytes(probe_audio, sizeof probe_audio);
  clear_bytes(metadata, sizeof metadata);
  metadata[0] = PS5_CAPTURE_PROBE_METADATA_BYTES;

  result.called_mask |= PS5_CAPTURE_CALL_LOOKUP_LOAD_MODULE;
  trace(ops, PS5_CAPTURE_PHASE_LOOKUP_LOAD_MODULE, 0, 1, 0);
  result.raw_load_lookup_status =
      ops->lookup_current("sceKernelLoadStartModule", &symbol);
  trace(ops, PS5_CAPTURE_PHASE_LOOKUP_LOAD_MODULE, 0, 0,
        result.raw_load_lookup_status);
  if (result.raw_load_lookup_status != 0 || !symbol) {
    set_primary(&result, PS5_CAPTURE_PROBE_UNAVAILABLE,
                PS5_CAPTURE_PHASE_LOOKUP_LOAD_MODULE);
    goto cleanup;
  }
  COPY_SYMBOL(load_module, symbol);

  result.called_mask |= PS5_CAPTURE_CALL_LOAD_IPMI;
  result.raw_ipmi_start_result = 0;
  trace(ops, PS5_CAPTURE_PHASE_LOAD_IPMI, 0, 1, 0);
  result.raw_ipmi_module = load_module("/system/common/lib/libSceIpmi.sprx", 0,
                                       0, 0, 0, &result.raw_ipmi_start_result);
  trace(ops, PS5_CAPTURE_PHASE_LOAD_IPMI, 0, 0,
        result.raw_ipmi_module < 0 ? result.raw_ipmi_module
                                   : result.raw_ipmi_start_result);
  if (result.raw_ipmi_module < 0 || result.raw_ipmi_start_result != 0) {
    set_primary(&result, PS5_CAPTURE_PROBE_UNAVAILABLE,
                PS5_CAPTURE_PHASE_LOAD_IPMI);
    goto cleanup;
  }

  result.called_mask |= PS5_CAPTURE_CALL_LOAD_AVCAP2;
  result.raw_avcap2_start_result = 0;
  trace(ops, PS5_CAPTURE_PHASE_LOAD_AVCAP2, 0, 1, 0);
  result.raw_avcap2_module =
      load_module("/system/common/lib/libSceAvcap2.sprx", 0, 0, 0, 0,
                  &result.raw_avcap2_start_result);
  trace(ops, PS5_CAPTURE_PHASE_LOAD_AVCAP2, 0, 0,
        result.raw_avcap2_module < 0 ? result.raw_avcap2_module
                                     : result.raw_avcap2_start_result);
  if (result.raw_avcap2_module < 0 || result.raw_avcap2_start_result != 0) {
    set_primary(&result, PS5_CAPTURE_PROBE_UNAVAILABLE,
                PS5_CAPTURE_PHASE_LOAD_AVCAP2);
    goto cleanup;
  }

  result.called_mask |= PS5_CAPTURE_CALL_RESOLVE_AVCAP2;
  if (resolve_avcap2(ops, result.raw_avcap2_module, &functions, &result) !=
      PS5_CAPTURE_PROBE_OK) {
    set_primary(&result, PS5_CAPTURE_PROBE_UNAVAILABLE,
                PS5_CAPTURE_PHASE_RESOLVE_AVCAP2);
    goto cleanup;
  }
  if (mode == PROBE_PREFLIGHT)
    goto cleanup;

  result.called_mask |= PS5_CAPTURE_CALL_INITIALIZE;
  trace(ops, PS5_CAPTURE_PHASE_INITIALIZE, 0, 1, 0);
  result.raw_initialize_status = functions.initialize();
  trace(ops, PS5_CAPTURE_PHASE_INITIALIZE, 0, 0, result.raw_initialize_status);
  if (result.raw_initialize_status != 0) {
    set_primary(&result, PS5_CAPTURE_PROBE_FAILED,
                PS5_CAPTURE_PHASE_INITIALIZE);
    goto cleanup;
  }
  initialized = 1;

  param.size = sizeof param;
  param.unknown_08 = 0x18;
  param.source = 2;
  param.mode = 0;
  param.unknown_1c = UINT32_MAX;
  result.called_mask |= PS5_CAPTURE_CALL_OPEN;
  trace(ops, PS5_CAPTURE_PHASE_OPEN, 0, 1, 0);
  result.raw_open_status = functions.open_audio(&handle, &param, 0);
  trace(ops, PS5_CAPTURE_PHASE_OPEN, 0, 0, result.raw_open_status);
  if (result.raw_open_status != 0 || !handle) {
    handle = 0;
    set_primary(&result, PS5_CAPTURE_PROBE_FAILED, PS5_CAPTURE_PHASE_OPEN);
    goto cleanup;
  }

  result.called_mask |= PS5_CAPTURE_CALL_START;
  trace(ops, PS5_CAPTURE_PHASE_START, 0, 1, 0);
  result.raw_start_status = functions.start(handle);
  trace(ops, PS5_CAPTURE_PHASE_START, 0, 0, result.raw_start_status);
  if (result.raw_start_status != 0) {
    set_primary(&result, PS5_CAPTURE_PROBE_FAILED, PS5_CAPTURE_PHASE_START);
    goto cleanup;
  }
  started = 1;

  if (mode == PROBE_ONCE && ops->wait_before_read) {
    result.called_mask |= PS5_CAPTURE_CALL_WAIT_BEFORE_READ;
    trace(ops, PS5_CAPTURE_PHASE_WAIT_BEFORE_READ, 0, 1, 0);
    result.raw_wait_status = ops->wait_before_read(PS5_CAPTURE_PROBE_SETTLE_US);
    trace(ops, PS5_CAPTURE_PHASE_WAIT_BEFORE_READ, 0, 0,
          result.raw_wait_status);
    if (result.raw_wait_status != 0) {
      set_primary(&result, PS5_CAPTURE_PROBE_FAILED,
                  PS5_CAPTURE_PHASE_WAIT_BEFORE_READ);
      goto cleanup;
    }
  }

  if (mode == PROBE_ONCE) {
    result.called_mask |= PS5_CAPTURE_CALL_READ;
    trace(ops, PS5_CAPTURE_PHASE_READ, 0, 1, 0);
    result.raw_read_status = functions.read_audio(
        handle, probe_audio, sizeof probe_audio, metadata, AVCAP2_READ_NOWAIT);
    trace(ops, PS5_CAPTURE_PHASE_READ, 0, 0, result.raw_read_status);
    classify_read(&result);
  } else {
    struct ps5_capture_observe observation = {0};
    struct observe_read_context read_context = {
        .ops = ops,
        .read_audio = functions.read_audio,
        .handle = handle,
        .metadata = metadata,
        .result = &result,
    };
    uint64_t start_us = 0, now_us = 0, last_read_start_us = 0;
    result.observation = 1;
    result.requested_frame_bytes = 0;
    if (observe_now(ops, &result, 0, &start_us) != 0 ||
        start_us > UINT64_MAX - PS5_CAPTURE_PROBE_OBSERVE_TOTAL_US) {
      set_primary(&result, PS5_CAPTURE_PROBE_FAILED,
                  PS5_CAPTURE_PHASE_OBSERVE_CLOCK);
      goto cleanup;
    }
    now_us = start_us;
    last_read_start_us = start_us;
    if (ps5_capture_observe_begin(&observation, start_us,
                                  start_us + PS5_CAPTURE_PROBE_OBSERVE_TOTAL_US,
                                  PS5_CAPTURE_PROBE_OBSERVE_MAX_READS) !=
        PS5_CAPTURE_OBSERVE_OK) {
      set_primary(&result, PS5_CAPTURE_PROBE_FAILED,
                  PS5_CAPTURE_PHASE_OBSERVE_CLOCK);
      goto cleanup;
    }
    for (;;) {
      if (now_us < observation.last_tick) {
        set_primary(&result, PS5_CAPTURE_PROBE_FAILED,
                    PS5_CAPTURE_PHASE_OBSERVE_CLOCK);
        (void)ps5_capture_observe_step(&observation, observation.last_tick, 1,
                                       0, 0, 0, 0);
        copy_observation(&result, &observation, start_us, last_read_start_us);
        break;
      }
      if (now_us < observation.deadline_tick &&
          observation.result.read_calls < observation.max_calls)
        last_read_start_us = now_us;
      if (ps5_capture_observe_step(
              &observation, now_us, 0, observe_read_once, &read_context,
              probe_audio, sizeof probe_audio) != PS5_CAPTURE_OBSERVE_OK) {
        set_primary(&result, PS5_CAPTURE_PROBE_FAILED, PS5_CAPTURE_PHASE_READ);
        (void)ps5_capture_observe_step(&observation, observation.last_tick, 1,
                                       0, 0, 0, 0);
        copy_observation(&result, &observation, start_us, last_read_start_us);
        break;
      }
      copy_observation(&result, &observation, start_us, last_read_start_us);
      if (!observation.active)
        break;

      result.called_mask |= PS5_CAPTURE_CALL_WAIT_BEFORE_READ;
      trace(ops, PS5_CAPTURE_PHASE_OBSERVE_WAIT, observation.result.read_calls,
            1, 0);
      result.raw_wait_status =
          ops->wait_before_read(PS5_CAPTURE_PROBE_OBSERVE_INTERVAL_US);
      trace(ops, PS5_CAPTURE_PHASE_OBSERVE_WAIT, observation.result.read_calls,
            0, result.raw_wait_status);
      if (result.raw_wait_status != 0) {
        set_primary(&result, PS5_CAPTURE_PROBE_FAILED,
                    PS5_CAPTURE_PHASE_OBSERVE_WAIT);
        (void)ps5_capture_observe_step(&observation, now_us, 1, 0, 0, 0, 0);
        copy_observation(&result, &observation, start_us, last_read_start_us);
        break;
      }
      if (observe_now(ops, &result, observation.result.read_calls, &now_us) !=
          0) {
        set_primary(&result, PS5_CAPTURE_PROBE_FAILED,
                    PS5_CAPTURE_PHASE_OBSERVE_CLOCK);
        (void)ps5_capture_observe_step(&observation, observation.last_tick, 1,
                                       0, 0, 0, 0);
        copy_observation(&result, &observation, start_us, last_read_start_us);
        break;
      }
    }
    if (observation.result.stop == PS5_CAPTURE_OBSERVE_STOP_INVALID_LENGTH)
      set_primary(&result, PS5_CAPTURE_PROBE_INVALID_LENGTH,
                  PS5_CAPTURE_PHASE_READ);
    else if (observation.result.stop == PS5_CAPTURE_OBSERVE_STOP_READ_ERROR)
      set_primary(&result, PS5_CAPTURE_PROBE_FAILED, PS5_CAPTURE_PHASE_READ);
    if (ps5_capture_observe_take_cleanup(&observation) !=
        PS5_CAPTURE_OBSERVE_OK)
      set_primary(&result, PS5_CAPTURE_PROBE_FAILED, PS5_CAPTURE_PHASE_READ);
  }

cleanup:
  if (started) {
    started = 0;
    result.called_mask |= PS5_CAPTURE_CALL_STOP;
    trace(ops, PS5_CAPTURE_PHASE_STOP, 0, 1, 0);
    result.raw_stop_status = functions.stop(handle);
    trace(ops, PS5_CAPTURE_PHASE_STOP, 0, 0, result.raw_stop_status);
    if (result.raw_stop_status != 0) {
      result.cleanup_uncertain = 1;
      set_primary(&result, PS5_CAPTURE_PROBE_CLEANUP_FAILED,
                  PS5_CAPTURE_PHASE_STOP);
    }
  }
  if (handle) {
    void *owned_handle = handle;
    handle = 0;
    result.called_mask |= PS5_CAPTURE_CALL_CLOSE;
    trace(ops, PS5_CAPTURE_PHASE_CLOSE, 0, 1, 0);
    result.raw_close_status = functions.close(owned_handle);
    trace(ops, PS5_CAPTURE_PHASE_CLOSE, 0, 0, result.raw_close_status);
    if (result.raw_close_status != 0) {
      result.cleanup_uncertain = 1;
      set_primary(&result, PS5_CAPTURE_PROBE_CLEANUP_FAILED,
                  PS5_CAPTURE_PHASE_CLOSE);
    }
  }
  if (initialized) {
    initialized = 0;
    result.called_mask |= PS5_CAPTURE_CALL_TERMINATE;
    trace(ops, PS5_CAPTURE_PHASE_TERMINATE, 0, 1, 0);
    result.raw_terminate_status = functions.terminate();
    trace(ops, PS5_CAPTURE_PHASE_TERMINATE, 0, 0, result.raw_terminate_status);
    if (result.raw_terminate_status != 0) {
      result.cleanup_uncertain = 1;
      set_primary(&result, PS5_CAPTURE_PROBE_CLEANUP_FAILED,
                  PS5_CAPTURE_PHASE_TERMINATE);
    }
  }
  if (result.primary_phase == PS5_CAPTURE_PHASE_NONE)
    result.primary_phase = PS5_CAPTURE_PHASE_DONE;
  wipe_bytes(metadata, sizeof metadata);
  wipe_bytes(probe_audio, sizeof probe_audio);
  probe_busy = 0;
  *out = result;
  return result.status;
}

enum ps5_capture_probe_status
ps5_capture_probe_run(const struct ps5_capture_probe_ops *ops,
                      struct ps5_capture_probe_result *out) {
  return execute_probe(ops, out, PROBE_ONCE);
}

enum ps5_capture_probe_status
ps5_capture_probe_observe(const struct ps5_capture_probe_ops *ops,
                          struct ps5_capture_probe_result *out) {
  return execute_probe(ops, out, PROBE_OBSERVE);
}

enum ps5_capture_probe_status
ps5_capture_probe_preflight(const struct ps5_capture_probe_ops *ops,
                            struct ps5_capture_probe_result *out) {
  return execute_probe(ops, out, PROBE_PREFLIGHT);
}

struct writer {
  char *data;
  size_t length;
};

static void put_text(struct writer *writer, const char *text) {
  while (*text) {
    if (writer->data)
      writer->data[writer->length] = *text;
    ++writer->length;
    ++text;
  }
}

static void put_unsigned(struct writer *writer, uint64_t value) {
  char digits[20];
  size_t count = 0;
  do {
    digits[count++] = (char)('0' + value % 10);
    value /= 10;
  } while (value);
  while (count)
    if (writer->data)
      writer->data[writer->length++] = digits[--count];
    else {
      ++writer->length;
      --count;
    }
}

static void put_signed(struct writer *writer, int64_t value) {
  uint64_t magnitude = (uint64_t)value;
  if (value < 0) {
    put_text(writer, "-");
    magnitude = UINT64_C(0) - magnitude;
  }
  put_unsigned(writer, magnitude);
}

static void put_hex32(struct writer *writer, uint32_t value) {
  static const char alphabet[] = "0123456789abcdef";
  put_text(writer, "0x");
  for (unsigned shift = 32; shift; shift -= 4) {
    char byte[2] = {alphabet[(value >> (shift - 4)) & 15], 0};
    put_text(writer, byte);
  }
}

static const char *status_name(enum ps5_capture_probe_status status) {
  switch (status) {
  case PS5_CAPTURE_PROBE_OK:
    return "ok";
  case PS5_CAPTURE_PROBE_UNAVAILABLE:
    return "unavailable";
  case PS5_CAPTURE_PROBE_FAILED:
    return "failed";
  case PS5_CAPTURE_PROBE_INVALID_LENGTH:
    return "invalid_length";
  case PS5_CAPTURE_PROBE_CLEANUP_FAILED:
    return "cleanup_failed";
  case PS5_CAPTURE_PROBE_BUSY:
    return "busy";
  case PS5_CAPTURE_PROBE_INVALID:
    return "invalid";
  case PS5_CAPTURE_PROBE_CAPACITY:
    return "capacity";
  default:
    return 0;
  }
}

static const char *phase_name(enum ps5_capture_probe_phase phase) {
  static const char *const names[] = {
      "none",
      "lookup_load",
      "load_ipmi",
      "load_avcap2",
      "resolve",
      "initialize",
      "open",
      "start",
      "read",
      "stop",
      "close",
      "terminate",
      "wait_before_read",
      "done",
      "observe_clock",
      "observe_wait",
  };
  if ((unsigned)phase >= sizeof names / sizeof names[0])
    return 0;
  return names[phase];
}

static const char *observation_stop_name(enum ps5_capture_observe_stop stop) {
  static const char *const names[] = {
      "running",   "data",       "call_limit",     "deadline",
      "cancelled", "read_error", "invalid_length",
  };
  if ((unsigned)stop >= sizeof names / sizeof names[0])
    return 0;
  return names[stop];
}

static const char *read_name(enum ps5_capture_probe_read_kind kind) {
  static const char *const names[] = {
      "not_called", "data",    "zero",  "empty",          "overrun",
      "ended",      "invalid", "error", "invalid_length",
  };
  if ((unsigned)kind >= sizeof names / sizeof names[0])
    return 0;
  return names[kind];
}

static void put_raw_call(struct writer *writer, const char *name,
                         const struct ps5_capture_probe_result *result,
                         uint32_t bit, int32_t raw) {
  put_text(writer, name);
  if (result->called_mask & bit)
    put_signed(writer, raw);
  else
    put_text(writer, "na");
}

static void encode(struct writer *writer,
                   const struct ps5_capture_probe_result *result) {
  put_text(writer, "capture_probe_v1 status=");
  put_text(writer, status_name(result->status));
  put_text(writer, " phase=");
  put_text(writer, phase_name(result->primary_phase));
  put_text(writer, " calls=");
  put_hex32(writer, result->called_mask);
  put_text(writer, " resolved=");
  put_hex32(writer, result->resolved_mask);
  put_text(writer, " capture_invoked=");
  put_unsigned(writer, !!(result->called_mask & PS5_CAPTURE_CALL_INITIALIZE));
  put_text(writer, " load_lookup_status=");
  if (result->called_mask & PS5_CAPTURE_CALL_LOOKUP_LOAD_MODULE)
    put_signed(writer, result->raw_load_lookup_status);
  else
    put_text(writer, "na");
  put_raw_call(writer, " ipmi_module=", result, PS5_CAPTURE_CALL_LOAD_IPMI,
               result->raw_ipmi_module);
  put_raw_call(writer, " ipmi_start_result=", result,
               PS5_CAPTURE_CALL_LOAD_IPMI, result->raw_ipmi_start_result);
  put_raw_call(writer, " avcap2_module=", result, PS5_CAPTURE_CALL_LOAD_AVCAP2,
               result->raw_avcap2_module);
  put_raw_call(writer, " avcap2_start_result=", result,
               PS5_CAPTURE_CALL_LOAD_AVCAP2, result->raw_avcap2_start_result);
  put_text(writer, " module_lookup_status=");
  if (result->called_mask & PS5_CAPTURE_CALL_RESOLVE_AVCAP2)
    put_signed(writer, result->raw_module_lookup_status);
  else
    put_text(writer, "na");
  put_raw_call(writer, " initialize_status=", result,
               PS5_CAPTURE_CALL_INITIALIZE, result->raw_initialize_status);
  put_raw_call(writer, " open_status=", result, PS5_CAPTURE_CALL_OPEN,
               result->raw_open_status);
  put_raw_call(writer, " start_status=", result, PS5_CAPTURE_CALL_START,
               result->raw_start_status);
  put_raw_call(writer, " wait_status=", result,
               PS5_CAPTURE_CALL_WAIT_BEFORE_READ, result->raw_wait_status);
  put_text(writer, " read_kind=");
  put_text(writer, read_name(result->read_kind));
  put_text(writer, " read_raw=");
  if (result->called_mask & PS5_CAPTURE_CALL_READ)
    put_hex32(writer, (uint32_t)result->raw_read_status);
  else
    put_text(writer, "na");
  put_text(writer, " read_capacity_bytes=");
  put_unsigned(writer, result->read_capacity_bytes);
  put_text(writer, " read_bytes=");
  put_unsigned(writer, result->read_bytes);
  put_text(writer, " requested_frame_bytes=");
  put_unsigned(writer, result->requested_frame_bytes);
  put_text(writer, " requested_frames=");
  put_unsigned(writer, result->requested_frames);
  put_text(writer, " whole_requested_frames=");
  put_unsigned(writer, result->whole_requested_frames);
  put_text(writer, " discontinuity=");
  put_unsigned(writer, result->discontinuity);
  put_text(writer, " timestamp_present=");
  put_unsigned(writer, result->timestamp_present);
  put_text(writer, " format=requires_hardware samples_stored=0");
  put_text(writer, " observation=");
  put_unsigned(writer, result->observation);
  put_text(writer, " observation_stop=");
  put_text(writer, observation_stop_name(result->observation_stop));
  put_text(writer, " observation_reads=");
  put_unsigned(writer, result->observation_read_calls);
  put_text(writer, " observation_empty=");
  put_unsigned(writer, result->observation_empty_calls);
  put_text(writer, " observation_overrun=");
  put_unsigned(writer, result->observation_overrun_calls);
  put_text(writer, " observation_last_read_start_elapsed_us=");
  put_unsigned(writer, result->observation_last_read_start_elapsed_us);
  put_text(writer, " observation_clock_status=");
  if (result->called_mask & PS5_CAPTURE_CALL_OBSERVE_CLOCK)
    put_signed(writer, result->raw_observation_clock_status);
  else
    put_text(writer, "na");
  put_raw_call(writer, " stop_status=", result, PS5_CAPTURE_CALL_STOP,
               result->raw_stop_status);
  put_raw_call(writer, " close_status=", result, PS5_CAPTURE_CALL_CLOSE,
               result->raw_close_status);
  put_raw_call(writer, " terminate_status=", result, PS5_CAPTURE_CALL_TERMINATE,
               result->raw_terminate_status);
  put_text(writer, " cleanup_uncertain=");
  put_unsigned(writer, result->cleanup_uncertain);
  put_text(writer, "\n");
}

enum ps5_capture_probe_status
ps5_capture_probe_encode_report(const struct ps5_capture_probe_result *result,
                                char *data, size_t capacity, size_t *out_len) {
  struct writer writer = {0};
  if (!out_len)
    return PS5_CAPTURE_PROBE_INVALID;
  *out_len = 0;
  if (!result || (!data && capacity) || !status_name(result->status) ||
      !phase_name(result->primary_phase) || !read_name(result->read_kind) ||
      !observation_stop_name(result->observation_stop) ||
      result->resolved_mask & ~SYMBOL_ALL ||
      result->whole_requested_frames > 1 || result->discontinuity > 1 ||
      result->timestamp_present > 1 || result->cleanup_uncertain > 1 ||
      result->observation > 1)
    return PS5_CAPTURE_PROBE_INVALID;
  encode(&writer, result);
  *out_len = writer.length;
  if (!data || capacity < writer.length)
    return PS5_CAPTURE_PROBE_CAPACITY;
  writer.data = data;
  writer.length = 0;
  encode(&writer, result);
  return PS5_CAPTURE_PROBE_OK;
}
