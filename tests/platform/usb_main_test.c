/* SPDX-License-Identifier: GPL-3.0-or-later */
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#define main diagnostic_main
#include "../../platform/ps5/main.c"
#undef main

static int stage;
static int metadata_status;
int32_t ps5_rust_probe(void) {
  assert(stage == 0);
  stage = 1;
  return 2;
}
int ps5_usb_probe_run(void) {
  assert(stage == 1);
  stage = 2;
  return metadata_status;
}
int main(void) {
  assert(diagnostic_main() == 2 && stage == 2);
  stage = 0;
  metadata_status = -1;
  assert(diagnostic_main() == 1 && stage == 2);
  puts("PASS synthetic USB main ordering and explicit metadata failure");
  return 0;
}
