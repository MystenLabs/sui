// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

fn main() {
    let crate_dir = PathBuf::from(std::env!("CARGO_MANIFEST_DIR"));
    let proto_dir = crate_dir.join("proto");
    let out_dir = crate_dir.join("src").join("bigtable").join("proto");

    println!("cargo:rerun-if-changed={}", proto_dir.display());

    let fds = protox::Compiler::new(&proto_dir)
        .unwrap()
        .include_source_info(false)
        .include_imports(true)
        .open_files(&[proto_dir
            .join("google")
            .join("bigtable")
            .join("v2")
            .join("bigtable.proto")])
        .unwrap()
        .file_descriptor_set();

    tonic_prost_build::configure()
        .build_client(true)
        .build_server(false)
        .out_dir(&out_dir)
        .compile_fds(fds)
        .unwrap();
}
