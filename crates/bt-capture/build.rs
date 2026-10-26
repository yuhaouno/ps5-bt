// SPDX-License-Identifier: GPL-3.0-or-later
use std::{env, path::PathBuf, process::Command};
fn run(command: &mut Command) {
    assert!(
        command
            .status()
            .expect("run host capture fixture tool")
            .success(),
        "host capture fixture build failed"
    );
}
fn main() {
    println!("cargo:rerun-if-env-changed=CC");
    println!("cargo:rerun-if-env-changed=AR");
    if env::var_os("CARGO_FEATURE_HOST_AVCAP2_TESTS").is_none() {
        return;
    }
    assert_eq!(
        env::var("HOST").unwrap(),
        env::var("TARGET").unwrap(),
        "host-avcap2-tests cannot be used for a cross target"
    );
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let sources = [
        "platform/ps5/capture/capture_stream.c",
        "tests/platform/capture_stream_fixture.c",
    ];
    let mut objects = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        println!("cargo:rerun-if-changed={}", root.join(source).display());
        let object = output.join(format!("capture-{index}.o"));
        run(
            Command::new(env::var_os("CC").unwrap_or_else(|| "cc".into()))
                .args([
                    "-std=c11",
                    "-DPS5_CAPTURE_STREAM_HOST_TEST",
                    "-Wall",
                    "-Wextra",
                    "-Wpedantic",
                    "-Werror",
                ])
                .arg("-I")
                .arg(root.join("platform/ps5/capture"))
                .arg("-I")
                .arg(root.join("tests/platform"))
                .arg("-c")
                .arg(root.join(source))
                .arg("-o")
                .arg(&object),
        );
        objects.push(object);
    }
    for header in [
        "platform/ps5/capture/capture_stream.h",
        "tests/platform/capture_stream_fixture.h",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(header).display());
    }
    let archive = output.join("libcapture_host_fixture.a");
    run(
        Command::new(env::var_os("AR").unwrap_or_else(|| "ar".into()))
            .arg("crs")
            .arg(&archive)
            .args(objects),
    );
    println!("cargo:rustc-link-search=native={}", output.display());
    println!("cargo:rustc-link-lib=static=capture_host_fixture");
}
