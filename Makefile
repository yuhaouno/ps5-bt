# SPDX-License-Identifier: GPL-3.0-or-later
.PHONY: check host-smoke rust-archive ps5-link usb-descriptors-test check-c-sanitize
HOST_CC ?= cc
USB_TEST_SOURCES := platform/ps5/diagnostics/usb_descriptors.c tests/platform/usb_descriptors_test.c
USB_TEST_FLAGS := -std=c11 -Wall -Wextra -Werror -pedantic -Iplatform/ps5/diagnostics

check:
	cargo fmt --all -- --check
	cargo check --workspace --all-targets --locked
	cargo test --workspace --locked
	cargo clippy --workspace --all-targets --locked -- -D warnings
	python3 tools/host_check.py
	$(MAKE) usb-descriptors-test
	$(MAKE) rust-archive

usb-descriptors-test:
	mkdir -p build/host/usb-diagnostics
	$(HOST_CC) $(USB_TEST_FLAGS) $(USB_TEST_SOURCES) -o build/host/usb-diagnostics/test
	build/host/usb-diagnostics/test

check-c-sanitize:
	mkdir -p build/host/usb-diagnostics
	$(HOST_CC) $(USB_TEST_FLAGS) -fsanitize=address,undefined -fno-omit-frame-pointer $(USB_TEST_SOURCES) -o build/host/usb-diagnostics/test-sanitized
	build/host/usb-diagnostics/test-sanitized

host-smoke:
	python3 tools/host_check.py

rust-archive:
	cargo build -p ps5-runtime --release --target x86_64-unknown-none --locked
	python3 tools/inspect_archive.py target/x86_64-unknown-none/release/libps5_runtime.a

ps5-link:
	$(MAKE) -f platform/ps5/payload.mk
