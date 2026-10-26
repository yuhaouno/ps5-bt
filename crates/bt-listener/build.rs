// SPDX-License-Identifier: GPL-3.0-or-later
use std::{env, path::PathBuf, process::Command};
fn run(command: &mut Command) {
    assert!(
        command
            .status()
            .expect("listener build tool unavailable")
            .success(),
        "listener native build failed"
    );
}
fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    for path in [
        "platform/control/tcp_listener.c",
        "platform/control/tcp_listener.h",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(path).display());
    }
    println!("cargo:rerun-if-env-changed=CC");
    println!("cargo:rerun-if-env-changed=AR");
    if env::var_os("CARGO_FEATURE_NATIVE_POSIX").is_none() {
        return;
    }
    assert_eq!(
        env::var("HOST").unwrap(),
        env::var("TARGET").unwrap(),
        "native-posix only uses host system headers; review/link target C separately with external-posix"
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
                "-Wpedantic",
                "-Werror",
                "-c",
            ])
            .arg(root.join("platform/control/tcp_listener.c"))
            .arg("-o")
            .arg(out.join("listener.o")),
    );
    run(
        Command::new(env::var_os("AR").unwrap_or_else(|| "ar".into()))
            .arg("crs")
            .arg(out.join("libps5_bt_listener.a"))
            .arg(out.join("listener.o")),
    );
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=ps5_bt_listener");
}
