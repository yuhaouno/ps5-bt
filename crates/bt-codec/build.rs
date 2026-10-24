// SPDX-License-Identifier: GPL-3.0-or-later

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

const INPUTS: &[&str] = &[
    "tools/test_sbc.py",
    "platform/codec/ps5_bt_sbc.c",
    "platform/codec/ps5_bt_sbc.h",
    "third_party/sbc/VENDORED.md",
    "third_party/sbc/patches/0001-generic-c-only.patch",
    "third_party/sbc/patches/0002-caller-storage.patch",
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
    for variable in ["PYTHON", "HOST_CC", "CC", "AR"] {
        println!("cargo:rerun-if-env-changed={variable}");
    }

    if env::var_os("CARGO_FEATURE_HOST_LIBSBC").is_none() {
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
