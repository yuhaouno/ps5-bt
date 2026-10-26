# SPDX-License-Identifier: GPL-3.0-or-later
.DEFAULT_GOAL := all
.PHONY: all check host-smoke rust-archive ps5-link ps5-native-query usb-descriptors-test native-query-test media-recovery-test check-c-sanitize check-build-tools source-bundle
export LLVM_CONFIG PS5_SDK_SOURCE
HOST_CC ?= cc
USB_TEST_SOURCES := platform/ps5/diagnostics/usb_descriptors.c tests/platform/usb_descriptors_test.c
USB_TEST_FLAGS := -std=c11 -Wall -Wextra -Werror -pedantic -Iplatform/ps5/diagnostics
NATIVE_QUERY_TEST_SOURCES := platform/ps5/diagnostics/native_query.c tests/platform/native_query_test.c
NATIVE_L2CAP_TEST_SOURCES := platform/ps5/diagnostics/native_l2cap_probe.c tests/platform/native_l2cap_probe_test.c
.PHONY: native-l2cap-probe-test ps5-native-l2cap-probe
USB_PROBE_TEST_SOURCES := platform/ps5/diagnostics/usb_probe.c platform/ps5/diagnostics/usb_descriptors.c tests/platform/usb_probe_test.c
CAPTURE_PROBE_TEST_SOURCES := platform/ps5/diagnostics/capture_probe.c platform/ps5/diagnostics/capture_observe.c tests/platform/capture_probe_test.c
CAPTURE_OBSERVE_RUNNER_SOURCES := platform/ps5/diagnostics/capture_probe.c platform/ps5/diagnostics/capture_observe.c tests/platform/capture_observe_runner_test.c
CAPTURE_OBSERVE_TEST_SOURCES := platform/ps5/diagnostics/capture_observe.c tests/platform/capture_observe_test.c
.PHONY: usb-probe-test capture-probe-test capture-observe-test sbc-codec-test ps5-usb-metadata ps5-capture-preflight ps5-capture-once ps5-capture-observe ps5-sbc-selftest sbc-main-test

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
	$(MAKE) native-l2cap-probe-test
	$(MAKE) usb-probe-test
	$(MAKE) capture-probe-test
	$(MAKE) capture-observe-test
	$(MAKE) capture-stream-test
	$(MAKE) audio-pipeline-test
	$(MAKE) scheduler-clock-test
	$(MAKE) sbc-codec-test
	$(MAKE) sbc-main-test
	$(MAKE) continuous-main-test
	python3 tests/codec/target_archive_test.py
	$(MAKE) check-build-tools
	$(MAKE) rust-archive

.PHONY: application-candidates-test
# Optional host development checks; never contacts a PS5. Requires Unix C and Node.
application-candidates-test:
	cargo test --locked -p bt-discovery -p bt-control -p bt-runner
	cargo test --locked -p bt-store -p bt-listener --features bt-store/native-posix,bt-listener/native-posix
	cargo test --locked -p bt-store -p bt-listener --features bt-store/external-posix,bt-listener/external-posix
	cargo clippy --locked -p bt-store -p bt-listener --all-targets --features bt-store/native-posix,bt-listener/native-posix -- -D warnings
	cargo clippy --locked -p bt-store -p bt-listener --all-targets --features bt-store/external-posix,bt-listener/external-posix -- -D warnings
	cargo check --locked -p bt-store -p bt-listener --target x86_64-unknown-none --features bt-store/external-posix,bt-listener/external-posix
	python3 tests/store/run.py --cc "$(HOST_CC)" --sanitize
	python3 tests/control/run.py --cc "$(HOST_CC)" --sanitize
	node --check web/control/control.js
	node web/control/control.test.cjs

check-build-tools:
	python3 tools/test_syscall_checked.py
	python3 tools/build_ps5.py --self-test
	python3 tests/tools/test_diagnostic_result.py
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
	$(HOST_CC) $(USB_TEST_FLAGS) $(CAPTURE_OBSERVE_RUNNER_SOURCES) -o build/host/capture-observe/runner-test
	build/host/capture-observe/runner-test
	$(HOST_CC) $(USB_TEST_FLAGS) -Iplatform/ps5/include -DPS5_BT_CAPTURE_PROBE -DPS5_BT_CAPTURE_OBSERVE tests/platform/capture_main_test.c -o build/host/capture-observe/main-test
	build/host/capture-observe/main-test

ps5-usb-metadata:
	python3 tools/build_ps5.py --usb-metadata --loader-kernel-resolver

ps5-capture-preflight:
	python3 tools/build_ps5.py --capture-preflight --loader-kernel-resolver

ps5-capture-once:
	python3 tools/build_ps5.py --capture-once --loader-kernel-resolver

ps5-capture-observe:
	python3 tools/build_ps5.py --capture-observe --loader-kernel-resolver

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

native-l2cap-probe-test:
	mkdir -p build/host/native-l2cap
	$(HOST_CC) $(USB_TEST_FLAGS) $(NATIVE_L2CAP_TEST_SOURCES) -o build/host/native-l2cap/test
	build/host/native-l2cap/test
	$(HOST_CC) $(USB_TEST_FLAGS) -Iplatform/ps5/include -DPS5_BT_NATIVE_L2CAP_PROBE platform/ps5/diagnostics/native_l2cap_probe.c tests/platform/native_l2cap_main_test.c -o build/host/native-l2cap/main-test
	build/host/native-l2cap/main-test

