# SPDX-License-Identifier: GPL-3.0-or-later
# Link-only experiment. No live adapter; no deployment target.
.DEFAULT_GOAL := all
ifeq ($(strip $(PS5_RUNTIME_REVIEW)),)
$(error PS5_RUNTIME_REVIEW must name a saved runtime/scope review; see docs/BUILDING.md)
endif
ifeq ($(wildcard $(PS5_RUNTIME_REVIEW)),)
$(error PS5_RUNTIME_REVIEW must exist)
endif
ifeq ($(strip $(PS5_PAYLOAD_SDK)),)
$(error PS5_PAYLOAD_SDK must name the pinned SDK v0.45; see docs/BUILDING.md)
endif

include $(PS5_PAYLOAD_SDK)/toolchain/prospero.mk

.PHONY: all archive
all: archive
	@mkdir -p build/ps5
	$(CC) -std=c11 -Wall -Wextra -Werror -O2 -Iplatform/ps5/include \
		platform/ps5/main.c platform/ps5/log.c platform/ps5/adapter_stub.c \
		target/x86_64-unknown-none/release/libps5_runtime.a \
		-Wl,--gc-sections -Wl,-Map,build/ps5/bootstrap.map -o build/ps5/bootstrap.elf

archive:
	cargo build -p ps5-runtime --release --target x86_64-unknown-none --locked
