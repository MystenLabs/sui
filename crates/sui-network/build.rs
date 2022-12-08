// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    env,
    path::{Path, PathBuf},
};
use tonic_build::manual::{Builder, Method, Service};

type Result<T> = ::std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn main() -> Result<()> {
    let out_dir = if env::var("DUMP_GENERATED_GRPC").is_ok() {
        PathBuf::from("")
    } else {
        PathBuf::from(env::var("OUT_DIR")?)
    };

    let codec_path = "mysten_network::codec::BincodeCodec";

    let validator_service = Service::builder()
        .name("Validator")
        .package("sui.validator")
        .comment("The Validator interface")
        .method(
            Method::builder()
                .name("transaction")
                .route_name("Transaction")
                .input_type("sui_types::messages::Transaction")
                .output_type("sui_types::messages::TransactionInfoResponse")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            Method::builder()
                .name("handle_certificate")
                .route_name("CertifiedTransaction")
                .input_type("sui_types::messages::CertifiedTransaction")
                .output_type("sui_types::messages::TransactionInfoResponse")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            Method::builder()
                .name("account_info")
                .route_name("AccountInfo")
                .input_type("sui_types::messages::AccountInfoRequest")
                .output_type("sui_types::messages::AccountInfoResponse")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            Method::builder()
                .name("object_info")
                .route_name("ObjectInfo")
                .input_type("sui_types::messages::ObjectInfoRequest")
                .output_type("sui_types::messages::ObjectInfoResponse")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            Method::builder()
                .name("transaction_info")
                .route_name("TransactionInfo")
                .input_type("sui_types::messages::TransactionInfoRequest")
                .output_type("sui_types::messages::TransactionInfoResponse")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            Method::builder()
                .name("checkpoint")
                .route_name("Checkpoint")
                .input_type("sui_types::messages_checkpoint::CheckpointRequest")
                .output_type("sui_types::messages_checkpoint::CheckpointResponse")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            Method::builder()
                .name("batch_info")
                .route_name("FollowTxStream")
                .input_type("sui_types::messages::BatchInfoRequest")
                .output_type("sui_types::messages::BatchInfoResponseItem")
                .server_streaming()
                .codec_path(codec_path)
                .build(),
        )
        .method(
            Method::builder()
                .name("committee_info")
                .route_name("CommitteeInfo")
                .input_type("sui_types::messages::CommitteeInfoRequest")
                .output_type("sui_types::messages::CommitteeInfoResponse")
                .codec_path(codec_path)
                .build(),
        )
        .build();

    Builder::new()
        .out_dir(&out_dir)
        .compile(&[validator_service]);

    build_anemo_services(&out_dir);

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=DUMP_GENERATED_GRPC");

    Ok(())
}

fn build_anemo_services(out_dir: &Path) {
    let discovery = anemo_build::manual::Service::builder()
        .name("Discovery")
        .package("sui")
        .method(
            anemo_build::manual::Method::builder()
                .name("get_external_address")
                .route_name("GetExternalAddress")
                .request_type("()")
                .response_type("std::net::SocketAddr")
                .codec_path("anemo::rpc::codec::BincodeCodec")
                .build(),
        )
        .method(
            anemo_build::manual::Method::builder()
                .name("get_known_peers")
                .route_name("GetKnownPeers")
                .request_type("()")
                .response_type("crate::discovery::GetKnownPeersResponse")
                .codec_path("anemo::rpc::codec::BincodeCodec")
                .build(),
        )
        .build();

    let state_sync = anemo_build::manual::Service::builder()
        .name("StateSync")
        .package("sui")
        .method(
            anemo_build::manual::Method::builder()
                .name("push_checkpoint_summary")
                .route_name("PushCheckpointSummary")
                .request_type("sui_types::messages_checkpoint::CertifiedCheckpointSummary")
                .response_type("()")
                .codec_path("anemo::rpc::codec::BincodeCodec")
                .build(),
        )
        .method(
            anemo_build::manual::Method::builder()
                .name("get_checkpoint_summary")
                .route_name("GetCheckpointSummary")
                .request_type("crate::state_sync::GetCheckpointSummaryRequest")
                .response_type("Option<sui_types::messages_checkpoint::CertifiedCheckpointSummary>")
                .codec_path("anemo::rpc::codec::BincodeCodec")
                .build(),
        )
        .method(
            anemo_build::manual::Method::builder()
                .name("get_checkpoint_contents")
                .route_name("GetCheckpointContents")
                .request_type("sui_types::messages_checkpoint::CheckpointContentsDigest")
                .response_type("Option<sui_types::messages_checkpoint::CheckpointContents>")
                .codec_path("anemo::rpc::codec::BincodeCodec")
                .build(),
        )
        .method(
            anemo_build::manual::Method::builder()
                .name("get_transaction_and_effects")
                .route_name("GetTransactionAndEffects")
                .request_type("sui_types::base_types::ExecutionDigests")
                .response_type("Option<(sui_types::messages::CertifiedTransaction, sui_types::messages::TransactionEffects)>")
                .codec_path("anemo::rpc::codec::BincodeCodec")
                .build(),
        )
        .build();

    anemo_build::manual::Builder::new()
        .out_dir(out_dir)
        .compile(&[discovery, state_sync]);
}
