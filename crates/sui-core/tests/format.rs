// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test]
#[cfg_attr(msim, ignore)]
fn test_format() {
    // If this test breaks and you intended a format change, you need to run to get the fresh format:
    // # cargo -q run --example generate-format -- print > crates/sui-core/tests/staged/sui.yaml

    let status = std::process::Command::new("cargo")
        .current_dir("..")
        .args(["run", "--example", "generate-format", "--"])
        .arg("test")
        .status()
        .expect("failed to execute process");
    assert!(
        status.success(),
        "\n\
If this test breaks and you intended a format change, you need to run to get the fresh format:\n\
cargo -q run --example generate-format -- print > crates/sui-core/tests/staged/sui.yaml\n\
        "
    );
}
