// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::encoding::{Base64, Encoding};
use rand::rngs::StdRng;
use rand::SeedableRng;
use serde_json::json;
use simulacrum::Simulacrum;
use std::sync::Arc;
use std::time::Duration;
use sui_graphql_rpc::client::simple_client::GraphqlQueryVariable;
use sui_graphql_rpc::client::ClientError;
use sui_graphql_rpc::config::Limits;
use sui_graphql_rpc::config::ServiceConfig;
use sui_graphql_rpc::test_infra::cluster::prep_executor_cluster;
use sui_graphql_rpc::test_infra::cluster::start_cluster;
use sui_types::digests::ChainIdentifier;
use sui_types::gas_coin::GAS;
use sui_types::transaction::CallArg;
use sui_types::transaction::ObjectArg;
use sui_types::transaction::TransactionDataAPI;
use sui_types::DEEPBOOK_ADDRESS;
use sui_types::SUI_FRAMEWORK_ADDRESS;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use tempfile::tempdir;
use tokio::time::sleep;

#[tokio::test]
async fn test_simple_client_validator_cluster() {
    telemetry_subscribers::init_for_testing();

    let cluster = start_cluster(ServiceConfig::test_defaults()).await;

    cluster
        .wait_for_checkpoint_catchup(1, Duration::from_secs(30))
        .await;

    let query = r#"
            {
                chainIdentifier
            }
        "#;
    let res = cluster
        .graphql_client
        .execute(query.to_string(), vec![])
        .await
        .unwrap();
    let chain_id_actual = cluster
        .network
        .validator_fullnode_handle
        .fullnode_handle
        .sui_client
        .read_api()
        .get_chain_identifier()
        .await
        .unwrap();

    let exp = format!(
        "{{\"data\":{{\"chainIdentifier\":\"{}\"}}}}",
        chain_id_actual
    );
    assert_eq!(&format!("{}", res), &exp);
}

#[tokio::test]
async fn test_simple_client_simulator_cluster() {
    let rng = StdRng::from_seed([12; 32]);
    let mut sim = Simulacrum::new_with_rng(rng);
    let data_ingestion_path = tempdir().unwrap();
    sim.set_data_ingestion_path(data_ingestion_path.path().to_path_buf());

    sim.create_checkpoint();
    sim.create_checkpoint();

    let genesis_checkpoint_digest1 = *sim
        .store()
        .get_checkpoint_by_sequence_number(0)
        .unwrap()
        .digest();

    let chain_id_actual = format!("{}", ChainIdentifier::from(genesis_checkpoint_digest1));
    let exp = format!(
        "{{\"data\":{{\"chainIdentifier\":\"{}\"}}}}",
        chain_id_actual
    );
    let cluster = sui_graphql_rpc::test_infra::cluster::serve_executor(
        Arc::new(sim),
        None,
        None,
        data_ingestion_path.path().to_path_buf(),
    )
    .await;
    cluster
        .wait_for_checkpoint_catchup(1, Duration::from_secs(30))
        .await;

    let query = r#"
            {
                chainIdentifier
            }
        "#;
    let res = cluster
        .graphql_client
        .execute(query.to_string(), vec![])
        .await
        .unwrap();

    assert_eq!(&format!("{}", res), &exp);
}

#[tokio::test]
async fn test_graphql_client_response() {
    let cluster = prep_executor_cluster().await;

    let query = r#"
            {
                chainIdentifier
            }
        "#;
    let res = cluster
        .graphql_client
        .execute_to_graphql(query.to_string(), true, vec![], vec![])
        .await
        .unwrap();

    assert_eq!(res.http_status().as_u16(), 200);
    assert_eq!(res.http_version(), reqwest::Version::HTTP_11);
    assert!(res.graphql_version().unwrap().len() >= 5);
    assert!(res.errors().is_empty());

    let usage = res.usage().unwrap().unwrap();
    assert_eq!(*usage.get("inputNodes").unwrap(), 1);
    assert_eq!(*usage.get("outputNodes").unwrap(), 1);
    assert_eq!(*usage.get("depth").unwrap(), 1);
    assert_eq!(*usage.get("variables").unwrap(), 0);
    assert_eq!(*usage.get("fragments").unwrap(), 0);
}

