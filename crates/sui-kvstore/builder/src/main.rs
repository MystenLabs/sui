// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Helper to build Rust bindings for BigTable.
/// Used as a separate project to avoid requiring developers to install the `protoc` dependency.

fn main() -> Result<(), std::io::Error> {
    let out_dir = std::env::current_dir()?.join("../src/bigtable/proto");
    let googleapis = std::env::current_dir()?.join("googleapis");
    tonic_build::configure()
        .build_client(true)
        .build_server(false)
        .out_dir(&out_dir)
        .compile(
            &[googleapis.join("google/bigtable/v2/bigtable.proto")],
            &[googleapis],
        )?;
    Ok(())
}