ps5-native-l2cap-probe:
	python3 tools/build_ps5.py --native-l2cap-probe --loader-kernel-resolver

check-c-sanitize:
	mkdir -p build/host/native-l2cap
	$(HOST_CC) $(USB_TEST_FLAGS) -fsanitize=address,undefined -fno-omit-frame-pointer $(NATIVE_L2CAP_TEST_SOURCES) -o build/host/native-l2cap/test-sanitized
	build/host/native-l2cap/test-sanitized
	$(HOST_CC) $(USB_TEST_FLAGS) -Iplatform/ps5/include -DPS5_BT_NATIVE_L2CAP_PROBE -fsanitize=address,undefined -fno-omit-frame-pointer platform/ps5/diagnostics/native_l2cap_probe.c tests/platform/native_l2cap_main_test.c -o build/host/native-l2cap/main-test-sanitized
	build/host/native-l2cap/main-test-sanitized
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
	$(HOST_CC) $(USB_TEST_FLAGS) -fsanitize=address,undefined -fno-omit-frame-pointer $(CAPTURE_OBSERVE_RUNNER_SOURCES) -o build/host/capture-observe/runner-test-sanitized
	build/host/capture-observe/runner-test-sanitized
	$(HOST_CC) $(USB_TEST_FLAGS) -Iplatform/ps5/include -DPS5_BT_CAPTURE_PROBE -DPS5_BT_CAPTURE_OBSERVE -fsanitize=address,undefined -fno-omit-frame-pointer tests/platform/capture_main_test.c -o build/host/capture-observe/main-test-sanitized
	build/host/capture-observe/main-test-sanitized
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

.PHONY: ps5-sbc-continuous continuous-main-test
ps5-sbc-continuous:
	python3 tools/build_ps5.py --sbc-continuous --loader-kernel-resolver

continuous-main-test:
	mkdir -p build/host/continuous-main
	$(HOST_CC) $(USB_TEST_FLAGS) -Iplatform/ps5/include tests/platform/continuous_main_test.c -o build/host/continuous-main/test
	build/host/continuous-main/test

.PHONY: capture-stream-test ps5-capture-stream capture-entry-test
ps5-capture-stream:
	python3 tools/build_ps5.py --capture-stream --loader-kernel-resolver

capture-entry-test:
	mkdir -p build/host/capture-stream
	$(HOST_CC) $(USB_TEST_FLAGS) -Iplatform/ps5/include tests/platform/capture_entry_test.c -o build/host/capture-stream/entry-test
	build/host/capture-stream/entry-test

capture-stream-test: capture-entry-test
	cargo test -p bt-capture --features host-avcap2-tests --locked
	cargo clippy -p bt-capture --all-targets --features host-avcap2-tests --locked -- -D warnings
	cargo check -p bt-capture --features external-avcap2 --target x86_64-unknown-none --locked
	$(HOST_CC) -std=c11 -Wall -Wextra -Wpedantic -Werror -DPS5_CAPTURE_STREAM_HOST_TEST -fsanitize=address,undefined -fno-omit-frame-pointer -Iplatform/ps5/capture -Itests/platform platform/ps5/capture/capture_stream.c tests/platform/capture_stream_fixture.c tests/platform/capture_stream_test.c -o build/host/capture-stream/test-sanitized
	build/host/capture-stream/test-sanitized

.PHONY: audio-pipeline-test
audio-pipeline-test:
	cargo test -p bt-app -p bt-runner --locked
	cargo test -p bt-runner --features host-libsbc --locked
	cargo clippy -p bt-runner --all-targets --features host-libsbc --locked -- -D warnings
	cargo check -p bt-runner --target x86_64-unknown-none --locked

.PHONY: scheduler-clock-test
scheduler-clock-test:
	mkdir -p build/host/scheduler-clock
	cargo test -p ps5-platform --features host-clock-wait-tests --locked
	cargo clippy -p ps5-platform --all-targets --features host-clock-wait-tests --locked -- -D warnings
	cargo check -p ps5-platform --target x86_64-unknown-none --features external-clock-wait --locked
	$(HOST_CC) -std=c11 -Wall -Wextra -Wpedantic -Werror -fsanitize=address,undefined -fno-omit-frame-pointer -Iplatform/ps5/scheduler -Itests/platform platform/ps5/scheduler/scheduler_clock.c tests/platform/scheduler_clock_fixture.c tests/platform/scheduler_clock_test.c -o build/host/scheduler-clock/test-sanitized
	build/host/scheduler-clock/test-sanitized

.PHONY: resource-factory-test
resource-factory-test:
	cargo test -p ps5-app --locked
	cargo test -p ps5-app --features host-candidate --locked
	cargo clippy -p ps5-app --all-targets --features host-candidate --locked -- -D warnings
	cargo check -p ps5-app --target x86_64-unknown-none --features native-resources --locked

.PHONY: posix-imports-test
posix-imports-test:
	mkdir -p build/host/app-target
	$(HOST_CC) -std=gnu11 -Wall -Wextra -Werror -fsanitize=address,undefined -fno-omit-frame-pointer tests/application/posix_imports_test.c -o build/host/app-target/posix-imports-test
	build/host/app-target/posix-imports-test