#[tokio::test]
async fn test_graphql_client_variables() {
    let cluster = prep_executor_cluster().await;

    let query = r#"{obj1: object(address: $framework_addr) {address}
            obj2: object(address: $deepbook_addr) {address}}"#;
    let variables = vec![
        GraphqlQueryVariable {
            name: "framework_addr".to_string(),
            ty: "SuiAddress!".to_string(),
            value: json!("0x2"),
        },
        GraphqlQueryVariable {
            name: "deepbook_addr".to_string(),
            ty: "SuiAddress!".to_string(),
            value: json!("0xdee9"),
        },
    ];
    let res = cluster
        .graphql_client
        .execute_to_graphql(query.to_string(), true, variables, vec![])
        .await
        .unwrap();

    assert!(res.errors().is_empty());
    let data = res.response_body().data.clone().into_json().unwrap();
    data.get("obj1").unwrap().get("address").unwrap();
    assert_eq!(
        data.get("obj1")
            .unwrap()
            .get("address")
            .unwrap()
            .as_str()
            .unwrap(),
        SUI_FRAMEWORK_ADDRESS.to_canonical_string(true)
    );
    assert_eq!(
        data.get("obj2")
            .unwrap()
            .get("address")
            .unwrap()
            .as_str()
            .unwrap(),
        DEEPBOOK_ADDRESS.to_canonical_string(true)
    );

    let bad_variables = vec![
        GraphqlQueryVariable {
            name: "framework_addr".to_string(),
            ty: "SuiAddress!".to_string(),
            value: json!("0x2"),
        },
        GraphqlQueryVariable {
            name: "deepbook_addr".to_string(),
            ty: "SuiAddress!".to_string(),
            value: json!("0xdee9"),
        },
        GraphqlQueryVariable {
            name: "deepbook_addr".to_string(),
            ty: "SuiAddress!".to_string(),
            value: json!("0xdee96666666"),
        },
    ];
    let res = cluster
        .graphql_client
        .execute_to_graphql(query.to_string(), true, bad_variables, vec![])
        .await;

    assert!(res.is_err());

    let bad_variables = vec![
        GraphqlQueryVariable {
            name: "framework_addr".to_string(),
            ty: "SuiAddress!".to_string(),
            value: json!("0x2"),
        },
        GraphqlQueryVariable {
            name: "deepbook_addr".to_string(),
            ty: "SuiAddress!".to_string(),
            value: json!("0xdee9"),
        },
        GraphqlQueryVariable {
            name: "deepbook_addr".to_string(),
            ty: "SuiAddressP!".to_string(),
            value: json!("0xdee9"),
        },
    ];
    let res = cluster
        .graphql_client
        .execute_to_graphql(query.to_string(), true, bad_variables, vec![])
        .await;

    assert!(res.is_err());

    let bad_variables = vec![
        GraphqlQueryVariable {
            name: "framework addr".to_string(),
            ty: "SuiAddress!".to_string(),
            value: json!("0x2"),
        },
        GraphqlQueryVariable {
            name: " deepbook_addr".to_string(),
            ty: "SuiAddress!".to_string(),
            value: json!("0xdee9"),
        },
        GraphqlQueryVariable {
            name: "4deepbook_addr".to_string(),
            ty: "SuiAddressP!".to_string(),
            value: json!("0xdee9"),
        },
        GraphqlQueryVariable {
            name: "".to_string(),
            ty: "SuiAddress!".to_string(),
            value: json!("0xdee9"),
        },
        GraphqlQueryVariable {
            name: " ".to_string(),
            ty: "SuiAddress!".to_string(),
            value: json!("0xdee9"),
        },
    ];

    for var in bad_variables {
        let res = cluster
            .graphql_client
            .execute_to_graphql(query.to_string(), true, vec![var.clone()], vec![])
            .await;

        assert!(res.is_err());
        assert!(
            res.unwrap_err().to_string()
                == ClientError::InvalidVariableName {
                    var_name: var.name.clone()
                }
                .to_string()
        );
    }
}

