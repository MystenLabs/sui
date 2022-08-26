// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{env, path::PathBuf};
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
                .name("epoch_info")
                .route_name("Epoch")
                .input_type("sui_types::messages::EpochRequest")
                .output_type("sui_types::messages::EpochResponse")
                .codec_path(codec_path)
                .build(),
        )
        .build();

    Builder::new()
        .out_dir(&out_dir)
        .compile(&[validator_service]);

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=DUMP_GENERATED_GRPC");

    Ok(())
}
