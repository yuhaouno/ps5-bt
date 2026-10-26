// SPDX-License-Identifier: GPL-3.0-or-later
use std::{env, path::PathBuf, process::Command};
fn run(command: &mut Command) {
    assert!(
        command
            .status()
            .expect("run host clock fixture tool")
            .success(),
        "host clock fixture build failed"
    );
}
fn main() {
    println!("cargo:rerun-if-env-changed=CC");
    println!("cargo:rerun-if-env-changed=AR");
    if env::var_os("CARGO_FEATURE_HOST_CLOCK_WAIT_TESTS").is_none() {
        return;
    }
    assert_eq!(
        env::var("HOST").unwrap(),
        env::var("TARGET").unwrap(),
        "host-clock-wait-tests cannot be cross compiled"
    );
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let mut objects = Vec::new();
    for (index, source) in [
        "platform/ps5/scheduler/scheduler_clock.c",
        "tests/platform/scheduler_clock_fixture.c",
    ]
    .iter()
    .enumerate()
    {
        println!("cargo:rerun-if-changed={}", root.join(source).display());
        let object = out.join(format!("clock-{index}.o"));
        run(
            Command::new(env::var_os("CC").unwrap_or_else(|| "cc".into()))
                .args(["-std=c11", "-Wall", "-Wextra", "-Wpedantic", "-Werror"])
                .arg("-I")
                .arg(root.join("platform/ps5/scheduler"))
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
        "platform/ps5/scheduler/scheduler_clock.h",
        "tests/platform/scheduler_clock_fixture.h",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(header).display());
    }
    run(
        Command::new(env::var_os("AR").unwrap_or_else(|| "ar".into()))
            .arg("crs")
            .arg(out.join("libclock_host_fixture.a"))
            .args(objects),
    );
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=clock_host_fixture");
}
