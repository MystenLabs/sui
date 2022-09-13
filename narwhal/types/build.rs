// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    env,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn main() -> Result<()> {
    let out_dir = if env::var("DUMP_GENERATED_GRPC").is_ok() {
        PathBuf::from("")
    } else {
        PathBuf::from(env::var("OUT_DIR")?)
    };

    let proto_files = &["proto/narwhal.proto"];
    let dirs = &["proto"];

    // Use `Bytes` instead of `Vec<u8>` for bytes fields
    let mut config = prost_build::Config::new();
    config.bytes(["."]);

    tonic_build::configure()
        .out_dir(&out_dir)
        .compile_with_config(config, proto_files, dirs)?;

    build_anemo_services(&out_dir);

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=proto");
    println!("cargo:rerun-if-env-changed=DUMP_GENERATED_GRPC");

    nightly();
    beta();
    stable();

    Ok(())
}

fn build_anemo_services(out_dir: &Path) {
    let primary_to_primary = anemo_build::manual::Service::builder()
        .name("PrimaryToPrimary")
        .package("narwhal")
        .method(
            anemo_build::manual::Method::builder()
                .name("send_message")
                .route_name("SendMessage")
                .request_type("crate::PrimaryMessage")
                .response_type("()")
                .codec_path("anemo::rpc::codec::BincodeCodec")
                .build(),
        )
        .build();

    let primary_to_worker = anemo_build::manual::Service::builder()
        .name("PrimaryToWorker")
        .package("narwhal")
        .method(
            anemo_build::manual::Method::builder()
                .name("send_message")
                .route_name("SendMessage")
                .request_type("crate::PrimaryWorkerMessage")
                .response_type("()")
                .codec_path("anemo::rpc::codec::BincodeCodec")
                .build(),
        )
        .build();

    let worker_to_primary = anemo_build::manual::Service::builder()
        .name("WorkerToPrimary")
        .package("narwhal")
        .method(
            anemo_build::manual::Method::builder()
                .name("send_message")
                .route_name("SendMessage")
                .request_type("crate::WorkerPrimaryMessage")
                .response_type("()")
                .codec_path("anemo::rpc::codec::BincodeCodec")
                .build(),
        )
        .method(
            anemo_build::manual::Method::builder()
                .name("worker_info")
                .route_name("WorkerInfo")
                .request_type("()")
                .response_type("crate::WorkerInfoResponse")
                .codec_path("anemo::rpc::codec::BincodeCodec")
                .build(),
        )
        .build();

    let worker_to_worker = anemo_build::manual::Service::builder()
        .name("WorkerToWorker")
        .package("narwhal")
        .method(
            anemo_build::manual::Method::builder()
                .name("send_message")
                .route_name("SendMessage")
                .request_type("crate::WorkerMessage")
                .response_type("()")
                .codec_path("anemo::rpc::codec::BincodeCodec")
                .build(),
        )
        .build();

    anemo_build::manual::Builder::new()
        .out_dir(out_dir)
        .compile(&[
            primary_to_primary,
            primary_to_worker,
            worker_to_primary,
            worker_to_worker,
        ]);
}

#[rustversion::nightly]
fn nightly() {
    println!("cargo:rustc-cfg=nightly");
}

#[rustversion::not(nightly)]
fn nightly() {}

#[rustversion::beta]
fn beta() {
    println!("cargo:rustc-cfg=beta");
}

#[rustversion::not(beta)]
fn beta() {}

#[rustversion::stable]
fn stable() {
    println!("cargo:rustc-cfg=stable");
}

#[rustversion::not(stable)]
fn stable() {}
