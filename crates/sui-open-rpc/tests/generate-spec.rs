// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test]
#[cfg_attr(msim, ignore)]
fn test_json_rpc_spec() {
    // If this test breaks and you intended a json rpc schema change, you need to run to get the fresh schema:
    // # cargo -q run --example generate-json-rpc-spec -- record
    let status = std::process::Command::new("cargo")
        .current_dir("..")
        .args(["run", "--example", "generate-json-rpc-spec", "--"])
        .arg("test")
        .status()
        .expect("failed to execute process");
    assert!(
        status.success(),
        "\n\
If this test breaks and you intended a json rpc schema change, you need to run to get the fresh schema:\n\
cargo -q run --example generate-json-rpc-spec -- record\n\
        "
    );
}