#[tokio::test]
async fn test_transaction_execution() {
    let cluster = start_cluster(ServiceConfig::test_defaults()).await;

    let addresses = cluster
        .network
        .validator_fullnode_handle
        .wallet
        .get_addresses();

    let sender = addresses[0];
    let recipient = addresses[1];
    let tx = cluster
        .network
        .validator_fullnode_handle
        .test_transaction_builder()
        .await
        .transfer_sui(Some(1_000), recipient)
        .build();
    let signed_tx = cluster
        .network
        .validator_fullnode_handle
        .wallet
        .sign_transaction(&tx);
    let original_digest = signed_tx.digest();
    let (tx_bytes, sigs) = signed_tx.to_tx_bytes_and_signatures();
    let tx_bytes = tx_bytes.encoded();
    let sigs = sigs.iter().map(|sig| sig.encoded()).collect::<Vec<_>>();

    let mutation = r#"{ executeTransactionBlock(txBytes: $tx,  signatures: $sigs) { effects { transactionBlock { digest } } errors}}"#;

    let variables = vec![
        GraphqlQueryVariable {
            name: "tx".to_string(),
            ty: "String!".to_string(),
            value: json!(tx_bytes),
        },
        GraphqlQueryVariable {
            name: "sigs".to_string(),
            ty: "[String!]!".to_string(),
            value: json!(sigs),
        },
    ];
    let res = cluster
        .graphql_client
        .execute_mutation_to_graphql(mutation.to_string(), variables)
        .await
        .unwrap();
    let binding = res.response_body().data.clone().into_json().unwrap();
    let res = binding.get("executeTransactionBlock").unwrap();

    let digest = res
        .get("effects")
        .unwrap()
        .get("transactionBlock")
        .unwrap()
        .get("digest")
        .unwrap()
        .as_str()
        .unwrap();
    assert!(res.get("errors").unwrap().is_null());
    assert_eq!(digest, original_digest.to_string());

    // Wait for the transaction to be committed and indexed
    sleep(Duration::from_secs(10)).await;
    // Query the transaction
    let query = r#"
            {
                transactionBlock(digest: $dig){
                    sender {
                        address
                    }
                }
            }
        "#;

    let variables = vec![GraphqlQueryVariable {
        name: "dig".to_string(),
        ty: "String!".to_string(),
        value: json!(digest),
    }];
    let res = cluster
        .graphql_client
        .execute_to_graphql(query.to_string(), true, variables, vec![])
        .await
        .unwrap();

    let binding = res.response_body().data.clone().into_json().unwrap();
    let sender_read = binding
        .get("transactionBlock")
        .unwrap()
        .get("sender")
        .unwrap()
        .get("address")
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(sender_read, sender.to_string());
}

