# SPDX-License-Identifier: GPL-3.0-or-later
.DEFAULT_GOAL := all
.PHONY: all check host-smoke rust-archive ps5-link ps5-native-query usb-descriptors-test native-query-test media-recovery-test check-c-sanitize check-build-tools source-bundle
export LLVM_CONFIG PS5_SDK_SOURCE
HOST_CC ?= cc
USB_TEST_SOURCES := platform/ps5/diagnostics/usb_descriptors.c tests/platform/usb_descriptors_test.c
USB_TEST_FLAGS := -std=c11 -Wall -Wextra -Werror -pedantic -Iplatform/ps5/diagnostics
NATIVE_QUERY_TEST_SOURCES := platform/ps5/diagnostics/native_query.c tests/platform/native_query_test.c
USB_PROBE_TEST_SOURCES := platform/ps5/diagnostics/usb_probe.c platform/ps5/diagnostics/usb_descriptors.c tests/platform/usb_probe_test.c
CAPTURE_PROBE_TEST_SOURCES := platform/ps5/diagnostics/capture_probe.c tests/platform/capture_probe_test.c
CAPTURE_OBSERVE_TEST_SOURCES := platform/ps5/diagnostics/capture_observe.c tests/platform/capture_observe_test.c
.PHONY: usb-probe-test capture-probe-test capture-observe-test sbc-codec-test ps5-usb-metadata ps5-capture-preflight ps5-capture-once ps5-sbc-selftest sbc-main-test

all: ps5-link

check:
	cargo fmt --all -- --check
	cargo check --workspace --all-targets --locked
	cargo test --workspace --locked
	$(MAKE) media-recovery-test
	cargo clippy --workspace --all-targets --locked -- -D warnings
	python3 tools/host_check.py
	$(MAKE) usb-descriptors-test
	$(MAKE) native-query-test
	$(MAKE) usb-probe-test
	$(MAKE) capture-probe-test
	$(MAKE) capture-observe-test
	$(MAKE) sbc-codec-test
	$(MAKE) sbc-main-test
	$(MAKE) check-build-tools
	$(MAKE) rust-archive

check-build-tools:
	python3 tools/test_syscall_checked.py
	python3 tools/build_ps5.py --self-test
	python3 tools/package_source.py --self-test
	python3 tools/mvp_test_kit.py --self-test

media-recovery-test:
	cargo run -p bt-core --example media_recovery --locked

sbc-codec-test:
	python3 tools/test_sbc.py --cc "$(HOST_CC)"
	cargo test --locked --offline -p bt-codec --features host-libsbc
	cargo clippy --locked --offline -p bt-codec --features host-libsbc --all-targets -- -D warnings
	cargo run --locked --offline -p bt-codec --features host-libsbc --example pcm_to_sbc
	cargo run --locked --offline -p bt-codec --features host-libsbc --example pcm_to_sbc_rtp_acl
	cargo check --locked --offline -p bt-codec --target x86_64-unknown-none

source-bundle:
	python3 tools/package_source.py

usb-probe-test:
	mkdir -p build/host/usb-probe
	$(HOST_CC) $(USB_TEST_FLAGS) $(USB_PROBE_TEST_SOURCES) -o build/host/usb-probe/test
	build/host/usb-probe/test
	$(HOST_CC) $(USB_TEST_FLAGS) -Iplatform/ps5/include -DPS5_BT_USB_METADATA tests/platform/usb_main_test.c -o build/host/usb-probe/main-test
	build/host/usb-probe/main-test

capture-probe-test:
	mkdir -p build/host/capture-probe
	$(HOST_CC) $(USB_TEST_FLAGS) $(CAPTURE_PROBE_TEST_SOURCES) -o build/host/capture-probe/test
	build/host/capture-probe/test
	$(HOST_CC) $(USB_TEST_FLAGS) -Iplatform/ps5/include -DPS5_BT_CAPTURE_PROBE tests/platform/capture_main_test.c -o build/host/capture-probe/main-test
	build/host/capture-probe/main-test
	$(HOST_CC) $(USB_TEST_FLAGS) -Iplatform/ps5/include -DPS5_BT_CAPTURE_PROBE -DPS5_BT_CAPTURE_PREFLIGHT tests/platform/capture_main_test.c -o build/host/capture-probe/preflight-main-test
	build/host/capture-probe/preflight-main-test

capture-observe-test:
	mkdir -p build/host/capture-observe
	$(HOST_CC) $(USB_TEST_FLAGS) $(CAPTURE_OBSERVE_TEST_SOURCES) -o build/host/capture-observe/test
	build/host/capture-observe/test

ps5-usb-metadata:
	python3 tools/build_ps5.py --usb-metadata --loader-kernel-resolver

ps5-capture-preflight:
	python3 tools/build_ps5.py --capture-preflight --loader-kernel-resolver

ps5-capture-once:
	python3 tools/build_ps5.py --capture-once --loader-kernel-resolver

ps5-sbc-selftest:
	python3 tools/build_ps5.py --sbc-selftest --loader-kernel-resolver

sbc-main-test:
	mkdir -p build/host/sbc-main
	$(HOST_CC) $(USB_TEST_FLAGS) -Iplatform/ps5/include -DPS5_BT_SBC_SELFTEST tests/platform/sbc_main_test.c -o build/host/sbc-main/test
	build/host/sbc-main/test

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
	mkdir -p build/host/usb-probe build/host/capture-probe build/host/capture-observe
	$(HOST_CC) $(USB_TEST_FLAGS) -fsanitize=address,undefined -fno-omit-frame-pointer $(USB_PROBE_TEST_SOURCES) -o build/host/usb-probe/test-sanitized
	build/host/usb-probe/test-sanitized
	$(HOST_CC) $(USB_TEST_FLAGS) -fsanitize=address,undefined -fno-omit-frame-pointer $(CAPTURE_PROBE_TEST_SOURCES) -o build/host/capture-probe/test-sanitized
	build/host/capture-probe/test-sanitized
	$(HOST_CC) $(USB_TEST_FLAGS) -Iplatform/ps5/include -DPS5_BT_CAPTURE_PROBE -fsanitize=address,undefined -fno-omit-frame-pointer tests/platform/capture_main_test.c -o build/host/capture-probe/main-test-sanitized
	build/host/capture-probe/main-test-sanitized
	$(HOST_CC) $(USB_TEST_FLAGS) -Iplatform/ps5/include -DPS5_BT_CAPTURE_PROBE -DPS5_BT_CAPTURE_PREFLIGHT -fsanitize=address,undefined -fno-omit-frame-pointer tests/platform/capture_main_test.c -o build/host/capture-probe/preflight-main-test-sanitized
	build/host/capture-probe/preflight-main-test-sanitized
	$(HOST_CC) $(USB_TEST_FLAGS) -fsanitize=address,undefined -fno-omit-frame-pointer $(CAPTURE_OBSERVE_TEST_SOURCES) -o build/host/capture-observe/test-sanitized
	build/host/capture-observe/test-sanitized
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

.PHONY: ps5-hw1
ps5-hw1:
	python3 tools/build_ps5.py --loader-kernel-resolver

ps5-native-query:
	python3 tools/build_ps5.py --native-query
