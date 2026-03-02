// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

fn main() {
    // This build script enables tidehunter config used to gate tidehunter in sui codebase
    println!("cargo::rerun-if-env-changed=USE_TIDEHUNTER");
    println!("cargo::rustc-check-cfg=cfg(tidehunter)");
    if std::env::var("USE_TIDEHUNTER").is_ok() {
        println!("cargo::rustc-cfg=tidehunter");
    }

    // Builds proto files.
    println!("cargo::rerun-if-changed=proto/congestion_log.proto");
    let file_descriptors = protox::compile(["proto/congestion_log.proto"], ["proto/"])
        .expect("failed to compile congestion_log.proto");
    prost_build::compile_fds(file_descriptors)
        .expect("failed to generate code from congestion_log.proto");
}
