// SPDX-License-Identifier: GPL-3.0-or-later
use std::{env, path::PathBuf, process::Command};
fn run(command: &mut Command) {
    assert!(
        command
            .status()
            .expect("failed to execute native store build tool")
            .success(),
        "native store build tool failed"
    );
}
fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    for file in [
        "platform/store/bond_file_store.c",
        "platform/store/bond_file_store.h",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(file).display());
    }
    println!("cargo:rerun-if-env-changed=CC");
    println!("cargo:rerun-if-env-changed=AR");
    if env::var_os("CARGO_FEATURE_NATIVE_POSIX").is_none() {
        return;
    }
    assert_eq!(
        env::var("HOST").unwrap(),
        env::var("TARGET").unwrap(),
        "native-posix only builds host system headers; use external-posix and an independently reviewed target C shim"
    );
    assert!(
        env::var("CARGO_CFG_UNIX").is_ok(),
        "native-posix requires a Unix host"
    );
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    run(
        Command::new(env::var_os("CC").unwrap_or_else(|| "cc".into()))
            .args([
                "-std=c11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-Wpedantic",
                "-c",
            ])
            .arg(root.join("platform/store/bond_file_store.c"))
            .arg("-o")
            .arg(out.join("bond_file_store.o")),
    );
    run(
        Command::new(env::var_os("AR").unwrap_or_else(|| "ar".into()))
            .arg("crs")
            .arg(out.join("libps5_bt_bond_store.a"))
            .arg(out.join("bond_file_store.o")),
    );
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=ps5_bt_bond_store");
}
