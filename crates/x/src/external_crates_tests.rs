// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

pub fn run() -> crate::Result<()> {
    // change into the external-crates/move directory
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../external-crates/");
    std::env::set_current_dir(&path).expect("Unable to change into `external-crates` directory");

    // execute a command to cd to path and run the ls command
    let mut cmd = Command::new("sh")
        .arg("tests.sh")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();

    match cmd.wait() {
        Ok(status) => {
            if status.success() {
                Ok(())
            } else {
                Err(anyhow!("failed to wait on process"))
            }
        }
        Err(err) => Err(anyhow!("failed to wait on process: {}", err)),
    }
}
