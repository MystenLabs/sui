// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rand::distributions::Distribution;
use std::net::SocketAddr;
use std::time::Duration;
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_macros::sim_test;
use sui_swarm_config::genesis_config::{AccountConfig, DEFAULT_GAS_AMOUNT};
use sui_test_transaction_builder::publish_basics_package_and_make_party_object;
use sui_types::base_types::SuiAddress;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Owner;
use sui_types::transaction::{CallArg, ObjectArg};
use test_cluster::TestClusterBuilder;
use tracing::info;

/// Delete a party object as the object owner.
#[sim_test]
async fn party_object_deletion() {
    telemetry_subscribers::init_for_testing();
    let test_cluster = TestClusterBuilder::new().build().await;

    let (package, object) =
        publish_basics_package_and_make_party_object(&test_cluster.wallet).await;
    let package_id = package.0;
    let object_id = object.0;
    let object_initial_shared_version = object.1;

    // Make a transaction to delete the party object.
    let transaction = test_cluster
        .test_transaction_builder()
        .await
        .call_object_delete(
            package_id,
            ObjectArg::SharedObject {
                id: object_id,
                initial_shared_version: object_initial_shared_version,
                mutable: true,
            },
        )
        .build();
    let effects = test_cluster
        .sign_and_execute_transaction(&transaction)
        .await
        .effects
        .unwrap();

    assert_eq!(effects.deleted().len(), 1);
    assert_eq!(effects.shared_objects().len(), 1);

    // assert the shared object was deleted
    let deleted_obj_id = effects.deleted()[0].object_id;
    assert_eq!(deleted_obj_id, object_id);
}

#[sim_test]
async fn party_object_deletion_multiple_times() {
    telemetry_subscribers::init_for_testing();

    let num_deletions = 20;
    let mut test_cluster = TestClusterBuilder::new()
        .with_accounts(vec![AccountConfig {
            address: None,
            gas_amounts: vec![DEFAULT_GAS_AMOUNT; num_deletions],
        }])
        .build()
        .await;

    let (package, object) =
        publish_basics_package_and_make_party_object(&test_cluster.wallet).await;
    let package_id = package.0;
    let object_id = object.0;
    let object_initial_shared_version = object.1;

    let accounts_and_gas = test_cluster
        .wallet
        .get_all_accounts_and_gas_objects()
        .await
        .unwrap();
    let sender = accounts_and_gas[0].0;
    let gas_coins = accounts_and_gas[0].1.clone();

    // Make a bunch transactions that all want to delete the party object.
    let mut txs = vec![];
    for coin_ref in gas_coins.into_iter() {
        let transaction = test_cluster
            .test_transaction_builder_with_gas_object(sender, coin_ref)
            .await
            .call_object_delete(
                package_id,
                ObjectArg::SharedObject {
                    id: object_id,
                    initial_shared_version: object_initial_shared_version,
                    mutable: true,
                },
            )
            .build();
        let signed = test_cluster.sign_transaction(&transaction);
        let client_ip = SocketAddr::new([127, 0, 0, 1].into(), 0);
        test_cluster
            .create_certificate(signed.clone(), Some(client_ip))
            .await
            .unwrap();
        txs.push(signed);
    }

    // Submit all the deletion transactions to the validators.
    let validators = test_cluster.get_validator_pubkeys();
    let submissions = txs.iter().map(|tx| async {
        test_cluster
            .submit_transaction_to_validators(tx.clone(), &validators)
            .await
            .unwrap();
        *tx.digest()
    });
    let digests = futures::future::join_all(submissions).await;

    // Start a new fullnode and let it sync from genesis and wait for us to see all the deletion
    // transactions.
    let fullnode = test_cluster.spawn_new_fullnode().await.sui_node;
    fullnode
        .state()
        .get_transaction_cache_reader()
        .notify_read_executed_effects(&digests)
        .await;
}

