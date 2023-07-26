// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    env,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn main() -> Result<()> {
    #[cfg(not(target_env = "msvc"))]
    std::env::set_var("PROTOC", protobuf_src::protoc());

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
    let mut automock_attribute = anemo_build::Attributes::default();
    automock_attribute.push_trait(".", r#"#[mockall::automock]"#);

    let codec_path = "mysten_network::codec::anemo::BcsSnappyCodec";

    let primary_to_primary = anemo_build::manual::Service::builder()
        .name("PrimaryToPrimary")
        .package("narwhal")
        .attributes(automock_attribute.clone())
        .method(
            anemo_build::manual::Method::builder()
                .name("send_certificate")
                .route_name("SendCertificate")
                .request_type("crate::SendCertificateRequest")
                .response_type("crate::SendCertificateResponse")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            anemo_build::manual::Method::builder()
                .name("request_vote")
                .route_name("RequestVote")
                .request_type("crate::RequestVoteRequest")
                .response_type("crate::RequestVoteResponse")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            anemo_build::manual::Method::builder()
                .name("fetch_certificates")
                .route_name("FetchCertificates")
                .request_type("crate::FetchCertificatesRequest")
                .response_type("crate::FetchCertificatesResponse")
                .codec_path(codec_path)
                .build(),
        )
        .build();

    let primary_to_worker = anemo_build::manual::Service::builder()
        .name("PrimaryToWorker")
        .package("narwhal")
        .attributes(automock_attribute.clone())
        .method(
            anemo_build::manual::Method::builder()
                .name("synchronize")
                .route_name("Synchronize")
                .request_type("crate::WorkerSynchronizeMessage")
                .response_type("()")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            anemo_build::manual::Method::builder()
                .name("fetch_batches")
                .route_name("FetchBatches")
                .request_type("crate::FetchBatchesRequest")
                .response_type("crate::FetchBatchesResponse")
                .codec_path(codec_path)
                .build(),
        )
        .build();

    let worker_to_primary = anemo_build::manual::Service::builder()
        .name("WorkerToPrimary")
        .package("narwhal")
        .attributes(automock_attribute.clone())
        // TODO: Remove once we have upgraded to protocol version 12.
        .method(
            anemo_build::manual::Method::builder()
                .name("report_our_batch")
                .route_name("ReportOurBatch")
                .request_type("crate::WorkerOurBatchMessage")
                .response_type("()")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            anemo_build::manual::Method::builder()
                .name("report_own_batch")
                .route_name("ReportOwnBatch")
                .request_type("crate::WorkerOwnBatchMessage")
                .response_type("()")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            anemo_build::manual::Method::builder()
                .name("report_others_batch")
                .route_name("ReportOthersBatch")
                .request_type("crate::WorkerOthersBatchMessage")
                .response_type("()")
                .codec_path(codec_path)
                .build(),
        )
        .build();

    let worker_to_worker = anemo_build::manual::Service::builder()
        .name("WorkerToWorker")
        .package("narwhal")
        .attributes(automock_attribute)
        .method(
            anemo_build::manual::Method::builder()
                .name("report_batch")
                .route_name("ReportBatch")
                .request_type("crate::WorkerBatchMessage")
                .response_type("()")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            anemo_build::manual::Method::builder()
                .name("request_batches")
                .route_name("RequestBatches")
                .request_type("crate::RequestBatchesRequest")
                .response_type("crate::RequestBatchesResponse")
                .codec_path(codec_path)
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
