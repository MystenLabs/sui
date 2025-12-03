// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::{Context as _, bail};
use fastcrypto::encoding::{Base64, Encoding};
use insta::assert_debug_snapshot;
use prometheus::Registry;
use serde::Deserialize;
use serde_json::json;
use shared_crypto::intent::{Intent, IntentMessage, PersonalMessage};
use sui_indexer_alt_e2e_tests::{OffchainCluster, OffchainClusterConfig};
use sui_indexer_alt_framework::ingestion::{ClientArgs, ingestion_client::IngestionClientArgs};
use sui_indexer_alt_graphql::config::{RpcConfig as GraphQlConfig, ZkLoginConfig, ZkLoginEnv};
use sui_swarm_config::genesis_config::AccountConfig;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::{
    base_types::SuiAddress, crypto::Signature, signature::GenericSignature,
    utils::load_test_vectors, zk_login_authenticator::ZkLoginAuthenticator,
};
use tempfile::TempDir;
use test_cluster::{TestCluster, TestClusterBuilder};
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

const QUERY: &str = r#"
query ($bytes: Base64!, $signature: Base64!, $scope: ZkLoginIntentScope!, $author: SuiAddress!) {
    verifyZkLoginSignature(bytes: $bytes, signature: $signature, intentScope: $scope, author: $author) {
        success
        error
    }
}
"#;

struct FullCluster {
    /// Validator and fullnodes
    onchain: TestCluster,

    /// Indexers and RPCs
    offchain: OffchainCluster,

    #[allow(unused)]
    temp_dir: TempDir,
}

#[derive(Deserialize, Eq, PartialEq, Debug)]
struct ZkLoginResult {
    success: bool,
    error: Option<String>,
}

