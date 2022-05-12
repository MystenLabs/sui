// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tonic_build::manual::{Builder, Method, Service};

type Result<T> = ::std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[test]
fn bootstrap() {
    let out_dir = PathBuf::from(std::env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("generated");
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
                .name("confirmation_transaction")
                .route_name("ConfirmationTransaction")
                .input_type("sui_types::messages::CertifiedTransaction")
                .output_type("sui_types::messages::TransactionInfoResponse")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            Method::builder()
                .name("consensus_transaction")
                .route_name("ConsensusTransaction")
                .input_type("sui_types::messages::ConsensusTransaction")
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
                .name("batch_info")
                .route_name("BatchInfo")
                .input_type("sui_types::messages::BatchInfoRequest")
                .output_type("sui_types::messages::BatchInfoResponseItem")
                .server_streaming()
                .codec_path(codec_path)
                .build(),
        )
        .build();

    Builder::new()
        .out_dir(&out_dir)
        .compile(&[validator_service]);

    prepend_license(&out_dir).unwrap();

    let status = Command::new("git")
        .arg("diff")
        .arg("--exit-code")
        .arg("--")
        .arg(format!("{}", out_dir.display()))
        .status()
        .unwrap();

    if !status.success() {
        panic!("You should commit the protobuf files");
    }
}

fn prepend_license(directory: &Path) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            prepend_license_to_file(&path)?;
        }
    }
    Ok(())
}

const LICENSE_HEADER: &str = "\
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
";

fn prepend_license_to_file(file: &Path) -> Result<()> {
    let mut contents = fs::read_to_string(file)?;
    contents.insert_str(0, LICENSE_HEADER);
    fs::write(file, &contents)?;
    Ok(())
}
