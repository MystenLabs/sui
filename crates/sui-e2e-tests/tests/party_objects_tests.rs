// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rand::distributions::Distribution;
use std::net::SocketAddr;
use std::time::Duration;
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_macros::sim_test;
use sui_swarm_config::genesis_config::{AccountConfig, DEFAULT_GAS_AMOUNT};
use sui_test_transaction_builder::publish_basics_package_and_make_party_object;
use sui_types::base_types::{FullObjectRef, SuiAddress};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Owner;
use sui_types::transaction::{CallArg, ObjectArg, SharedObjectMutability};
use test_cluster::TestClusterBuilder;
use tracing::info;

/// Delete a party object as the object owner.
#[sim_test]
async fn party_object_deletion() {
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

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
                mutability: SharedObjectMutability::Mutable,
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
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

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
                    mutability: SharedObjectMutability::Mutable,
                },
            )
            .build();
        let signed = test_cluster.sign_transaction(&transaction).await;
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
        .notify_read_executed_effects("", &digests)
        .await;
}

#[sim_test]
async fn party_object_deletion_multiple_times_cert_racing() {
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

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
                    mutability: SharedObjectMutability::Mutable,
                },
            )
            .build();
        let signed = test_cluster.sign_transaction(&transaction).await;

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
        .notify_read_executed_effects("", &digests)
        .await;
}

/// Transfer a party object as the object owner.
#[sim_test]
async fn party_object_transfer() {
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

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
                mutability: SharedObjectMutability::Mutable,
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
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

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
                    mutability: SharedObjectMutability::Mutable,
                },
                SuiAddress::ZERO,
            )
            .build();
        let signed = test_cluster.sign_transaction(&transaction).await;
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
        .notify_read_executed_effects("", &digests)
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
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

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
                mutability: SharedObjectMutability::Mutable,
            },
            SuiAddress::ZERO,
        )
        .build();
    let xfer_tx = test_cluster.sign_transaction(&xfer_tx).await;

    let repeat_tx_a = test_cluster
        .test_transaction_builder_with_gas_object(sender, gas2)
        .await
        .call_object_party_transfer_single_owner(
            package_id,
            ObjectArg::SharedObject {
                id: object_id,
                initial_shared_version: object_initial_shared_version,
                mutability: SharedObjectMutability::Mutable,
            },
            SuiAddress::ZERO,
        )
        .build();
    let repeat_tx_a = test_cluster.sign_transaction(&repeat_tx_a).await;
    let repeat_tx_a_digest = *repeat_tx_a.digest();

    let repeat_tx_b = test_cluster
        .test_transaction_builder_with_gas_object(sender, gas3)
        .await
        .call_object_party_transfer_single_owner(
            package_id,
            ObjectArg::SharedObject {
                id: object_id,
                initial_shared_version: object_initial_shared_version,
                mutability: SharedObjectMutability::Mutable,
            },
            SuiAddress::ZERO,
        )
        .build();
    let repeat_tx_b = test_cluster.sign_transaction(&repeat_tx_b).await;
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
        .notify_read_executed_effects("", &[repeat_tx_a_digest, repeat_tx_b_digest])
        .await;
}

/// Use a party object immutably.
#[sim_test]
async fn party_object_read() {
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

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
                    mutability: SharedObjectMutability::Immutable,
                })],
            )
            .build();
        let signed = test_cluster.sign_transaction(&transaction).await;
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
                mutability: SharedObjectMutability::Mutable,
            },
            recipient,
        )
        .build();
    let signed_transfer = test_cluster.sign_transaction(&transfer_transaction).await;
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
                    mutability: SharedObjectMutability::Immutable,
                })],
            )
            .build();
        let signed = test_cluster.sign_transaction(&transaction).await;
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
        .notify_read_executed_effects("", &all_digests)
        .await;
    assert_eq!(effects.len(), all_digests.len());
    for effect in effects {
        assert!(effect.status().is_ok(), "Transaction failed: {effect:?}");
    }
}