#[sim_test]
async fn party_object_deletion_multiple_times_cert_racing() {
    telemetry_subscribers::init_for_testing();

    let num_deletions = 10;
    let mut test_cluster = TestClusterBuilder::new()
        .with_accounts(vec![AccountConfig {
            address: None,
            gas_amounts: vec![DEFAULT_GAS_AMOUNT; num_deletions],
        }])
        .build()
        .await;

    let (package, object) =
        publish_basics_package_and_make_party_object(&test_cluster.wallet).await;
    let package_id = package.0;
    let object_id = object.0;
    let object_initial_shared_version = object.1;

    let accounts_and_gas = test_cluster
        .wallet
        .get_all_accounts_and_gas_objects()
        .await
        .unwrap();
    let sender = accounts_and_gas[0].0;
    let gas_coins = accounts_and_gas[0].1.clone();

    // Make a bunch of transactions that all want to delete the party object.
    let validators = test_cluster.get_validator_pubkeys();
    let mut digests = vec![];
    for coin_ref in gas_coins.into_iter() {
        let transaction = test_cluster
            .test_transaction_builder_with_gas_object(sender, coin_ref)
            .await
            .call_object_delete(
                package_id,
                ObjectArg::SharedObject {
                    id: object_id,
                    initial_shared_version: object_initial_shared_version,
                    mutable: true,
                },
            )
            .build();
        let signed = test_cluster.sign_transaction(&transaction);

        let client_ip = SocketAddr::new([127, 0, 0, 1].into(), 0);
        test_cluster
            .create_certificate(signed.clone(), Some(client_ip))
            .await
            .unwrap();
        info!(
            "Submitting transaction with digest: {:?}\n{:#?}",
            signed.digest(),
            signed.data().inner().intent_message().value
        );
        test_cluster
            .submit_transaction_to_validators(signed.clone(), &validators)
            .await
            .unwrap();
        digests.push(*signed.digest());
    }

    // Start a new fullnode and let it sync from genesis and wait for us to see all the deletion
    // transactions.
    let fullnode = test_cluster.spawn_new_fullnode().await.sui_node;
    fullnode
        .state()
        .get_transaction_cache_reader()
        .notify_read_executed_effects(&digests)
        .await;
}

/// Transfer a party object as the object owner.
#[sim_test]
async fn party_object_transfer() {
    telemetry_subscribers::init_for_testing();
    let test_cluster = TestClusterBuilder::new().build().await;

    let (package, object) =
        publish_basics_package_and_make_party_object(&test_cluster.wallet).await;
    let package_id = package.0;
    let object_id = object.0;
    let object_initial_shared_version = object.1;

    // Make a transaction to transfer the party object.
    let transaction = test_cluster
        .test_transaction_builder()
        .await
        .call_object_party_transfer_single_owner(
            package_id,
            ObjectArg::SharedObject {
                id: object_id,
                initial_shared_version: object_initial_shared_version,
                mutable: true,
            },
            SuiAddress::ZERO,
        )
        .build();
    let effects = test_cluster
        .sign_and_execute_transaction(&transaction)
        .await
        .effects
        .unwrap();

    assert_eq!(effects.shared_objects().len(), 1);
    let mutated_party = effects
        .mutated()
        .iter()
        .filter(|obj| matches!(obj.owner, Owner::ConsensusAddressOwner { .. }))
        .collect::<Vec<_>>();
    assert_eq!(mutated_party.len(), 1);
    let mutated_party = mutated_party[0];
    assert_eq!(
        mutated_party.owner,
        Owner::ConsensusAddressOwner {
            start_version: object_initial_shared_version.next(),
            owner: SuiAddress::ZERO,
        }
    );
}

#[sim_test]
async fn party_object_transfer_multiple_times() {
    telemetry_subscribers::init_for_testing();

    let num_transfers = 20;
    let mut test_cluster = TestClusterBuilder::new()
        .with_accounts(vec![AccountConfig {
            address: None,
            gas_amounts: vec![DEFAULT_GAS_AMOUNT; num_transfers],
        }])
        .build()
        .await;

    let (package, object) =
        publish_basics_package_and_make_party_object(&test_cluster.wallet).await;
    let package_id = package.0;
    let object_id = object.0;
    let object_initial_shared_version = object.1;

    let accounts_and_gas = test_cluster
        .wallet
        .get_all_accounts_and_gas_objects()
        .await
        .unwrap();
    let sender = accounts_and_gas[0].0;
    let gas_coins = accounts_and_gas[0].1.clone();

    // Make a bunch transactions that all want to transfer the party object.
    let mut txs = vec![];
    for coin_ref in gas_coins.into_iter() {
        let transaction = test_cluster
            .test_transaction_builder_with_gas_object(sender, coin_ref)
            .await
            .call_object_party_transfer_single_owner(
                package_id,
                ObjectArg::SharedObject {
                    id: object_id,
                    initial_shared_version: object_initial_shared_version,
                    mutable: true,
                },
                SuiAddress::ZERO,
            )
            .build();
        let signed = test_cluster.sign_transaction(&transaction);
        let client_ip = SocketAddr::new([127, 0, 0, 1].into(), 0);
        test_cluster
            .create_certificate(signed.clone(), Some(client_ip))
            .await
            .unwrap();
        txs.push(signed);
    }

    // Submit all the transfer transactions to the validators.
    let validators = test_cluster.get_validator_pubkeys();
    let submissions = txs.iter().map(|tx| async {
        test_cluster
            .submit_transaction_to_validators(tx.clone(), &validators)
            .await
            .unwrap();
        *tx.digest()
    });
    let digests = futures::future::join_all(submissions).await;

    // Start a new fullnode and let it sync from genesis and wait for us to see all the transfer
    // transactions.
    let fullnode = test_cluster.spawn_new_fullnode().await.sui_node;
    fullnode
        .state()
        .get_transaction_cache_reader()
        .notify_read_executed_effects(&digests)
        .await;
}

