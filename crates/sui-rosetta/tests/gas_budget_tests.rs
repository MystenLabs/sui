// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use fastcrypto::encoding::{Encoding, Hex};
use serde::Deserialize;
use serde_json::json;

use rosetta_client::start_rosetta_test_server;
use sui_keys::keystore::AccountKeystore;
use sui_rosetta::operations::Operations;
use sui_rosetta::types::{
    ConstructionCombineRequest, ConstructionCombineResponse, ConstructionMetadataRequest,
    ConstructionMetadataResponse, ConstructionPayloadsRequest, ConstructionPayloadsResponse,
    ConstructionPreprocessRequest, ConstructionPreprocessResponse, ConstructionSubmitRequest,
    NetworkIdentifier, PreprocessMetadata, Signature, SignatureType, SuiEnv,
    TransactionIdentifierResponse,
};
use sui_types::crypto::SuiSignature;
use test_cluster::TestClusterBuilder;

use crate::rosetta_client::RosettaEndpoint;

#[allow(dead_code)]
mod rosetta_client;

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum TransactionIdentifierResponseResult {
    #[allow(unused)]
    Success(TransactionIdentifierResponse),
    Error(RosettaSubmitGasError),
}

#[derive(Deserialize, PartialEq, Eq, Debug)]
struct RosettaSubmitGasError {
    pub code: i32,
    pub message: String,
    pub description: Option<String>,
    pub retriable: bool,
    pub details: RosettaSubmitGasErrorDetails,
}

#[derive(Deserialize, PartialEq, Eq, Debug)]
struct RosettaSubmitGasErrorDetails {
    error: String,
}

async fn pay_with_gas_budget(budget: u64) -> TransactionIdentifierResponseResult {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    tokio::time::sleep(Duration::from_secs(1)).await;

    let network_identifier = NetworkIdentifier {
        blockchain: "sui".to_string(),
        network: SuiEnv::LocalNet,
    };

    let ops: Operations = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"PaySui",
            "account": { "address" : recipient.to_string() },
            "amount" : { "value": "1000000000" , "currency": { "symbol": "SUI", "decimals": 9}}
        },{
            "operation_identifier":{"index":1},
            "type":"PaySui",
            "account": { "address" : sender.to_string() },
            "amount" : {
                "value": "-1000000000",
                "currency": {
                    "symbol": "SUI",
                    "decimals": 9,
                }
            },
        }]
    ))
    .unwrap();

    let metadata = Some(PreprocessMetadata {
        budget: Some(budget),
    });

    let preprocess: ConstructionPreprocessResponse = rosetta_client
        .call(
            RosettaEndpoint::Preprocess,
            &ConstructionPreprocessRequest {
                network_identifier: network_identifier.clone(),
                operations: ops.clone(),
                metadata,
            },
        )
        .await;
    println!("Preprocess : {preprocess:?}");
    assert_eq!(preprocess.options.as_ref().unwrap().budget.unwrap(), budget);

    let metadata: ConstructionMetadataResponse = rosetta_client
        .call(
            RosettaEndpoint::Metadata,
            &ConstructionMetadataRequest {
                network_identifier: network_identifier.clone(),
                options: preprocess.options,
                public_keys: vec![],
            },
        )
        .await;
    println!("Metadata : {metadata:?}");
    assert_eq!(metadata.metadata.budget, budget);

    let payloads: ConstructionPayloadsResponse = rosetta_client
        .call(
            RosettaEndpoint::Payloads,
            &ConstructionPayloadsRequest {
                network_identifier: network_identifier.clone(),
                operations: ops.clone(),
                metadata: Some(metadata.metadata),
                public_keys: vec![],
            },
        )
        .await;
    println!("Payload : {payloads:?}");

    // Combine
    let signing_payload = payloads.payloads.first().unwrap();
    let bytes = Hex::decode(&signing_payload.hex_bytes).unwrap();
    let signer = signing_payload.account_identifier.address;
    let signature = keystore.sign_hashed(&signer, &bytes).unwrap();
    let public_key = keystore.get_key(&signer).unwrap().public();

    let combine: ConstructionCombineResponse = rosetta_client
        .call(
            RosettaEndpoint::Combine,
            &ConstructionCombineRequest {
                network_identifier: network_identifier.clone(),
                unsigned_transaction: payloads.unsigned_transaction,
                signatures: vec![Signature {
                    signing_payload: signing_payload.clone(),
                    public_key: public_key.into(),
                    signature_type: SignatureType::Ed25519,
                    hex_bytes: Hex::from_bytes(SuiSignature::signature_bytes(&signature)),
                }],
            },
        )
        .await;
    println!("Combine : {combine:?}");

    // Submit
    let submit: TransactionIdentifierResponseResult = rosetta_client
        .call(
            RosettaEndpoint::Submit,
            &ConstructionSubmitRequest {
                network_identifier,
                signed_transaction: combine.signed_transaction,
            },
        )
        .await;
    println!("Submit : {submit:?}");
    submit
}

#[tokio::test]
async fn test_pay_with_gas_budget() {
    const TX_BUDGET_PASS: u64 = 5_000_000;
    let submit = pay_with_gas_budget(TX_BUDGET_PASS).await;
    match submit {
        TransactionIdentifierResponseResult::Success(_) => {}
        _ => panic!("Expected transaction to succeed"),
    }
}

#[tokio::test]
async fn test_pay_with_gas_budget_fail() {
    const TX_BUDGET_FAIL: u64 = 1_100_000;
    let submit = pay_with_gas_budget(TX_BUDGET_FAIL).await;
    match submit {
        TransactionIdentifierResponseResult::Error(rosetta_submit_gas_error) => {
            assert_eq!(
                rosetta_submit_gas_error,
                RosettaSubmitGasError {
                    code: 11,
                    message: "Transaction dry run error".to_string(),
                    description: None,
                    retriable: false,
                    details: RosettaSubmitGasErrorDetails {
                        error: "InsufficientGas".to_string()
                    }
                }
            )
        }
        _ => panic!("Expected transaction to fail"),
    }
}
