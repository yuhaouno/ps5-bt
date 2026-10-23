# SPDX-License-Identifier: GPL-3.0-or-later
.PHONY: check host-smoke rust-archive ps5-link

check:
	cargo fmt --all -- --check
	cargo check --workspace --all-targets --locked
	cargo test --workspace --locked
	cargo clippy --workspace --all-targets --locked -- -D warnings
	python3 tools/host_check.py
	$(MAKE) rust-archive

host-smoke:
	python3 tools/host_check.py

rust-archive:
	cargo build -p ps5-runtime --release --target x86_64-unknown-none --locked
	python3 tools/inspect_archive.py target/x86_64-unknown-none/release/libps5_runtime.a

ps5-link:
	$(MAKE) -f platform/ps5/payload.mk
