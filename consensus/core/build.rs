// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    env,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn main() -> Result<()> {
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    build_tonic_services(&out_dir);

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=USE_TIDEHUNTER");
    println!("cargo::rustc-check-cfg=cfg(tidehunter)");
    if std::env::var("USE_TIDEHUNTER").is_ok() {
        println!("cargo::rustc-cfg=tidehunter");
    }
    Ok(())
}

fn build_tonic_services(out_dir: &Path) {
    let codec_path = "tonic_prost::ProstCodec";

    let service = tonic_build::manual::Service::builder()
        .name("ConsensusService")
        .package("consensus")
        .comment("Consensus authority interface")
        .method(
            tonic_build::manual::Method::builder()
                .name("send_block")
                .route_name("SendBlock")
                .input_type("crate::network::tonic_network::SendBlockRequest")
                .output_type("crate::network::tonic_network::SendBlockResponse")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            tonic_build::manual::Method::builder()
                .name("subscribe_blocks")
                .route_name("SubscribeBlocks")
                .input_type("crate::network::tonic_network::SubscribeBlocksRequest")
                .output_type("crate::network::tonic_network::SubscribeBlocksResponse")
                .codec_path(codec_path)
                .server_streaming()
                .client_streaming()
                .build(),
        )
        .method(
            tonic_build::manual::Method::builder()
                .name("fetch_blocks")
                .route_name("FetchBlocks")
                .input_type("crate::network::tonic_network::FetchBlocksRequest")
                .output_type("crate::network::tonic_network::FetchBlocksResponse")
                .codec_path(codec_path)
                .server_streaming()
                .build(),
        )
        .method(
            tonic_build::manual::Method::builder()
                .name("fetch_commits")
                .route_name("FetchCommits")
                .input_type("crate::network::tonic_network::FetchCommitsRequest")
                .output_type("crate::network::tonic_network::FetchCommitsResponse")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            tonic_build::manual::Method::builder()
                .name("fetch_latest_blocks")
                .route_name("FetchLatestBlocks")
                .input_type("crate::network::tonic_network::FetchLatestBlocksRequest")
                .output_type("crate::network::tonic_network::FetchLatestBlocksResponse")
                .codec_path(codec_path)
                .server_streaming()
                .build(),
        )
        .method(
            tonic_build::manual::Method::builder()
                .name("get_latest_rounds")
                .route_name("GetLatestRounds")
                .input_type("crate::network::tonic_network::GetLatestRoundsRequest")
                .output_type("crate::network::tonic_network::GetLatestRoundsResponse")
                .codec_path(codec_path)
                .build(),
        )
        .build();

    tonic_build::manual::Builder::new()
        .out_dir(out_dir)
        .compile(&[service]);
}
