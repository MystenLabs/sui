// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(not(target_env = "msvc"))]
    std::env::set_var("PROTOC", protobuf_src::protoc());
    tonic_build::compile_protos("protos/narwhal.proto")?;
    Ok(())
}
