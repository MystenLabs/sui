// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(feature = "pg_integration")]
mod tests {
    use fastcrypto::encoding::{Base64, Encoding};
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use serde_json::json;
    use serial_test::serial;
    use simulacrum::Simulacrum;
    use std::sync::Arc;
    use std::time::Duration;
    use sui_graphql_rpc::client::simple_client::GraphqlQueryVariable;
    use sui_graphql_rpc::client::ClientError;
    use sui_graphql_rpc::config::ConnectionConfig;
    use sui_graphql_rpc::test_infra::cluster::ExecutorCluster;
    use sui_graphql_rpc::test_infra::cluster::DEFAULT_INTERNAL_DATA_SOURCE_PORT;
    use sui_types::digests::ChainIdentifier;
    use sui_types::gas_coin::GAS;
    use sui_types::transaction::CallArg;
    use sui_types::transaction::ObjectArg;
    use sui_types::transaction::TransactionDataAPI;
    use sui_types::DEEPBOOK_ADDRESS;
    use sui_types::SUI_FRAMEWORK_ADDRESS;
    use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
    use tokio::time::sleep;

    async fn prep_cluster() -> (ConnectionConfig, ExecutorCluster) {
        let rng = StdRng::from_seed([12; 32]);
        let mut sim = Simulacrum::new_with_rng(rng);

        sim.create_checkpoint();
        sim.create_checkpoint();

        let connection_config = ConnectionConfig::default();
        let cluster = sui_graphql_rpc::test_infra::cluster::serve_executor(
            connection_config.clone(),
            DEFAULT_INTERNAL_DATA_SOURCE_PORT,
            Arc::new(sim),
            None,
        )
        .await;

        cluster
            .wait_for_checkpoint_catchup(1, Duration::from_secs(10))
            .await;

        (connection_config, cluster)
    }

