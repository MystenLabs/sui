// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::io::Result;
fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=BUILD_REMOTE_WRITE");

    // add this env var to build. you'll need protoc installed locally and a copy of the proto files
    if option_env!("BUILD_REMOTE_WRITE").is_some() {
        prost_build::compile_protos(
            &["protobufs/remote.proto", "protobufs/types.proto"],
            &["protobufs/"],
        )?;
    }
    Ok(())
}