/// Transfer a party object as the object owner and ensure grpc properly handles updating its
/// indexes
#[sim_test]
async fn party_object_grpc() {
    use sui_rpc::field::FieldMask;
    use sui_rpc::field::FieldMaskUtil;
    use sui_rpc::proto::sui::rpc::v2::GetObjectRequest;
    use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;
    use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
    use sui_rpc::proto::sui::rpc::v2::owner::OwnerKind;
    use sui_rpc::proto::sui::rpc::v2::state_service_client::StateServiceClient;

    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

    let test_cluster = TestClusterBuilder::new().build().await;

    let (package, object) =
        publish_basics_package_and_make_party_object(&test_cluster.wallet).await;

    let package_id = package.0;
    let object_id = object.0;
    let object_id_str = object_id.to_canonical_string(true);
    let object_initial_shared_version = object.1;

    let channel = tonic::transport::Channel::from_shared(test_cluster.rpc_url().to_owned())
        .unwrap()
        .connect()
        .await
        .unwrap();

    let mut live_data_service_client = StateServiceClient::new(channel.clone());
    let mut ledger_service_client = LedgerServiceClient::new(channel);

    // run a list operation to make sure the party object shows up for the current owner
    let resp = ledger_service_client
        .get_object({
            let mut message = GetObjectRequest::default();
            message.object_id = Some(object_id_str.clone());
            message.read_mask = Some(FieldMask::from_paths([
                "object_id",
                "version",
                "digest",
                "owner",
                "object_type",
            ]));
            message
        })
        .await
        .unwrap()
        .into_inner()
        .object
        .unwrap();
    let original_owner = resp.owner.unwrap();
    assert_eq!(original_owner.kind(), OwnerKind::ConsensusAddress);
    assert!(original_owner.address.is_some());

    let objects = live_data_service_client
        .list_owned_objects({
            let mut message = ListOwnedObjectsRequest::default();
            message.owner = original_owner.address.clone();
            message
        })
        .await
        .unwrap()
        .into_inner()
        .objects;

    // We expect that we should be able to find the consensus owned object via list
    assert!(objects.iter().any(|o| o.object_id() == object_id_str));

    // Make a transaction to transfer the party object.
    let transaction = test_cluster
        .test_transaction_builder()
        .await
        .call_object_party_transfer_single_owner(
            package_id,
            ObjectArg::SharedObject {
                id: object_id,
                initial_shared_version: object_initial_shared_version,
                mutability: SharedObjectMutability::Mutable,
            },
            SuiAddress::ZERO,
        )
        .build();
    test_cluster
        .sign_and_execute_transaction(&transaction)
        .await
        .effects
        .unwrap();

    // Once we've transferred the object to another address we need to make sure that its owner is
    // properly updated and that the owner index correctly updated
    let resp = ledger_service_client
        .get_object({
            let mut message = GetObjectRequest::default();
            message.object_id = Some(object_id_str.clone());
            message.read_mask = Some(FieldMask::from_paths([
                "object_id",
                "version",
                "digest",
                "owner",
                "object_type",
            ]));
            message
        })
        .await
        .unwrap()
        .into_inner()
        .object
        .unwrap();
    let new_owner = resp.owner.unwrap();
    assert_eq!(new_owner.kind(), OwnerKind::ConsensusAddress);
    assert_eq!(new_owner.address, Some(SuiAddress::ZERO.to_string()));

    let objects = live_data_service_client
        .list_owned_objects({
            let mut message = ListOwnedObjectsRequest::default();
            message.owner = original_owner.address;
            message
        })
        .await
        .unwrap()
        .into_inner()
        .objects;

    // We expect that the old owner shouldn't have this object listed in its index anymore
    assert!(!objects.iter().any(|o| o.object_id() == object_id_str));

    // Now we need to ensure that the object properly shows up in the new owner's index
    let objects = live_data_service_client
        .list_owned_objects({
            let mut message = ListOwnedObjectsRequest::default();
            message.owner = new_owner.address;
            message
        })
        .await
        .unwrap()
        .into_inner()
        .objects;
    assert!(objects.iter().any(|o| o.object_id() == object_id_str))
}