#[tokio::test]
async fn test_zklogin_sig_verify() {
    use shared_crypto::intent::Intent;
    use shared_crypto::intent::IntentMessage;
    use sui_test_transaction_builder::TestTransactionBuilder;
    use sui_types::base_types::SuiAddress;
    use sui_types::crypto::Signature;
    use sui_types::signature::GenericSignature;
    use sui_types::utils::load_test_vectors;
    use sui_types::zk_login_authenticator::ZkLoginAuthenticator;

    telemetry_subscribers::init_for_testing();

    let cluster = start_cluster(ServiceConfig::test_defaults()).await;

    let test_cluster = &cluster.network.validator_fullnode_handle;
    test_cluster.trigger_reconfiguration().await;
    test_cluster.wait_for_epoch_all_nodes(1).await;
    test_cluster.wait_for_authenticator_state_update().await;

    // Construct a valid zkLogin transaction data, signature.
    let (kp, pk_zklogin, inputs) =
        &load_test_vectors("../sui-types/src/unit_tests/zklogin_test_vectors.json")[1];

    let zklogin_addr = (pk_zklogin).into();
    let rgp = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), zklogin_addr)
        .await;
    let tx_data = TestTransactionBuilder::new(zklogin_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
    let eph_sig = Signature::new_secure(&msg, kp);
    let generic_sig = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
        inputs.clone(),
        2,
        eph_sig.clone(),
    ));

    // construct all parameters for the query
    let bytes = Base64::encode(bcs::to_bytes(&tx_data).unwrap());
    let signature = Base64::encode(generic_sig.as_ref());
    let intent_scope = "TRANSACTION_DATA";
    let author = zklogin_addr.to_string();

    // now query the endpoint with a valid tx data bytes and a valid signature with the correct proof for dev env.
    let query = r#"{ verifyZkloginSignature(bytes: $bytes, signature: $signature, intentScope: $intent_scope, author: $author ) { success, errors}}"#;
    let variables = vec![
        GraphqlQueryVariable {
            name: "bytes".to_string(),
            ty: "String!".to_string(),
            value: json!(bytes),
        },
        GraphqlQueryVariable {
            name: "signature".to_string(),
            ty: "String!".to_string(),
            value: json!(signature),
        },
        GraphqlQueryVariable {
            name: "intent_scope".to_string(),
            ty: "ZkLoginIntentScope!".to_string(),
            value: json!(intent_scope),
        },
        GraphqlQueryVariable {
            name: "author".to_string(),
            ty: "SuiAddress!".to_string(),
            value: json!(author),
        },
    ];
    let res = cluster
        .graphql_client
        .execute_to_graphql(query.to_string(), true, variables, vec![])
        .await
        .unwrap();

    // a valid signature with tx bytes returns success as true.
    let binding = res.response_body().data.clone().into_json().unwrap();
    let res = binding.get("verifyZkloginSignature").unwrap();
    assert_eq!(res.get("success").unwrap(), true);

    // set up an invalid intent scope.
    let incorrect_intent_scope = "PERSONAL_MESSAGE";
    let incorrect_variables = vec![
        GraphqlQueryVariable {
            name: "bytes".to_string(),
            ty: "String!".to_string(),
            value: json!(bytes),
        },
        GraphqlQueryVariable {
            name: "signature".to_string(),
            ty: "String!".to_string(),
            value: json!(signature),
        },
        GraphqlQueryVariable {
            name: "intent_scope".to_string(),
            ty: "ZkLoginIntentScope!".to_string(),
            value: json!(incorrect_intent_scope),
        },
        GraphqlQueryVariable {
            name: "author".to_string(),
            ty: "SuiAddress!".to_string(),
            value: json!(author),
        },
    ];
    //  returns a non-empty errors list in response
    let res = cluster
        .graphql_client
        .execute_to_graphql(query.to_string(), true, incorrect_variables, vec![])
        .await
        .unwrap();
    let binding = res.response_body().data.clone().into_json().unwrap();
    let res = binding.get("verifyZkloginSignature").unwrap();
    assert_eq!(res.get("success").unwrap(), false);
}

