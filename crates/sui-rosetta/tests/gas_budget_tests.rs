// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::encoding::{Encoding, Hex};
use serde_json::json;

use rosetta_client::{FlowResponses, RosettaError, start_rosetta_test_server};
use sui_keys::keystore::AccountKeystore;
use sui_rosetta::operations::Operations;
use sui_rosetta::types::{
    ConstructionCombineRequest, ConstructionCombineResponse, ConstructionMetadataRequest,
    ConstructionMetadataResponse, ConstructionPayloadsRequest, ConstructionPayloadsResponse,
    ConstructionPreprocessRequest, ConstructionPreprocessResponse, ConstructionSubmitRequest,
    NetworkIdentifier, PreprocessMetadata, Signature, SignatureType, SuiEnv,
    TransactionIdentifierResponse,
};
use sui_rpc::client::Client as GrpcClient;
use sui_types::crypto::SuiSignature;
use test_cluster::TestClusterBuilder;

use crate::rosetta_client::RosettaEndpoint;

#[allow(dead_code)]
mod rosetta_client;

async fn pay_with_gas_budget(budget: u64) -> anyhow::Result<FlowResponses> {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;

    let client = GrpcClient::new(test_cluster.rpc_url())?;
    let (rosetta_client, _handle) = start_rosetta_test_server(client).await;

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
                "currency": { "symbol": "SUI", "decimals": 9,
                }
            },
        }]
    ))?;

    let mut flow_responses = FlowResponses::default();

    // We manually use rosetta-flow here to check the intermediate results.
    let metadata = Some(PreprocessMetadata {
        budget: Some(budget),
    });

    let preprocess_result: Result<ConstructionPreprocessResponse, RosettaError> = rosetta_client
        .call(
            RosettaEndpoint::Preprocess,
            &ConstructionPreprocessRequest {
                network_identifier: network_identifier.clone(),
                operations: ops.clone(),
                metadata,
            },
        )
        .await;

    let preprocess_options = match preprocess_result {
        Ok(resp) => {
            assert_eq!(
                resp.options
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Missing options"))?
                    .budget
                    .ok_or_else(|| anyhow::anyhow!("Missing budget"))?,
                budget
            );
            let options = resp.options.clone();
            flow_responses.preprocess = Some(Ok(resp));
            options
        }
        Err(e) => {
            flow_responses.preprocess = Some(Err(e));
            return Ok(flow_responses);
        }
    };

    let metadata_result: Result<ConstructionMetadataResponse, RosettaError> = rosetta_client
        .call(
            RosettaEndpoint::Metadata,
            &ConstructionMetadataRequest {
                network_identifier: network_identifier.clone(),
                options: preprocess_options,
                public_keys: vec![],
            },
        )
        .await;

    let construction_metadata = match metadata_result {
        Ok(resp) => {
            assert_eq!(resp.metadata.budget, budget);
            let metadata = resp.metadata.clone();
            flow_responses.metadata = Some(Ok(resp));
            metadata
        }
        Err(e) => {
            flow_responses.metadata = Some(Err(e));
            return Ok(flow_responses);
        }
    };

    let payloads_result: Result<ConstructionPayloadsResponse, RosettaError> = rosetta_client
        .call(
            RosettaEndpoint::Payloads,
            &ConstructionPayloadsRequest {
                network_identifier: network_identifier.clone(),
                operations: ops.clone(),
                metadata: Some(construction_metadata),
                public_keys: vec![],
            },
        )
        .await;

    let (unsigned_transaction, signing_payload) = match payloads_result {
        Ok(resp) => {
            let signing_payload = resp
                .payloads
                .first()
                .ok_or_else(|| anyhow::anyhow!("No payloads"))?
                .clone();
            let unsigned_transaction = resp.unsigned_transaction.clone();
            flow_responses.payloads = Some(Ok(resp));
            (unsigned_transaction, signing_payload)
        }
        Err(e) => {
            flow_responses.payloads = Some(Err(e));
            return Ok(flow_responses);
        }
    };

    let bytes = Hex::decode(&signing_payload.hex_bytes)?;
    let signer = signing_payload.account_identifier.address;
    let signature = keystore.sign_hashed(&signer, &bytes).await?;
    let public_key = keystore.export(&signer)?.public();

    let combine_result: Result<ConstructionCombineResponse, RosettaError> = rosetta_client
        .call(
            RosettaEndpoint::Combine,
            &ConstructionCombineRequest {
                network_identifier: network_identifier.clone(),
                unsigned_transaction,
                signatures: vec![Signature {
                    signing_payload: signing_payload.clone(),
                    public_key: public_key.into(),
                    signature_type: SignatureType::Ed25519,
                    hex_bytes: Hex::from_bytes(SuiSignature::signature_bytes(&signature)),
                }],
            },
        )
        .await;

    let signed_transaction = match combine_result {
        Ok(resp) => {
            let signed_transaction = resp.signed_transaction.clone();
            flow_responses.combine = Some(Ok(resp));
            signed_transaction
        }
        Err(e) => {
            flow_responses.combine = Some(Err(e));
            return Ok(flow_responses);
        }
    };

    let submit_result: Result<TransactionIdentifierResponse, RosettaError> = rosetta_client
        .call(
            RosettaEndpoint::Submit,
            &ConstructionSubmitRequest {
                network_identifier,
                signed_transaction,
            },
        )
        .await;

    match submit_result {
        Ok(resp) => {
            flow_responses.submit = Some(Ok(resp));
        }
        Err(e) => {
            flow_responses.submit = Some(Err(e));
        }
    };

    Ok(flow_responses)
}

