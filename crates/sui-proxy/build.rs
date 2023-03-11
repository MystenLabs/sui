// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::env;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    protobuf_codegen_pure::Codegen::new()
        .out_dir(out_dir)
        .inputs(&["protobufs/remote.proto"])
        .include("protobufs")
        .run()
        .expect("remote write protobuf codegen failed");
}
