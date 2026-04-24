// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::PathBuf;

use protox::prost::Message as _;

fn main() {
    cynic_codegen::register_schema("rpc")
        .from_sdl_file("../sui-indexer-alt-graphql/schema.graphql")
        .expect("Failed to find GraphQL Schema")
        .as_default()
        .unwrap();

    let crate_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let proto_dir = crate_dir.join("proto");
    let out_dir = crate_dir.join("src/proto/generated");

    println!("cargo:rerun-if-changed={}", proto_dir.display());

    fs::create_dir_all(&out_dir).expect("create proto out dir");

    let proto_files = vec![proto_dir.join("sui/forking/v1alpha/forking_service.proto")];
    let file_descriptors =
        protox::compile(proto_files, [&proto_dir]).expect("compile forking_service.proto");

    let encoded_descriptors = file_descriptors.encode_to_vec();
    fs::write(out_dir.join("forking_descriptor.bin"), &encoded_descriptors)
        .expect("write forking_descriptor.bin");

    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .out_dir(&out_dir)
        .compile_fds(file_descriptors)
        .expect("generate forking_service gRPC code");
}
