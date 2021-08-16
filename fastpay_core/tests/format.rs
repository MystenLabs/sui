// Copyright (c) Facebook Inc.
// SPDX-License-Identifier: Apache-2.0

#[test]
fn test_format() {
    let status = std::process::Command::new("target/debug/generate-format")
        .current_dir("..")
        .arg("test")
        .status()
        .expect("failed to execute process");
    assert!(status.success());
}
