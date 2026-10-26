// SPDX-License-Identifier: GPL-3.0-or-later

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

const INPUTS: &[&str] = &[
    "tools/test_sbc.py",
    "tools/target_sbc.py",
    "tools/inspect_archive.py",
    "platform/codec/ps5_bt_sbc.c",
    "platform/codec/ps5_bt_sbc.h",
    "platform/codec/ps5_bt_sbc_freestanding.h",
    "third_party/sbc/VENDORED.md",
    "third_party/sbc/patches/0001-generic-c-only.patch",
    "third_party/sbc/patches/0002-caller-storage.patch",
    "third_party/sbc/patches/0003-freestanding-static.patch",
    "third_party/sbc/upstream/AUTHORS",
    "third_party/sbc/upstream/COPYING.LIB",
    "third_party/sbc/upstream/sbc/sbc.c",
    "third_party/sbc/upstream/sbc/sbc.h",
    "third_party/sbc/upstream/sbc/sbc_math.h",
    "third_party/sbc/upstream/sbc/sbc_primitives.c",
    "third_party/sbc/upstream/sbc/sbc_primitives.h",
    "third_party/sbc/upstream/sbc/sbc_private.h",
    "third_party/sbc/upstream/sbc/sbc_tables.h",
];

fn repository_root(manifest_dir: &Path) -> PathBuf {
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("bt-codec must remain under crates/")
        .to_path_buf()
}

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let root = repository_root(&manifest_dir);
    for input in INPUTS {
        println!("cargo:rerun-if-changed={}", root.join(input).display());
    }
    for variable in [
        "PYTHON",
        "HOST_CC",
        "CC",
        "AR",
        "PS5_BT_SBC_ARCHIVE",
        "PS5_BT_SBC_NM",
    ] {
        println!("cargo:rerun-if-env-changed={variable}");
    }

    let host_feature = env::var_os("CARGO_FEATURE_HOST_LIBSBC").is_some();
    let target_feature = env::var_os("CARGO_FEATURE_TARGET_LIBSBC").is_some();
    assert!(
        !(host_feature && target_feature),
        "host-libsbc and target-libsbc are mutually exclusive"
    );
    if target_feature {
        assert_eq!(
            env::var("TARGET").unwrap(),
            "x86_64-unknown-none",
            "target-libsbc only supports the reviewed x86_64-unknown-none Rust target"
        );
        let archive = PathBuf::from(
            env::var_os("PS5_BT_SBC_ARCHIVE")
                .expect("target-libsbc requires PS5_BT_SBC_ARCHIVE from tools/target_sbc.py"),
        )
        .canonicalize()
        .expect("target SBC archive does not exist");
        assert_eq!(
            archive.file_name().and_then(|s| s.to_str()),
            Some("libps5_bt_sbc.a")
        );
        println!("cargo:rerun-if-changed={}", archive.display());
        let python = env::var("PYTHON").unwrap_or_else(|_| "python3".into());
        let nm = env::var("PS5_BT_SBC_NM")
            .expect("target-libsbc requires PS5_BT_SBC_NM (LLVM 18.1.8 llvm-nm)");
        let result = Command::new(python)
            .current_dir(&root)
            .arg(root.join("tools/target_sbc.py"))
            .arg("--verify-archive")
            .arg(&archive)
            .arg("--nm")
            .arg(nm)
            .output()
            .expect("failed to verify target SBC archive");
        assert!(
            result.status.success(),
            "target SBC archive verification failed:\n{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        println!(
            "cargo:rustc-link-search=native={}",
            archive.parent().unwrap().display()
        );
        // Keep the native archive external to the Rust archive for LGPL replacement.
        println!("cargo:rustc-link-lib=static:-bundle=ps5_bt_sbc");
        return;
    }
    if !host_feature {
        return;
    }

    let host = env::var("HOST").expect("Cargo did not supply HOST");
    let target = env::var("TARGET").expect("Cargo did not supply TARGET");
    assert_eq!(
        target, host,
        "feature `host-libsbc` is host-only; do not compile it with a host C compiler for {target}"
    );

    let output_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let python = env::var("PYTHON").unwrap_or_else(|_| "python3".into());
    let compiler = env::var("HOST_CC")
        .or_else(|_| env::var("CC"))
        .unwrap_or_else(|_| "cc".into());
    let archiver = env::var("AR").unwrap_or_else(|_| "ar".into());
    let result = Command::new(&python)
        .current_dir(&root)
        .arg(root.join("tools/test_sbc.py"))
        .arg("--build-archive")
        .arg(&output_dir)
        .arg("--cc")
        .arg(&compiler)
        .arg("--ar")
        .arg(&archiver)
        .output()
        .unwrap_or_else(|error| panic!("failed to execute {python}: {error}"));
    if !result.status.success() {
        panic!(
            "verified host libsbc archive build failed:\n{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }

    println!("cargo:rustc-link-search=native={}", output_dir.display());
    println!("cargo:rustc-link-lib=static=ps5_bt_sbc");
}