/// Test for execution of party object certs that are sequenced after a party object is transferred.
/// The test strategy is:
/// 0. Inject a random delay just before execution of a transaction.
/// 1. Create a shared object
/// 2. Create three transfer certs, but do not execute any of them yet.
/// 3. Execute one.
/// 4. Execute the remaining two.
#[sim_test]
async fn party_object_transfer_multi_certs() {
    telemetry_subscribers::init_for_testing();

    // cause random delay just before tx is executed (to explore all orders)
    sui_macros::register_fail_point_async("transaction_execution_delay", move || async move {
        let delay = {
            let dist = rand::distributions::Uniform::new(0, 1000);
            let mut rng = rand::thread_rng();
            dist.sample(&mut rng)
        };
        tokio::time::sleep(Duration::from_millis(delay)).await;
    });

    let mut test_cluster = TestClusterBuilder::new().build().await;

    let (package, object) =
        publish_basics_package_and_make_party_object(&test_cluster.wallet).await;
    let package_id = package.0;
    let object_id = object.0;
    let object_initial_shared_version = object.1;

    let accounts_and_gas = test_cluster
        .wallet
        .get_all_accounts_and_gas_objects()
        .await
        .unwrap();

    let sender = accounts_and_gas[0].0;
    let gas1 = accounts_and_gas[0].1[0];
    let gas2 = accounts_and_gas[0].1[1];
    let gas3 = accounts_and_gas[0].1[2];

    let xfer_tx = test_cluster
        .test_transaction_builder_with_gas_object(sender, gas1)
        .await
        .call_object_party_transfer_single_owner(
            package_id,
            ObjectArg::SharedObject {
                id: object_id,
                initial_shared_version: object_initial_shared_version,
                mutable: true,
            },
            SuiAddress::ZERO,
        )
        .build();
    let xfer_tx = test_cluster.sign_transaction(&xfer_tx);

    let repeat_tx_a = test_cluster
        .test_transaction_builder_with_gas_object(sender, gas2)
        .await
        .call_object_party_transfer_single_owner(
            package_id,
            ObjectArg::SharedObject {
                id: object_id,
                initial_shared_version: object_initial_shared_version,
                mutable: true,
            },
            SuiAddress::ZERO,
        )
        .build();
    let repeat_tx_a = test_cluster.sign_transaction(&repeat_tx_a);
    let repeat_tx_a_digest = *repeat_tx_a.digest();

    let repeat_tx_b = test_cluster
        .test_transaction_builder_with_gas_object(sender, gas3)
        .await
        .call_object_party_transfer_single_owner(
            package_id,
            ObjectArg::SharedObject {
                id: object_id,
                initial_shared_version: object_initial_shared_version,
                mutable: true,
            },
            SuiAddress::ZERO,
        )
        .build();
    let repeat_tx_b = test_cluster.sign_transaction(&repeat_tx_b);
    let repeat_tx_b_digest = *repeat_tx_b.digest();
    let client_ip = SocketAddr::new([127, 0, 0, 1].into(), 0);

    let _ = test_cluster
        .create_certificate(xfer_tx.clone(), Some(client_ip))
        .await
        .unwrap();
    let _ = test_cluster
        .create_certificate(repeat_tx_a.clone(), Some(client_ip))
        .await
        .unwrap();
    let _ = test_cluster
        .create_certificate(repeat_tx_b.clone(), Some(client_ip))
        .await
        .unwrap();

    let validators = test_cluster.get_validator_pubkeys();

    // transfer obj on all validators, await effects
    test_cluster
        .submit_transaction_to_validators(xfer_tx, &validators)
        .await
        .unwrap();

    // now submit remaining txns simultaneously
    futures::join!(
        async {
            test_cluster
                .submit_transaction_to_validators(repeat_tx_a, &validators)
                .await
                .unwrap()
        },
        async {
            test_cluster
                .submit_transaction_to_validators(repeat_tx_b, &validators)
                .await
                .unwrap()
        }
    );

    // Start a new fullnode that is not on the write path
    let fullnode = test_cluster.spawn_new_fullnode().await.sui_node;
    fullnode
        .state()
        .get_transaction_cache_reader()
        .notify_read_executed_effects(&[repeat_tx_a_digest, repeat_tx_b_digest])
        .await;
}