// TODO: add more test cases for transaction execution/dry run in transactional test runner.
#[tokio::test]
async fn test_transaction_dry_run() {
    telemetry_subscribers::init_for_testing();

    let cluster = start_cluster(ServiceConfig::test_defaults()).await;

    let addresses = cluster
        .network
        .validator_fullnode_handle
        .wallet
        .get_addresses();

    let sender = addresses[0];
    let recipient = addresses[1];
    let tx = cluster
        .network
        .validator_fullnode_handle
        .test_transaction_builder()
        .await
        .transfer_sui(Some(1_000), recipient)
        .build();
    let tx_bytes = Base64::encode(bcs::to_bytes(&tx).unwrap());

    let query = r#"{ dryRunTransactionBlock(txBytes: $tx) {
                transaction {
                    digest
                    sender {
                        address
                    }
                    gasInput {
                        gasSponsor {
                            address
                        }
                        gasPrice
                    }
                }
                error
                results {
                    mutatedReferences {
                        input {
                            __typename
                            ... on Input {
                                ix
                            }
                            ... on Result {
                                cmd
                                ix
                            }
                        }
                        type {
                            repr
                        }
                    }
                    returnValues {
                        type {
                            repr
                        }
                        bcs
                    }
                }
            }
        }"#;
    let variables = vec![GraphqlQueryVariable {
        name: "tx".to_string(),
        ty: "String!".to_string(),
        value: json!(tx_bytes),
    }];
    let res = cluster
        .graphql_client
        .execute_to_graphql(query.to_string(), true, variables, vec![])
        .await
        .unwrap();
    let binding = res.response_body().data.clone().into_json().unwrap();
    let res = binding.get("dryRunTransactionBlock").unwrap();

    let digest = res.get("transaction").unwrap().get("digest").unwrap();
    // Dry run txn does not have digest
    assert!(digest.is_null());
    assert!(res.get("error").unwrap().is_null());
    let sender_read = res
        .get("transaction")
        .unwrap()
        .get("sender")
        .unwrap()
        .get("address")
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(sender_read, sender.to_string());
    assert!(res.get("results").unwrap().is_array());
}

// Test dry run where the transaction kind is provided instead of the full transaction.
#[tokio::test]
async fn test_transaction_dry_run_with_kind() {
    telemetry_subscribers::init_for_testing();

    let cluster = start_cluster(ServiceConfig::test_defaults()).await;

    let addresses = cluster
        .network
        .validator_fullnode_handle
        .wallet
        .get_addresses();

    let recipient = addresses[1];
    let tx = cluster
        .network
        .validator_fullnode_handle
        .test_transaction_builder()
        .await
        .transfer_sui(Some(1_000), recipient)
        .build();
    let tx_kind_bytes = Base64::encode(bcs::to_bytes(&tx.into_kind()).unwrap());

    let query = r#"{ dryRunTransactionBlock(txBytes: $tx, txMeta: {}) {
                results {
                    mutatedReferences {
                        input {
                            __typename
                        }
                    }
                }
                transaction {
                    digest
                    sender {
                        address
                    }
                    gasInput {
                        gasSponsor {
                            address
                        }
                        gasPrice
                    }
                }
                error
            }
        }"#;
    let variables = vec![GraphqlQueryVariable {
        name: "tx".to_string(),
        ty: "String!".to_string(),
        value: json!(tx_kind_bytes),
    }];
    let res = cluster
        .graphql_client
        .execute_to_graphql(query.to_string(), true, variables, vec![])
        .await
        .unwrap();
    let binding = res.response_body().data.clone().into_json().unwrap();
    let res = binding.get("dryRunTransactionBlock").unwrap();

    let digest = res.get("transaction").unwrap().get("digest").unwrap();
    // Dry run txn does not have digest
    assert!(digest.is_null());
    assert!(res.get("error").unwrap().is_null());
    let sender_read = res.get("transaction").unwrap().get("sender").unwrap();
    // Since no transaction metadata is provided, we use 0x0 as the sender while dry running the trasanction
    // in which case the sender is null.
    assert!(sender_read.is_null());
    assert!(res.get("results").unwrap().is_array());
}

