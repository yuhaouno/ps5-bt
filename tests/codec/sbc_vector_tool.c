/* SPDX-License-Identifier: GPL-3.0-or-later */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef USE_PS5_BT_WRAPPER
#include "ps5_bt_sbc.h"
#else
#include "sbc/sbc.h"
#endif

#define TOOL_STORAGE_CAPACITY 8192U

struct aligned_storage {
  _Alignas(64) uint8_t bytes[TOOL_STORAGE_CAPACITY];
};

static void fill_pcm(int16_t pcm[256], int silence) {
  uint32_t i;

  for (i = 0; i < 256; ++i)
    pcm[i] = silence ? 0
                     : (int16_t)((i * UINT32_C(257) + UINT32_C(0x1357)) ^
                                 (i << (i & 3)));
}

static int encode_frame(uint32_t mode, uint32_t bitpool, int silence) {
  int16_t pcm[256];
  uint8_t frame[119];
  size_t frame_len;

  fill_pcm(pcm, silence);
#ifdef USE_PS5_BT_WRAPPER
  struct aligned_storage storage;
  uint32_t consumed = 0;
  uint32_t written = 0;

  memset(storage.bytes, 0, sizeof(storage.bytes));
  if (ps5_bt_sbc_init(storage.bytes, ps5_bt_sbc_storage_size(), 48000, mode, 16,
                      8, PS5_BT_SBC_ALLOCATION_LOUDNESS, bitpool) != 0)
    return 10;
  if (ps5_bt_sbc_encode(storage.bytes, ps5_bt_sbc_storage_size(), pcm, 256,
                        frame, sizeof(frame), &consumed, &written) != 0 ||
      consumed != 256)
    return 11;
  if (ps5_bt_sbc_finish(storage.bytes, ps5_bt_sbc_storage_size()) != 0)
    return 12;
  frame_len = written;
#else
  sbc_t codec;
  ssize_t written = 0;
  ssize_t consumed;

  if (sbc_init(&codec, 0) < 0)
    return 20;
  codec.frequency = SBC_FREQ_48000;
  codec.mode = mode == 0 ? SBC_MODE_STEREO : SBC_MODE_JOINT_STEREO;
  codec.blocks = SBC_BLK_16;
  codec.subbands = SBC_SB_8;
  codec.allocation = SBC_AM_LOUDNESS;
  codec.bitpool = (uint8_t)bitpool;
  codec.endian = SBC_LE;
  consumed =
      sbc_encode(&codec, pcm, sizeof(pcm), frame, sizeof(frame), &written);
  sbc_finish(&codec);
  if (consumed != (ssize_t)sizeof(pcm) || written <= 0)
    return 21;
  frame_len = (size_t)written;
#endif
  return fwrite(frame, 1, frame_len, stdout) == frame_len ? 0 : 30;
}

#ifndef USE_PS5_BT_WRAPPER
static int decode_frame(void) {
  uint8_t frame[119];
  uint8_t pcm[512];
  size_t frame_len = fread(frame, 1, sizeof(frame), stdin);
  size_t written = 0;
  ssize_t consumed;
  sbc_t codec;

  if (frame_len == 0 || fgetc(stdin) != EOF || sbc_init(&codec, 0) < 0)
    return 40;
  codec.endian = SBC_LE;
  consumed = sbc_decode(&codec, frame, frame_len, pcm, sizeof(pcm), &written);
  sbc_finish(&codec);
  if (consumed != (ssize_t)frame_len || written != sizeof(pcm))
    return 41;
  return fwrite(pcm, 1, written, stdout) == written ? 0 : 42;
}
#endif

int main(int argc, char **argv) {
  char *end;
  unsigned long mode;
  unsigned long bitpool;

#ifndef USE_PS5_BT_WRAPPER
  if (argc == 2 && strcmp(argv[1], "decode") == 0)
    return decode_frame();
#endif
  if (argc != 5 || strcmp(argv[1], "encode") != 0)
    return 2;
  mode = strtoul(argv[2], &end, 10);
  if (*end != '\0' || mode > 1)
    return 3;
  bitpool = strtoul(argv[3], &end, 10);
  if (*end != '\0' || bitpool < 2 || bitpool > 53)
    return 4;
  if (strcmp(argv[4], "silence") != 0 && strcmp(argv[4], "pattern") != 0)
    return 5;
  return encode_frame((uint32_t)mode, (uint32_t)bitpool,
                      strcmp(argv[4], "silence") == 0);
}
