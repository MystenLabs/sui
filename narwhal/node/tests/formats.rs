// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test]
fn test_format() {
    // If this test breaks and you intended a format change, you need to run to get the fresh format:
    // # cargo -q run --example narwhal-generate-format -- print > narwhal/node/tests/staged/narwhal.yaml

    let status = std::process::Command::new("cargo")
        .current_dir("..")
        .args(["run", "--example", "narwhal-generate-format", "--"])
        .arg("test")
        .status()
        .expect("failed to execute process");
    assert!(status.success());
}