// Test that we can handle dry run with failures at execution stage too.
#[tokio::test]
async fn test_dry_run_failed_execution() {
    telemetry_subscribers::init_for_testing();

    let cluster = start_cluster(ServiceConfig::test_defaults()).await;

    let addresses = cluster
        .network
        .validator_fullnode_handle
        .wallet
        .get_addresses();

    let sender = addresses[0];
    let coin = *cluster
        .network
        .validator_fullnode_handle
        .wallet
        .get_gas_objects_owned_by_address(sender, None)
        .await
        .unwrap()
        .get(1)
        .unwrap();
    let tx = cluster
        .network
        .validator_fullnode_handle
        .test_transaction_builder()
        .await
        // A split coin that goes nowhere -> execution failure
        .move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            "coin",
            "split",
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(coin)),
                CallArg::Pure(bcs::to_bytes(&1000u64).unwrap()),
            ],
        )
        .with_type_args(vec![GAS::type_tag()])
        .build();
    let tx_bytes = Base64::encode(bcs::to_bytes(&tx).unwrap());

    let query = r#"{ dryRunTransactionBlock(txBytes: $tx) {
                results {
                    mutatedReferences {
                        input {
                            __typename
                        }
                    }
                }
                transaction {
                    digest
                    sender {
                        address
                    }
                    gasInput {
                        gasSponsor {
                            address
                        }
                        gasPrice
                    }
                }
                error
            }
        }"#;
    let variables = vec![GraphqlQueryVariable {
        name: "tx".to_string(),
        ty: "String!".to_string(),
        value: json!(tx_bytes),
    }];
    let res = cluster
        .graphql_client
        .execute_to_graphql(query.to_string(), true, variables, vec![])
        .await
        .unwrap();
    let binding = res.response_body().data.clone().into_json().unwrap();
    let res = binding.get("dryRunTransactionBlock").unwrap();

    // Execution failed so the results are null.
    assert!(res.get("results").unwrap().is_null());
    // Check that the error is not null and contains the error message.
    assert!(res
        .get("error")
        .unwrap()
        .as_str()
        .unwrap()
        .contains("UnusedValueWithoutDrop"));
}

#[tokio::test]
async fn test_epoch_live_object_set_digest() {
    telemetry_subscribers::init_for_testing();

    let cluster = start_cluster(ServiceConfig::test_defaults()).await;

    cluster
        .network
        .validator_fullnode_handle
        .trigger_reconfiguration()
        .await;

    // Wait for the epoch to be indexed
    cluster
        .wait_for_epoch_catchup(0, Duration::from_secs(30))
        .await;

    // Query the epoch
    let query = "
            {
                epoch(id: 0){
                    liveObjectSetDigest
                }
            }
        ";

    let res = cluster
        .graphql_client
        .execute_to_graphql(query.to_string(), true, vec![], vec![])
        .await
        .unwrap();

    let binding = res.response_body().data.clone().into_json().unwrap();

    // Check that liveObjectSetDigest is not null
    assert!(!binding
        .get("epoch")
        .unwrap()
        .get("liveObjectSetDigest")
        .unwrap()
        .is_null());
}

#[tokio::test]
async fn test_payload_using_vars_mutation_passes() {
    telemetry_subscribers::init_for_testing();
    let cluster = sui_graphql_rpc::test_infra::cluster::start_cluster(ServiceConfig {
        limits: Limits {
            max_query_payload_size: 5000,
            max_tx_payload_size: 6000,
            ..Default::default()
        },
        ..ServiceConfig::test_defaults()
    })
    .await;
    let addresses = cluster
        .network
        .validator_fullnode_handle
        .wallet
        .get_addresses();

    let recipient = addresses[1];
    let tx = cluster
        .network
        .validator_fullnode_handle
        .test_transaction_builder()
        .await
        .transfer_sui(Some(1_000), recipient)
        .build();
    let signed_tx = cluster
        .network
        .validator_fullnode_handle
        .wallet
        .sign_transaction(&tx);
    let (tx_bytes, sigs) = signed_tx.to_tx_bytes_and_signatures();
    let tx_bytes = tx_bytes.encoded();
    let sigs = sigs.iter().map(|sig| sig.encoded()).collect::<Vec<_>>();

    let mutation = r#"{
            executeTransactionBlock(txBytes: $tx,  signatures: $sigs) {
                effects {
                    transactionBlock { digest }
                    status
                }
                errors
            }
        }"#;

    let variables = vec![
        GraphqlQueryVariable {
            name: "tx".to_string(),
            ty: "String!".to_string(),
            value: json!(tx_bytes),
        },
        GraphqlQueryVariable {
            name: "sigs".to_string(),
            ty: "[String!]!".to_string(),
            value: json!(sigs),
        },
    ];

    let res = cluster
        .graphql_client
        .execute_mutation_to_graphql(mutation.to_string(), variables)
        .await
        .unwrap();

    assert!(res.errors().is_empty(), "{:#?}", res.errors());
}