/// Ensure that party coin objects show up in the owner index and then resolve ignored them for gas
/// selection
#[sim_test]
async fn party_coin_grpc() {
    use sui_rpc::field::FieldMask;
    use sui_rpc::field::FieldMaskUtil;
    use sui_rpc::proto::sui::rpc::v2::Argument;
    use sui_rpc::proto::sui::rpc::v2::Command;
    use sui_rpc::proto::sui::rpc::v2::GetObjectRequest;
    use sui_rpc::proto::sui::rpc::v2::Input;
    use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;
    use sui_rpc::proto::sui::rpc::v2::MoveCall;
    use sui_rpc::proto::sui::rpc::v2::ProgrammableTransaction;
    use sui_rpc::proto::sui::rpc::v2::SimulateTransactionRequest;
    use sui_rpc::proto::sui::rpc::v2::Transaction;
    use sui_rpc::proto::sui::rpc::v2::TransactionKind;
    use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
    use sui_rpc::proto::sui::rpc::v2::owner::OwnerKind;
    use sui_rpc::proto::sui::rpc::v2::state_service_client::StateServiceClient;
    use sui_rpc::proto::sui::rpc::v2::transaction_execution_service_client::TransactionExecutionServiceClient;
    use sui_types::Identifier;
    use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
    use sui_types::transaction::{CallArg, ObjectArg, TransactionData};

    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

    let cluster = TestClusterBuilder::new().build().await;
    let channel = tonic::transport::Channel::from_shared(cluster.rpc_url().to_owned())
        .unwrap()
        .connect()
        .await
        .unwrap();

    let mut live_data_service_client = StateServiceClient::new(channel.clone());
    let mut execution_client = TransactionExecutionServiceClient::new(channel.clone());
    let mut ledger_service_client = LedgerServiceClient::new(channel);

    // Make a transaction to transfer 1 gas coin that is Address owned and 1 gas coin that is
    // ConsensusAddress owned
    let (sender, gas) = cluster.wallet.get_one_account().await.unwrap();
    let recipient = SuiAddress::ZERO;
    let gas_coin = gas[0];
    let party_coin = gas[1];
    let owned_coin = gas[2];

    let mut builder = ProgrammableTransactionBuilder::new();
    let recipient_arg = builder
        .input(CallArg::Pure(bcs::to_bytes(&recipient).unwrap()))
        .unwrap();
    let party_owner = builder.programmable_move_call(
        "0x2".parse().unwrap(),
        Identifier::new("party").unwrap(),
        Identifier::new("single_owner").unwrap(),
        vec![],
        vec![recipient_arg],
    );
    let party_coin_arg = builder
        .input(CallArg::Object(ObjectArg::ImmOrOwnedObject(party_coin)))
        .unwrap();
    builder.programmable_move_call(
        "0x2".parse().unwrap(),
        Identifier::new("transfer").unwrap(),
        Identifier::new("public_party_transfer").unwrap(),
        vec!["0x2::coin::Coin<0x2::sui::SUI>".parse().unwrap()],
        vec![party_coin_arg, party_owner],
    );
    builder
        .transfer_object(recipient, FullObjectRef::from_fastpath_ref(owned_coin))
        .unwrap();
    let ptb = builder.finish();

    let gas_data = sui_types::transaction::GasData {
        payment: vec![gas_coin],
        owner: sender,
        price: 1000,
        budget: 100_000_000,
    };

    let kind = sui_types::transaction::TransactionKind::ProgrammableTransaction(ptb);
    let tx_data = TransactionData::new_with_gas_data(kind, sender, gas_data);

    cluster
        .sign_and_execute_transaction(&tx_data)
        .await
        .effects
        .unwrap();

    // run a list operation to make sure the party and non-party coins show up
    let resp = ledger_service_client
        .get_object({
            let mut message = GetObjectRequest::default();
            message.object_id = Some(party_coin.0.to_canonical_string(true));
            message.read_mask = Some(FieldMask::from_paths([
                "object_id",
                "version",
                "digest",
                "owner",
                "object_type",
            ]));
            message
        })
        .await
        .unwrap()
        .into_inner()
        .object
        .unwrap();
    let actual_owner = resp.owner.unwrap();
    assert_eq!(actual_owner.kind(), OwnerKind::ConsensusAddress);
    assert_eq!(actual_owner.address(), recipient.to_string());
    assert!(actual_owner.version.is_some());

    let objects = live_data_service_client
        .list_owned_objects({
            let mut message = ListOwnedObjectsRequest::default();
            message.owner = Some(recipient.to_string());
            message.read_mask = Some(FieldMask::from_paths([
                "object_id",
                "version",
                "digest",
                "owner",
                "object_type",
            ]));
            message
        })
        .await
        .unwrap()
        .into_inner()
        .objects;

    // We expect that we should be able to find the party coin
    assert!(
        objects
            .iter()
            .any(|o| o.object_id() == party_coin.0.to_canonical_string(true)
                && o.owner.as_ref().is_some_and(|owner| {
                    owner.kind() == OwnerKind::ConsensusAddress
                        && owner.address() == recipient.to_string()
                        && owner.version == actual_owner.version
                }))
    );
    // We expect that we should be able to find the non-party coin
    assert!(
        objects
            .iter()
            .any(|o| o.object_id() == owned_coin.0.to_canonical_string(true)
                && o.owner.as_ref().is_some_and(|owner| {
                    owner.kind() == OwnerKind::Address
                        && owner.address() == recipient.to_string()
                        && owner.version.is_none()
                }))
    );

    // Now we need to ensure that we can properly do gas selection when we have party-gas
    let mut unresolved_transaction = Transaction::default();
    unresolved_transaction.kind = Some(TransactionKind::from({
        let mut ptb = ProgrammableTransaction::default();
        ptb.inputs = vec![{
            let mut message = Input::default();
            message.object_id = Some("0x6".to_owned());
            message
        }];
        ptb.commands = vec![Command::from({
            let mut message = MoveCall::default();
            message.package = Some("0x2".to_owned());
            message.module = Some("clock".to_owned());
            message.function = Some("timestamp_ms".to_owned());
            message.type_arguments = vec![];
            message.arguments = vec![Argument::new_input(0)];
            message
        })];
        ptb
    }));
    unresolved_transaction.sender = Some(recipient.to_string());

    let resolved = execution_client
        .simulate_transaction(
            SimulateTransactionRequest::new(unresolved_transaction).with_do_gas_selection(true),
        )
        .await
        .unwrap()
        .into_inner();

    // Assert that the simulation was successful
    assert!(
        resolved
            .transaction
            .unwrap()
            .effects
            .unwrap()
            .status
            .unwrap()
            .success
            .unwrap()
    );
}