impl FullCluster {
    async fn new() -> anyhow::Result<Self> {
        let temp_dir = TempDir::new()?;
        let ingestion_dir = temp_dir.path().to_path_buf();

        let onchain = TestClusterBuilder::new()
            .with_num_validators(1)
            .with_data_ingestion_dir(ingestion_dir.clone())
            .with_epoch_duration_ms(300_000) // 5 minutes
            .with_accounts(vec![
                AccountConfig {
                    address: None,
                    gas_amounts: vec![1_000_000_000_000; 2],
                };
                4
            ])
            .build()
            .await;

        let offchain = OffchainCluster::new(
            ClientArgs {
                ingestion: IngestionClientArgs {
                    local_ingestion_path: Some(ingestion_dir),
                    ..Default::default()
                },
                ..Default::default()
            },
            OffchainClusterConfig {
                graphql_config: GraphQlConfig {
                    zklogin: ZkLoginConfig {
                        env: ZkLoginEnv::Test,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
            &Registry::new(),
            CancellationToken::new(),
        )
        .await?;

        // Trigger an epoch change and wait until GraphQL sees Epoch 1
        onchain.trigger_reconfiguration().await;
        onchain.wait_for_authenticator_state_update().await;
        tokio::time::timeout(Duration::from_secs(5), async {
            let mut interval = interval(Duration::from_millis(200));
            loop {
                interval.tick().await;
                if matches!(offchain.latest_graphql_epoch().await, Ok(1)) {
                    break;
                }
            }
        })
        .await
        .unwrap();

        Ok(Self {
            onchain,
            offchain,
            temp_dir,
        })
    }

    async fn verify_zklogin(
        &self,
        bytes: Vec<u8>,
        signature: Vec<u8>,
        scope: &str,
        author: SuiAddress,
    ) -> anyhow::Result<ZkLoginResult> {
        let client = reqwest::Client::new();
        let url = self.offchain.graphql_url();

        let variables = json!({
            "bytes": Base64::encode(bytes),
            "signature": Base64::encode(signature),
            "scope": scope,
            "author": author.to_string(),
        });

        let response: serde_json::Value = client
            .post(url.as_str())
            .json(&json!({
                "query": QUERY,
                "variables": variables
            }))
            .send()
            .await?
            .json()
            .await?;

        if let Some(errors) = response.get("errors").and_then(|es| es.as_array()) {
            let errors: Vec<_> = errors
                .iter()
                .map(|e| e.get("message").unwrap().as_str().unwrap().to_owned())
                .collect();

            bail!(serde_json::to_string(&errors).unwrap());
        }

        let result = response
            .pointer("/data/verifyZkLoginSignature")
            .with_context(|| format!("missing data.verifyZkLoginSignature in {response:#?}"))?;

        serde_json::from_value(result.clone()).context("failed to deserialize result")
    }
}

#[tokio::test]
async fn test_verify_transaction() {
    telemetry_subscribers::init_for_testing();
    let cluster = FullCluster::new().await.unwrap();

    let (kp, pk, inputs) =
        &load_test_vectors("../sui-types/src/unit_tests/zklogin_test_vectors.json")[1];

    let addr = pk.into();
    let rgp = cluster.onchain.get_reference_gas_price().await;
    let gas = cluster
        .onchain
        .fund_address_and_return_gas(rgp, Some(1000000), addr)
        .await;

    let tx = TestTransactionBuilder::new(addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();

    let message = IntentMessage::new(Intent::sui_transaction(), tx.clone());
    let signature = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
        inputs.clone(),
        2,
        Signature::new_secure(&message, kp),
    ));

    let result = cluster
        .verify_zklogin(
            bcs::to_bytes(&tx).unwrap(),
            signature.as_ref().to_owned(),
            "TRANSACTION_DATA",
            addr,
        )
        .await
        .unwrap();

    assert_eq!(
        result,
        ZkLoginResult {
            success: true,
            error: None
        }
    );
}

#[tokio::test]
async fn test_verify_personal_message() {
    telemetry_subscribers::init_for_testing();
    let cluster = FullCluster::new().await.unwrap();

    let (kp, pk, inputs) =
        &load_test_vectors("../sui-types/src/unit_tests/zklogin_test_vectors.json")[1];

    let addr = pk.into();
    let personal = b"Hello, World!".to_vec();

    let message = IntentMessage::new(
        Intent::personal_message(),
        PersonalMessage {
            message: personal.clone(),
        },
    );

    let signature = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
        inputs.clone(),
        2,
        Signature::new_secure(&message, kp),
    ));

    let result = cluster
        .verify_zklogin(
            personal,
            signature.as_ref().to_owned(),
            "PERSONAL_MESSAGE",
            addr,
        )
        .await
        .unwrap();

    assert_eq!(
        result,
        ZkLoginResult {
            success: true,
            error: None
        }
    );
}

#[tokio::test]
async fn test_verify_invalid_scope() {
    telemetry_subscribers::init_for_testing();
    let cluster = FullCluster::new().await.unwrap();

    let (kp, pk, inputs) =
        &load_test_vectors("../sui-types/src/unit_tests/zklogin_test_vectors.json")[1];

    let addr = pk.into();
    let personal = b"Hello, World!".to_vec();

    let message = IntentMessage::new(
        Intent::personal_message(),
        PersonalMessage {
            message: personal.clone(),
        },
    );

    let signature = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
        inputs.clone(),
        2,
        Signature::new_secure(&message, kp),
    ));

    let result = cluster
        .verify_zklogin(
            personal,
            signature.as_ref().to_owned(),
            "TRANSACTION_DATA",
            addr,
        )
        .await
        .unwrap_err();

    assert_debug_snapshot!(result, @r###""[\"Failed to deserialize TransactionData from bytes\"]""###);
}

