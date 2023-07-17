// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    env,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn main() -> Result<()> {
    let out_dir = if env::var("DUMP_GENERATED_ANEMO").is_ok() {
        PathBuf::from("")
    } else {
        PathBuf::from(env::var("OUT_DIR")?)
    };

    build_anemo_services(&out_dir);

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=DUMP_GENERATED_ANEMO");

    Ok(())
}

fn build_anemo_services(out_dir: &Path) {
    let codec_path = "mysten_network::codec::anemo::BcsSnappyCodec";

    let bench = anemo_build::manual::Service::builder()
        .name("Benchmark")
        .package("anemo_benchmark")
        .method(
            anemo_build::manual::Method::builder()
                .name("send_bytes")
                .route_name("SendBytes")
                .request_type("Vec<u8>")
                .response_type("()")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            anemo_build::manual::Method::builder()
                .name("request_bytes")
                .route_name("RequestBytes")
                .request_type("u32")
                .response_type("Vec<u8>")
                .codec_path(codec_path)
                .build(),
        )
        .build();

    anemo_build::manual::Builder::new()
        .out_dir(out_dir)
        .compile(&[bench]);
}