/// Transfer a party object as the object owner and ensure jsonrpc properly handles updating its
/// indexes
#[sim_test]
async fn party_object_jsonrpc() {
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

    let test_cluster = TestClusterBuilder::new().build().await;

    let (package, object) =
        publish_basics_package_and_make_party_object(&test_cluster.wallet).await;

    let package_id = package.0;
    let object_id = object.0;
    let object_initial_shared_version = object.1;

    let client = test_cluster.sui_client();

    let object = client
        .read_api()
        .get_object_with_options(
            object_id,
            sui_json_rpc_types::SuiObjectDataOptions::new().with_owner(),
        )
        .await
        .unwrap()
        .data
        .unwrap();
    let original_owner = object.owner.unwrap();
    assert!(matches!(
        original_owner,
        Owner::ConsensusAddressOwner { .. }
    ));
    let original_owner_address = original_owner.get_owner_address().unwrap();

    let objects = client
        .read_api()
        .get_owned_objects(original_owner_address, None, None, None)
        .await
        .unwrap()
        .data;

    assert!(
        objects
            .into_iter()
            .any(|o| o.data.unwrap().object_id == object_id)
    );

    // Make a transaction to transfer the party object.
    let transaction = test_cluster
        .test_transaction_builder()
        .await
        .call_object_party_transfer_single_owner(
            package_id,
            ObjectArg::SharedObject {
                id: object_id,
                initial_shared_version: object_initial_shared_version,
                mutability: SharedObjectMutability::Mutable,
            },
            SuiAddress::ZERO,
        )
        .build();
    test_cluster
        .sign_and_execute_transaction(&transaction)
        .await
        .effects
        .unwrap();

    // Once we've transferred the object to another address we need to make sure that its owner is
    // properly updated and that the owner index correctly updated
    let object = client
        .read_api()
        .get_object_with_options(
            object_id,
            sui_json_rpc_types::SuiObjectDataOptions::new().with_owner(),
        )
        .await
        .unwrap()
        .data
        .unwrap();
    let new_owner = object.owner.unwrap();
    assert!(matches!(new_owner, Owner::ConsensusAddressOwner { .. }));
    let new_owner_address = new_owner.get_owner_address().unwrap();
    assert_eq!(new_owner_address, SuiAddress::ZERO);

    let objects = client
        .read_api()
        .get_owned_objects(original_owner_address, None, None, None)
        .await
        .unwrap()
        .data;

    assert!(
        !objects
            .into_iter()
            .any(|o| o.data.unwrap().object_id == object_id)
    );

    let objects = client
        .read_api()
        .get_owned_objects(new_owner_address, None, None, None)
        .await
        .unwrap()
        .data;

    assert!(
        objects
            .into_iter()
            .any(|o| o.data.unwrap().object_id == object_id)
    );
}
