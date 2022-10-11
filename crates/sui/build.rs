// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{env, process::Command};

/// Save revision info to environment variable
fn main() {
    if env::var("GIT_REVISION").is_err() {
        let output = Command::new("git")
            .args(["describe", "--always", "--dirty", "--exclude", "*"])
            .output()
            .unwrap();
        if !output.status.success() {
            panic!(
                "failed to run git command: {}",
                output.stderr.escape_ascii()
            );
        }
        let git_rev = String::from_utf8(output.stdout).unwrap().trim().to_owned();

        println!("cargo:rustc-env=GIT_REVISION={}", git_rev);
        println!("cargo:rerun-if-changed=build.rs");
    }
}