    #[tokio::test]
    #[serial]
    async fn test_simple_client_validator_cluster() {
        let _guard = telemetry_subscribers::TelemetryConfig::new()
            .with_env()
            .init();

        let cluster =
            sui_graphql_rpc::test_infra::cluster::start_cluster(ConnectionConfig::default(), None)
                .await;

        // Wait for servers to start and catchup

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
    #[serial]
    async fn test_simple_client_simulator_cluster() {
        let rng = StdRng::from_seed([12; 32]);
        let mut sim = Simulacrum::new_with_rng(rng);

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
            ConnectionConfig::default(),
            DEFAULT_INTERNAL_DATA_SOURCE_PORT,
            Arc::new(sim),
            None,
        )
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
    #[serial]
    async fn test_graphql_client_response() {
        let (_, cluster) = prep_cluster().await;

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
        assert_eq!(res.http_version(), hyper::Version::HTTP_11);
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
    #[serial]
    async fn test_graphql_client_variables() {
        let (_, cluster) = prep_cluster().await;

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
    #[serial]
    async fn test_transaction_execution() {
        let _guard = telemetry_subscribers::TelemetryConfig::new()
            .with_env()
            .init();

        let cluster =
            sui_graphql_rpc::test_infra::cluster::start_cluster(ConnectionConfig::default(), None)
                .await;

        let addresses = cluster.validator_fullnode_handle.wallet.get_addresses();

        let sender = addresses[0];
        let recipient = addresses[1];
        let tx = cluster
            .validator_fullnode_handle
            .test_transaction_builder()
            .await
            .transfer_sui(Some(1_000), recipient)
            .build();
        let signed_tx = cluster
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
    #[serial]
    async fn test_zklogin_sig_verify() {
        let _guard = telemetry_subscribers::TelemetryConfig::new()
            .with_env()
            .init();

        let cluster =
            sui_graphql_rpc::test_infra::cluster::start_cluster(ConnectionConfig::default(), None)
                .await;

        // wait for epoch to be indexed, so that current epoch and JWK are populated in db.
        let test_cluster = cluster.validator_fullnode_handle;
        test_cluster.wait_for_epoch(Some(1)).await;
        test_cluster.wait_for_authenticator_state_update().await;

        // now query the endpoint with a valid tx data bytes and a valid signature with the correct proof for dev env.
        let bytes = "AAABACACAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgEBAQABAAAcpgUkGBwS5nPO79YXkjMyvaRjGS57hqxzfyd2yGtejwGbB4FfBEl+LgXSLKw6oGFBCyCGjMYZFUxCocYb6ZAnFwEAAAAAAAAAIJZw7UpW1XHubORIOaY8d2+WyBNwoJ+FEAxlsa7h7JHrHKYFJBgcEuZzzu/WF5IzMr2kYxkue4asc38ndshrXo8BAAAAAAAAABAnAAAAAAAAAA==";
        let signature = "BQNNMTczMTgwODkxMjU5NTI0MjE3MzYzNDIyNjM3MTc5MzI3MTk0Mzc3MTc4NDQyODI0MTAxODc5NTc5ODQ3NTE5Mzk5NDI4OTgyNTEyNTBNMTEzNzM5NjY2NDU0NjkxMjI1ODIwNzQwODIyOTU5ODUzODgyNTg4NDA2ODE2MTgyNjg1OTM5NzY2OTczMjU4OTIyODA5MTU2ODEyMDcBMQMCTDU5Mzk4NzExNDczNDg4MzQ5OTczNjE3MjAxMjIyMzg5ODAxNzcxNTIzMDMyNzQzMTEwNDcyNDk5MDU5NDIzODQ5MTU3Njg2OTA4OTVMNDUzMzU2ODI3MTEzNDc4NTI3ODczMTIzNDU3MDM2MTQ4MjY1MTk5Njc0MDc5MTg4ODI4NTg2NDk2Njg4NDAzMjcxNzA0OTgxMTcwOAJNMTA1NjQzODcyODUwNzE1NTU0Njk3NTM5OTA2NjE0MTA4NDAxMTg2MzU5MjU0NjY1OTcwMzcwMTgwNTg3NzAwNDEzNDc1MTg0NjEzNjhNMTI1OTczMjM1NDcyNzc1NzkxNDQ2OTg0OTYzNzIyNDI2MTUzNjgwODU4MDEzMTMzNDMxNTU3MzU1MTEzMzAwMDM4ODQ3Njc5NTc4NTQCATEBMANNMTU3OTE1ODk0NzI1NTY4MjYyNjMyMzE2NDQ3Mjg4NzMzMzc2MjkwMTUyNjk5ODQ2OTk0MDQwNzM2MjM2MDMzNTI1Mzc2Nzg4MTMxNzFMNDU0Nzg2NjQ5OTI0ODg4MTQ0OTY3NjE2MTE1ODAyNDc0ODA2MDQ4NTM3MzI1MDAyOTQyMzkwNDExMzAxNzQyMjUzOTAzNzE2MjUyNwExMXdpYVhOeklqb2lhSFIwY0hNNkx5OXBaQzUwZDJsMFkyZ3VkSFl2YjJGMWRHZ3lJaXcCMmV5SmhiR2NpT2lKU1V6STFOaUlzSW5SNWNDSTZJa3BYVkNJc0ltdHBaQ0k2SWpFaWZRTTIwNzk0Nzg4NTU5NjIwNjY5NTk2MjA2NDU3MDIyOTY2MTc2OTg2Njg4NzI3ODc2MTI4MjIzNjI4MTEzOTE2MzgwOTI3NTAyNzM3OTExCgAAAAAAAABhAG6Bf8BLuaIEgvF8Lx2jVoRWKKRIlaLlEJxgvqwq5nDX+rvzJxYAUFd7KeQBd9upNx+CHpmINkfgj26jcHbbqAy5xu4WMO8+cRFEpkjbBruyKE9ydM++5T/87lA8waSSAA==";
        let intent_scope = "TRANSACTION_DATA";
        let author = "0x1ca60524181c12e673ceefd617923332bda463192e7b86ac737f2776c86b5e8f";
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
    #[serial]
    async fn test_transaction_dry_run() {
        let _guard = telemetry_subscribers::TelemetryConfig::new()
            .with_env()
            .init();

        let cluster =
            sui_graphql_rpc::test_infra::cluster::start_cluster(ConnectionConfig::default(), None)
                .await;

        let addresses = cluster.validator_fullnode_handle.wallet.get_addresses();

        let sender = addresses[0];
        let recipient = addresses[1];
        let tx = cluster
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
    #[serial]
    async fn test_transaction_dry_run_with_kind() {
        let _guard = telemetry_subscribers::TelemetryConfig::new()
            .with_env()
            .init();

        let cluster =
            sui_graphql_rpc::test_infra::cluster::start_cluster(ConnectionConfig::default(), None)
                .await;

        let addresses = cluster.validator_fullnode_handle.wallet.get_addresses();

        let recipient = addresses[1];
        let tx = cluster
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
    #[serial]
    async fn test_dry_run_failed_execution() {
        let _guard = telemetry_subscribers::TelemetryConfig::new()
            .with_env()
            .init();

        let cluster =
            sui_graphql_rpc::test_infra::cluster::start_cluster(ConnectionConfig::default(), None)
                .await;

        let addresses = cluster.validator_fullnode_handle.wallet.get_addresses();

        let sender = addresses[0];
        let coin = *cluster
            .validator_fullnode_handle
            .wallet
            .get_gas_objects_owned_by_address(sender, None)
            .await
            .unwrap()
            .get(1)
            .unwrap();
        let tx = cluster
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
    #[serial]
    async fn test_epoch_data() {
        let _guard = telemetry_subscribers::TelemetryConfig::new()
            .with_env()
            .init();

        let cluster =
            sui_graphql_rpc::test_infra::cluster::start_cluster(ConnectionConfig::default(), None)
                .await;

        cluster
            .validator_fullnode_handle
            .trigger_reconfiguration()
            .await;

        // Wait for the epoch to be indexed
        sleep(Duration::from_secs(10)).await;

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

    use sui_graphql_rpc::server::builder::tests::*;

    #[tokio::test]
    #[serial]
    async fn test_timeout() {
        let _guard = telemetry_subscribers::TelemetryConfig::new()
            .with_env()
            .init();
        let cluster =
            sui_graphql_rpc::test_infra::cluster::start_cluster(ConnectionConfig::default(), None)
                .await;
        cluster
            .wait_for_checkpoint_catchup(0, Duration::from_secs(10))
            .await;
        // timeout test includes mutation timeout, which requies a [SuiClient] to be able to run
        // the test, and a transaction. [WalletContext] gives access to everything that's needed.
        let wallet = cluster.validator_fullnode_handle.wallet;
        test_timeout_impl(wallet).await;
    }

    #[tokio::test]
    #[serial]
    async fn test_query_depth_limit() {
        test_query_depth_limit_impl().await;
    }

    #[tokio::test]
    #[serial]
    async fn test_query_node_limit() {
        test_query_node_limit_impl().await;
    }

    #[tokio::test]
    #[serial]
    async fn test_query_default_page_limit() {
        let (connection_config, _) = prep_cluster().await;
        test_query_default_page_limit_impl(connection_config).await;
    }

    #[tokio::test]
    #[serial]
    async fn test_query_max_page_limit() {
        test_query_max_page_limit_impl().await;
    }

    #[tokio::test]
    #[serial]
    async fn test_query_complexity_metrics() {
        test_query_complexity_metrics_impl().await;
    }

    #[tokio::test]
    #[serial]
    async fn test_health_check() {
        let _guard = telemetry_subscribers::TelemetryConfig::new()
            .with_env()
            .init();
        let connection_config = ConnectionConfig::ci_integration_test_cfg_with_db_name(
            "sui_graphql_rpc_e2e_tests".to_string(),
            5432,
            9184,
        );
        let cluster =
            sui_graphql_rpc::test_infra::cluster::start_cluster(connection_config, None).await;

        println!("Cluster started");
        cluster
            .wait_for_checkpoint_catchup(0, Duration::from_secs(10))
            .await;
        test_health_check_impl().await;
    }
}
