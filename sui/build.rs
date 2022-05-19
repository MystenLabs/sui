// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{env, process::Command};

/// Save revision info to environment variable
fn main() {
    if env::var("GIT_REV").is_err() {
        let output = Command::new("git")
            .args(&["rev-parse", "--short", "HEAD"])
            .output()
            .unwrap();
        if !output.status.success() {
            panic!(
                "failed to run git command: {:?}",
                output.stderr.escape_ascii()
            );
        }
        let mut git_rev = String::from_utf8(output.stdout).unwrap().trim().to_owned();

        let output = Command::new("git")
            .args(&["diff-index", "--name-only", "HEAD", "--"])
            .output()
            .unwrap();
        if !output.status.success() {
            panic!(
                "failed to run git command: {:?}",
                output.stderr.escape_ascii()
            );
        }
        if !output.stdout.is_empty() {
            git_rev.push_str("-dirty");
        }

        println!("cargo:rustc-env=GIT_REV={}", git_rev);
        println!("cargo:rerun-if-changed=build.rs");
    }
}
