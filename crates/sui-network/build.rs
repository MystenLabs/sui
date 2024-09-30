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

    let codec_path = "mysten_network::codec::BcsCodec";

    let validator_service = Service::builder()
        .name("Validator")
        .package("sui.validator")
        .comment("The Validator interface")
        .method(
            Method::builder()
                .name("transaction")
                .route_name("Transaction")
                .input_type("sui_types::transaction::Transaction")
                .output_type("sui_types::messages_grpc::HandleTransactionResponse")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            Method::builder()
                .name("transaction_v2")
                .route_name("TransactionV2")
                .input_type("sui_types::messages_grpc::HandleTransactionRequestV2")
                .output_type("sui_types::messages_grpc::HandleTransactionResponseV2")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            Method::builder()
                .name("handle_certificate_v2")
                .route_name("CertifiedTransactionV2")
                .input_type("sui_types::transaction::CertifiedTransaction")
                .output_type("sui_types::messages_grpc::HandleCertificateResponseV2")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            Method::builder()
                .name("handle_certificate_v3")
                .route_name("CertifiedTransactionV3")
                .input_type("sui_types::messages_grpc::HandleCertificateRequestV3")
                .output_type("sui_types::messages_grpc::HandleCertificateResponseV3")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            Method::builder()
                .name("handle_soft_bundle_certificates_v3")
                .route_name("SoftBundleCertifiedTransactionsV3")
                .input_type("sui_types::messages_grpc::HandleSoftBundleCertificatesRequestV3")
                .output_type("sui_types::messages_grpc::HandleSoftBundleCertificatesResponseV3")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            Method::builder()
                .name("submit_certificate")
                .route_name("SubmitCertificate")
                .input_type("sui_types::transaction::CertifiedTransaction")
                .output_type("sui_types::messages_grpc::SubmitCertificateResponse")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            Method::builder()
                .name("object_info")
                .route_name("ObjectInfo")
                .input_type("sui_types::messages_grpc::ObjectInfoRequest")
                .output_type("sui_types::messages_grpc::ObjectInfoResponse")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            Method::builder()
                .name("transaction_info")
                .route_name("TransactionInfo")
                .input_type("sui_types::messages_grpc::TransactionInfoRequest")
                .output_type("sui_types::messages_grpc::TransactionInfoResponse")
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
                .name("checkpoint_v2")
                .route_name("CheckpointV2")
                .input_type("sui_types::messages_checkpoint::CheckpointRequestV2")
                .output_type("sui_types::messages_checkpoint::CheckpointResponseV2")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            Method::builder()
                .name("get_system_state_object")
                .route_name("GetSystemStateObject")
                .input_type("sui_types::messages_grpc::SystemStateRequest")
                .output_type("sui_types::sui_system_state::SuiSystemState")
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
    let codec_path = "mysten_network::codec::anemo::BcsSnappyCodec";

    let discovery = anemo_build::manual::Service::builder()
        .name("Discovery")
        .package("sui")
        .method(
            anemo_build::manual::Method::builder()
                .name("get_known_peers")
                .route_name("GetKnownPeers")
                .request_type("()")
                .response_type("crate::discovery::GetKnownPeersResponse")
                .codec_path(codec_path)
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
                .codec_path(codec_path)
                .build(),
        )
        .method(
            anemo_build::manual::Method::builder()
                .name("get_checkpoint_summary")
                .route_name("GetCheckpointSummary")
                .request_type("crate::state_sync::GetCheckpointSummaryRequest")
                .response_type("Option<sui_types::messages_checkpoint::CertifiedCheckpointSummary>")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            anemo_build::manual::Method::builder()
                .name("get_checkpoint_contents")
                .route_name("GetCheckpointContents")
                .request_type("sui_types::messages_checkpoint::CheckpointContentsDigest")
                .response_type("Option<sui_types::messages_checkpoint::FullCheckpointContents>")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            anemo_build::manual::Method::builder()
                .name("get_checkpoint_availability")
                .route_name("GetCheckpointAvailability")
                .request_type("()")
                .response_type("crate::state_sync::GetCheckpointAvailabilityResponse")
                .codec_path(codec_path)
                .build(),
        )
        .build();

    let randomness = anemo_build::manual::Service::builder()
        .name("Randomness")
        .package("sui")
        .method(
            anemo_build::manual::Method::builder()
                .name("send_signatures")
                .route_name("SendSignatures")
                .request_type("crate::randomness::SendSignaturesRequest")
                .response_type("()")
                .codec_path(codec_path)
                .build(),
        )
        .build();

    anemo_build::manual::Builder::new()
        .out_dir(out_dir)
        .compile(&[discovery, state_sync, randomness]);
}