#[tokio::test]
async fn test_pay_with_gas_budget() {
    const TX_BUDGET_PASS: u64 = 5_000_000;
    let flow_responses = pay_with_gas_budget(TX_BUDGET_PASS)
        .await
        .expect("Should not error during test setup");

    assert!(
        flow_responses.preprocess.as_ref().unwrap().is_ok(),
        "Preprocess should succeed"
    );
    assert!(
        flow_responses.metadata.as_ref().unwrap().is_ok(),
        "Metadata should succeed"
    );
    assert!(
        flow_responses.payloads.as_ref().unwrap().is_ok(),
        "Payloads should succeed"
    );
    assert!(
        flow_responses.combine.as_ref().unwrap().is_ok(),
        "Combine should succeed"
    );
    assert!(
        flow_responses.submit.as_ref().unwrap().is_ok(),
        "Submit should succeed"
    );
}

#[tokio::test]
async fn test_pay_with_gas_budget_fail() {
    const TX_BUDGET_FAIL: u64 = 1_100_000;
    let flow_responses = pay_with_gas_budget(TX_BUDGET_FAIL)
        .await
        .expect("Should not error during test setup");

    assert!(
        flow_responses.preprocess.as_ref().unwrap().is_ok(),
        "Preprocess should succeed"
    );

    assert!(
        flow_responses.metadata.is_some(),
        "Metadata should be attempted"
    );

    match &flow_responses.metadata.as_ref().unwrap() {
        Err(rosetta_error) => {
            assert_eq!(
                rosetta_error.code, 11,
                "Expected error code 11 for dry run error"
            );
            assert_eq!(rosetta_error.message, "Transaction dry run error");
            assert!(!rosetta_error.retriable);

            if let Some(details) = &rosetta_error.details {
                let details_str = serde_json::to_string(details).unwrap();
                assert!(
                    details_str.contains("INSUFFICIENT_GAS"),
                    "Expected InsufficientGas in error details, got: {}",
                    details_str
                );
            } else {
                panic!("Expected error details to be present");
            }
        }
        Ok(_) => panic!("Expected metadata to fail due to insufficient gas budget"),
    }

    assert!(
        flow_responses.payloads.is_none(),
        "Payloads should not be attempted after metadata failure"
    );
    assert!(
        flow_responses.combine.is_none(),
        "Combine should not be attempted after metadata failure"
    );
    assert!(
        flow_responses.submit.is_none(),
        "Submit should not be attempted after metadata failure"
    );
}
