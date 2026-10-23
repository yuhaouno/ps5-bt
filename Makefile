# SPDX-License-Identifier: GPL-3.0-or-later
.DEFAULT_GOAL := all
.PHONY: all check host-smoke rust-archive ps5-link ps5-native-query usb-descriptors-test native-query-test check-c-sanitize check-build-tools source-bundle
export LLVM_CONFIG PS5_SDK_SOURCE
HOST_CC ?= cc
USB_TEST_SOURCES := platform/ps5/diagnostics/usb_descriptors.c tests/platform/usb_descriptors_test.c
USB_TEST_FLAGS := -std=c11 -Wall -Wextra -Werror -pedantic -Iplatform/ps5/diagnostics
NATIVE_QUERY_TEST_SOURCES := platform/ps5/diagnostics/native_query.c tests/platform/native_query_test.c

all: ps5-link

check:
	cargo fmt --all -- --check
	cargo check --workspace --all-targets --locked
	cargo test --workspace --locked
	cargo clippy --workspace --all-targets --locked -- -D warnings
	python3 tools/host_check.py
	$(MAKE) usb-descriptors-test
	$(MAKE) native-query-test
	$(MAKE) check-build-tools
	$(MAKE) rust-archive

check-build-tools:
	python3 tools/build_ps5.py --self-test
	python3 tools/package_source.py --self-test

source-bundle:
	python3 tools/package_source.py

usb-descriptors-test:
	mkdir -p build/host/usb-diagnostics
	$(HOST_CC) $(USB_TEST_FLAGS) $(USB_TEST_SOURCES) -o build/host/usb-diagnostics/test
	build/host/usb-diagnostics/test

native-query-test:
	mkdir -p build/host/native-query
	$(HOST_CC) $(USB_TEST_FLAGS) $(NATIVE_QUERY_TEST_SOURCES) -o build/host/native-query/test
	build/host/native-query/test
	$(HOST_CC) $(USB_TEST_FLAGS) -Iplatform/ps5/include -DPS5_BT_NATIVE_QUERY platform/ps5/diagnostics/native_query.c tests/platform/native_main_test.c -o build/host/native-query/main-test
	build/host/native-query/main-test
	$(HOST_CC) $(USB_TEST_FLAGS) -Iplatform/ps5/include tests/platform/native_main_test.c -o build/host/native-query/baseline-main-test
	build/host/native-query/baseline-main-test

check-c-sanitize:
	mkdir -p build/host/usb-diagnostics
	$(HOST_CC) $(USB_TEST_FLAGS) -fsanitize=address,undefined -fno-omit-frame-pointer $(USB_TEST_SOURCES) -o build/host/usb-diagnostics/test-sanitized
	build/host/usb-diagnostics/test-sanitized
	mkdir -p build/host/native-query
	$(HOST_CC) $(USB_TEST_FLAGS) -fsanitize=address,undefined -fno-omit-frame-pointer $(NATIVE_QUERY_TEST_SOURCES) -o build/host/native-query/test-sanitized
	build/host/native-query/test-sanitized
	$(HOST_CC) $(USB_TEST_FLAGS) -Iplatform/ps5/include -DPS5_BT_NATIVE_QUERY -fsanitize=address,undefined -fno-omit-frame-pointer platform/ps5/diagnostics/native_query.c tests/platform/native_main_test.c -o build/host/native-query/main-test-sanitized
	build/host/native-query/main-test-sanitized

host-smoke:
	python3 tools/host_check.py

rust-archive:
	cargo build -p ps5-runtime --release --target x86_64-unknown-none --locked
	python3 tools/inspect_archive.py target/x86_64-unknown-none/release/libps5_runtime.a

ps5-link:
	$(MAKE) -f platform/ps5/payload.mk

ps5-native-query:
	python3 tools/build_ps5.py --native-query