/// Use a party object immutably.
#[sim_test]
async fn party_object_read() {
    telemetry_subscribers::init_for_testing();

    // Create a test cluster with enough gas coins for the below.
    let num_reads = 10;
    let mut test_cluster = TestClusterBuilder::new()
        .with_accounts(vec![
            AccountConfig {
                address: None,
                gas_amounts: vec![DEFAULT_GAS_AMOUNT; num_reads / 2 + 1], // First account
            },
            AccountConfig {
                address: None,
                gas_amounts: vec![DEFAULT_GAS_AMOUNT; num_reads / 2 + 1], // Second account
            },
        ])
        .build()
        .await;

    let (package, object) =
        publish_basics_package_and_make_party_object(&test_cluster.wallet).await;
    let package_id = package.0;
    let object_id = object.0;
    let mut object_initial_shared_version = object.1;

    let accounts_and_gas = test_cluster
        .wallet
        .get_all_accounts_and_gas_objects()
        .await
        .unwrap();
    let sender = accounts_and_gas[0].0;
    let gas_coins_account1 = accounts_and_gas[0].1.clone();
    let recipient = accounts_and_gas[1].0;
    let gas_coins_account2 = accounts_and_gas[1].1.clone();

    // Make some transactions that read the party object.
    let mut all_digests = vec![];
    for gas_coin in gas_coins_account1.iter().take(num_reads / 2) {
        let transaction = test_cluster
            .test_transaction_builder_with_gas_object(sender, *gas_coin)
            .await
            .move_call(
                package_id,
                "object_basics",
                "get_value",
                vec![CallArg::Object(ObjectArg::SharedObject {
                    id: object_id,
                    initial_shared_version: object_initial_shared_version,
                    mutable: false,
                })],
            )
            .build();
        let signed = test_cluster.sign_transaction(&transaction);
        let client_ip = SocketAddr::new([127, 0, 0, 1].into(), 0);
        test_cluster
            .create_certificate(signed.clone(), Some(client_ip))
            .await
            .unwrap();

        let validators = test_cluster.get_validator_pubkeys();
        test_cluster
            .submit_transaction_to_validators(signed.clone(), &validators)
            .await
            .unwrap();
        all_digests.push(*signed.digest());
    }

    // Make a transaction to transfer the party object to a different account in the cluster.
    let transfer_gas = gas_coins_account1[num_reads / 2];
    let transfer_transaction = test_cluster
        .test_transaction_builder_with_gas_object(sender, transfer_gas)
        .await
        .call_object_party_transfer_single_owner(
            package_id,
            ObjectArg::SharedObject {
                id: object_id,
                initial_shared_version: object_initial_shared_version,
                mutable: true,
            },
            recipient,
        )
        .build();
    let signed_transfer = test_cluster.sign_transaction(&transfer_transaction);
    let client_ip = SocketAddr::new([127, 0, 0, 1].into(), 0);
    test_cluster
        .create_certificate(signed_transfer.clone(), Some(client_ip))
        .await
        .unwrap();

    let validators = test_cluster.get_validator_pubkeys();
    let (transfer_effects, _) = test_cluster
        .submit_transaction_to_validators(signed_transfer.clone(), &validators)
        .await
        .unwrap();
    all_digests.push(*signed_transfer.digest());

    // Find the party object in the mutated objects and get its new start version
    let mutated_party = transfer_effects
        .mutated()
        .into_iter()
        .find(|obj| matches!(obj.1, Owner::ConsensusAddressOwner { .. }))
        .expect("Party object should be mutated");
    object_initial_shared_version = mutated_party.1.start_version().unwrap();

    // Make some more transactions that read the party object from the new owner.
    for gas_coin in gas_coins_account2.iter().take(num_reads / 2) {
        let transaction = test_cluster
            .test_transaction_builder_with_gas_object(recipient, *gas_coin)
            .await
            .move_call(
                package_id,
                "object_basics",
                "get_value",
                vec![CallArg::Object(ObjectArg::SharedObject {
                    id: object_id,
                    initial_shared_version: object_initial_shared_version,
                    mutable: false,
                })],
            )
            .build();
        let signed = test_cluster.sign_transaction(&transaction);
        let client_ip = SocketAddr::new([127, 0, 0, 1].into(), 0);
        test_cluster
            .create_certificate(signed.clone(), Some(client_ip))
            .await
            .unwrap();

        let validators = test_cluster.get_validator_pubkeys();
        test_cluster
            .submit_transaction_to_validators(signed.clone(), &validators)
            .await
            .unwrap();
        all_digests.push(*signed.digest());
    }

    // Start a new fullnode and let it sync from genesis and wait for us to see all the
    // transactions.
    let fullnode = test_cluster.spawn_new_fullnode().await.sui_node;
    let effects = fullnode
        .state()
        .get_transaction_cache_reader()
        .notify_read_executed_effects(&all_digests)
        .await;
    assert_eq!(effects.len(), all_digests.len());
    for effect in effects {
        assert!(effect.status().is_ok(), "Transaction failed: {effect:?}");
    }
}