#[tokio::test]
async fn test_verify_invalid_transaction() {
    telemetry_subscribers::init_for_testing();
    let cluster = FullCluster::new().await.unwrap();

    let (kp, pk, inputs) =
        &load_test_vectors("../sui-types/src/unit_tests/zklogin_test_vectors.json")[1];

    let addr = pk.into();
    // Create bytes that are not valid TransactionData
    let invalid_tx_bytes = b"not a valid transaction".to_vec();

    // Create a signature for a real transaction (but we'll send invalid bytes)
    let rgp = cluster.onchain.get_reference_gas_price().await;
    let gas = cluster
        .onchain
        .fund_address_and_return_gas(rgp, Some(1000000), addr)
        .await;

    let tx = TestTransactionBuilder::new(addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();

    let message = IntentMessage::new(Intent::sui_transaction(), tx);
    let signature = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
        inputs.clone(),
        2,
        Signature::new_secure(&message, kp),
    ));

    let result = cluster
        .verify_zklogin(
            invalid_tx_bytes,
            signature.as_ref().to_owned(),
            "TRANSACTION_DATA",
            addr,
        )
        .await
        .unwrap_err();

    assert_debug_snapshot!(result, @r###""[\"Failed to deserialize TransactionData from bytes\"]""###);
}

#[tokio::test]
async fn test_verify_wrong_address() {
    telemetry_subscribers::init_for_testing();
    let cluster = FullCluster::new().await.unwrap();

    let (kp, pk, inputs) =
        &load_test_vectors("../sui-types/src/unit_tests/zklogin_test_vectors.json")[1];

    let _addr: SuiAddress = pk.into();
    let personal = b"Hello, World!".to_vec();

    let message = IntentMessage::new(
        Intent::personal_message(),
        PersonalMessage {
            message: personal.clone(),
        },
    );

    let signature = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
        inputs.clone(),
        2,
        Signature::new_secure(&message, kp),
    ));

    let result = cluster
        .verify_zklogin(
            personal,
            signature.as_ref().to_owned(),
            "PERSONAL_MESSAGE",
            SuiAddress::ZERO, // Wrong address
        )
        .await
        .unwrap();

    assert_eq!(
        result,
        ZkLoginResult {
            success: false,
            error: Some("Invalid address".to_string())
        }
    );
}

#[tokio::test]
async fn test_verify_invalid_signature() {
    telemetry_subscribers::init_for_testing();
    let cluster = FullCluster::new().await.unwrap();

    let (_, pk, _) = &load_test_vectors("../sui-types/src/unit_tests/zklogin_test_vectors.json")[1];

    let addr: SuiAddress = pk.into();
    let personal = b"Hello, World!".to_vec();

    // Create invalid signature bytes
    let invalid_signature = vec![0xFF; 100];

    let result = cluster
        .verify_zklogin(personal, invalid_signature, "PERSONAL_MESSAGE", addr)
        .await
        .unwrap_err();

    assert_debug_snapshot!(result, @r###""[\"Cannot parse signature\"]""###);
}

#[tokio::test]
async fn test_verify_not_zklogin_signature() {
    telemetry_subscribers::init_for_testing();
    let cluster = FullCluster::new().await.unwrap();

    // Create a regular Ed25519 keypair
    use fastcrypto::ed25519::Ed25519KeyPair;
    use fastcrypto::traits::KeyPair;
    use rand::SeedableRng;
    use sui_types::crypto::SuiKeyPair;

    let keypair = SuiKeyPair::Ed25519(Ed25519KeyPair::generate(
        &mut rand::rngs::StdRng::from_seed([1; 32]),
    ));
    let addr = SuiAddress::from(&keypair.public());

    let personal = b"Hello, World!".to_vec();

    let message = IntentMessage::new(
        Intent::personal_message(),
        PersonalMessage {
            message: personal.clone(),
        },
    );

    // Sign with regular Ed25519 signature
    let signature = GenericSignature::Signature(Signature::new_secure(&message, &keypair));

    let result = cluster
        .verify_zklogin(
            personal,
            signature.as_ref().to_owned(),
            "PERSONAL_MESSAGE",
            addr,
        )
        .await
        .unwrap_err();

    assert_debug_snapshot!(result, @r###""[\"Not a zkLogin signature\"]""###);
}
