// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use sui_core::authority_client::NetworkAuthorityClient;
use sui_core::safe_client::SafeClient;
use sui_keys::keystore::AccountKeystore;
use sui_macros::{register_fail_point_arg, sim_test};
use sui_protocol_config::ProtocolConfig;
use sui_swarm_config::genesis_config::AccountConfig;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::address_alias::get_address_alias_state_obj_initial_shared_version;
use sui_types::base_types::AuthorityName;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::messages_grpc::{
    SubmitTxRequest, SubmitTxResult, WaitForEffectsRequest, WaitForEffectsResponse,
};
use sui_types::transaction::{CallArg, ObjectArg, Transaction};
use sui_types::{SUI_ADDRESS_ALIAS_STATE_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID};
use test_cluster::TestClusterBuilder;

async fn submit_and_wait_for_effects(
    client: &Arc<SafeClient<NetworkAuthorityClient>>,
    tx: Transaction,
) -> TransactionEffects {
    let digest = *tx.digest();

    let results = client
        .submit_transaction(SubmitTxRequest::new_transaction(tx), None)
        .await
        .expect("Failed to submit transaction");
    assert_eq!(results.results.len(), 1);
    let SubmitTxResult::Submitted { consensus_position } = results.results[0] else {
        panic!("Expected Submitted result, got: {:?}", results.results[0]);
    };

    let effects = client
        .wait_for_effects(
            WaitForEffectsRequest {
                transaction_digest: Some(digest),
                consensus_position: Some(consensus_position),
                include_details: true,
                ping_type: None,
            },
            None,
        )
        .await
        .unwrap();

    let WaitForEffectsResponse::Executed {
        details: Some(details),
        effects_digest: _,
        fast_path: _,
    } = effects
    else {
        panic!("Expected Executed response, got {effects:?}");
    };

    details.effects
}

#[sim_test]
async fn test_alias_changes() {
    telemetry_subscribers::init_for_testing();

    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_address_aliases_for_testing(true);
        config
    });

    // Create accounts with more gas objects than the default
    let accounts = vec![
        AccountConfig {
            address: None,
            gas_amounts: vec![30_000_000_000; 10], // 10 gas objects for account1
        },
        AccountConfig {
            address: None,
            gas_amounts: vec![30_000_000_000; 5], // 5 gas objects for account2
        },
    ];

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(3)
        .with_additional_accounts(accounts)
        .with_state_sync_config(sui_config::p2p::StateSyncConfig {
            use_get_checkpoint_contents_v2: Some(true),
            ..Default::default()
        })
        .build()
        .await;

    let validator_handle = test_cluster
        .swarm
        .validator_node_handles()
        .into_iter()
        .next()
        .expect("No validator found");

    let address_alias_state_initial_shared_version = validator_handle.with(|node| {
        get_address_alias_state_obj_initial_shared_version(node.state().get_object_store().as_ref())
            .expect("failed to get address alias state object")
            .expect("address alias state object should exist")
    });

    let accounts = test_cluster
        .wallet
        .get_all_accounts_and_gas_objects()
        .await
        .unwrap();

    // Use the custom account we added which has more gas objects
    let (account1, gas_objects1, account1_index) = {
        let mut result = None;
        for (i, account) in accounts.iter().enumerate() {
            if account.1.len() >= 10 {
                result = Some((account.0, account.1.clone(), i));
                break;
            }
        }
        result.unwrap_or_else(|| {
            unreachable!("Should have at least one account with 10+ gas objects")
        })
    };
    let (account2, _gas_objects2) = &accounts[(account1_index + 1) % accounts.len()];
    let gas_price = test_cluster.wallet.get_reference_gas_price().await.unwrap();

    let client = test_cluster
        .authority_aggregator()
        .authority_clients
        .iter()
        .next()
        .unwrap()
        .1
        .clone();

    // Submit transaction to call enable
    let enable_tx = test_cluster
        .wallet
        .sign_transaction(
            &TestTransactionBuilder::new(account1, gas_objects1[0], gas_price)
                .move_call(
                    SUI_FRAMEWORK_PACKAGE_ID,
                    "address_alias",
                    "enable",
                    vec![CallArg::Object(ObjectArg::SharedObject {
                        id: SUI_ADDRESS_ALIAS_STATE_OBJECT_ID,
                        initial_shared_version: address_alias_state_initial_shared_version,
                        mutability: sui_types::transaction::SharedObjectMutability::Mutable,
                    })],
                )
                .build(),
        )
        .await;

    let enable_effects = submit_and_wait_for_effects(&client, enable_tx).await;
    assert!(enable_effects.status().is_ok());

    // Get the AddressAliases object created by enable
    let address_aliases_ref = enable_effects
        .created()
        .iter()
        .find(|(_, owner)| {
            matches!(
                owner,
                sui_types::object::Owner::ConsensusAddressOwner { .. }
            )
        })
        .expect("AddressAliases object should be created")
        .0;

    // Submit a dummy transaction after enable to verify sender can still transact.
    let post_enable_tx = test_cluster
        .wallet
        .sign_transaction(
            &TestTransactionBuilder::new(account1, gas_objects1[1], gas_price)
                .transfer_sui(None, account1)
                .build(),
        )
        .await;

    let effects = submit_and_wait_for_effects(&client, post_enable_tx).await;
    assert!(effects.status().is_ok());

    // Call add to add account2 as an alias for account1
    let add_tx = test_cluster
        .wallet
        .sign_transaction(
            &TestTransactionBuilder::new(account1, gas_objects1[2], gas_price)
                .move_call(
                    SUI_FRAMEWORK_PACKAGE_ID,
                    "address_alias",
                    "add",
                    vec![
                        CallArg::Object(ObjectArg::SharedObject {
                            id: address_aliases_ref.0,
                            initial_shared_version: address_aliases_ref.1,
                            mutability: sui_types::transaction::SharedObjectMutability::Mutable,
                        }),
                        CallArg::Pure(bcs::to_bytes(&account2).unwrap()),
                    ],
                )
                .build(),
        )
        .await;

    let effects = submit_and_wait_for_effects(&client, add_tx).await;
    assert!(effects.status().is_ok());
    // Wait for all validators to execute the `add` tx.
    test_cluster
        .wait_for_tx_settlement(&[*effects.transaction_digest()])
        .await;

    // Submit a transaction with account1 as sender and account2 as signer
    // Since account2 is now an alias for account1, account2 can sign transactions on behalf of account1
    let account2_keypair = test_cluster
        .wallet
        .config
        .keystore
        .export(account2)
        .unwrap();
    let alias_signer_tx_data = TestTransactionBuilder::new(account1, gas_objects1[3], gas_price)
        .transfer_sui(None, account1)
        .build();
    let alias_signer_tx =
        Transaction::from_data_and_signer(alias_signer_tx_data, vec![account2_keypair]);
    let effects = submit_and_wait_for_effects(&client, alias_signer_tx).await;
    assert!(effects.status().is_ok());

    // Call remove_alias to remove account1 from its own alias list
    let remove_alias_tx = test_cluster
        .wallet
        .sign_transaction(
            &TestTransactionBuilder::new(account1, gas_objects1[4], gas_price)
                .move_call(
                    SUI_FRAMEWORK_PACKAGE_ID,
                    "address_alias",
                    "remove",
                    vec![
                        CallArg::Object(ObjectArg::SharedObject {
                            id: address_aliases_ref.0,
                            initial_shared_version: address_aliases_ref.1,
                            mutability: sui_types::transaction::SharedObjectMutability::Mutable,
                        }),
                        CallArg::Pure(bcs::to_bytes(&account1).unwrap()),
                    ],
                )
                .build(),
        )
        .await;

    let effects = submit_and_wait_for_effects(&client, remove_alias_tx).await;
    assert!(effects.status().is_ok());
    // Wait for all validators to execute the `remove` tx.
    test_cluster
        .wait_for_tx_settlement(&[*effects.transaction_digest()])
        .await;

    // Try to submit a transaction signed by account1 itself - this should fail
    // because account1 has been removed from its own alias list
    let account1_keypair = test_cluster
        .wallet
        .config
        .keystore
        .export(&account1)
        .unwrap();

    let account1_self_signed_tx_data =
        TestTransactionBuilder::new(account1, gas_objects1[5], gas_price)
            .transfer_sui(None, account1)
            .build();

    let account1_self_signed_tx =
        Transaction::from_data_and_signer(account1_self_signed_tx_data, vec![account1_keypair]);

    let result = client
        .submit_transaction(
            SubmitTxRequest::new_transaction(account1_self_signed_tx),
            None,
        )
        .await;
    assert!(
        result.is_err(),
        "Expected transaction to be rejected, but got: {result:?}",
    );

    // Resubmit a transaction with account1 as sender and account2 as signer
    // This should still work because account2 is still in the alias list
    let alias_signer_tx_data2 = TestTransactionBuilder::new(account1, gas_objects1[6], gas_price)
        .transfer_sui(None, account1)
        .build();

    let alias_signer_tx2 =
        Transaction::from_data_and_signer(alias_signer_tx_data2, vec![account2_keypair]);

    let effects = submit_and_wait_for_effects(&client, alias_signer_tx2).await;
    assert!(effects.status().is_ok());
}

#[sim_test]
async fn test_alias_race() {
    telemetry_subscribers::init_for_testing();

    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_address_aliases_for_testing(true);
        config
    });

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(3)
        .with_state_sync_config(sui_config::p2p::StateSyncConfig {
            use_get_checkpoint_contents_v2: Some(true),
            ..Default::default()
        })
        .build()
        .await;

    let accounts = test_cluster
        .wallet
        .get_all_accounts_and_gas_objects()
        .await
        .unwrap();
    let (account, gas_objects) = &accounts[0];
    let gas_price = test_cluster.wallet.get_reference_gas_price().await.unwrap();

    let (client_name, client) = test_cluster
        .authority_aggregator()
        .authority_clients
        .iter()
        .next()
        .map(|(name, client)| (name.to_owned(), client.to_owned()))
        .unwrap();
    // Names of all validators except the one to which we are submitting a tx.
    let other_validator_names: Vec<AuthorityName> = test_cluster
        .swarm
        .validator_nodes()
        .map(|node| node.name())
        .filter(|name| name != &client_name)
        .collect();
    register_fail_point_arg(
        "consensus-validator-always-report-aliases-changed",
        move || Some(other_validator_names.clone()),
    );

    let tx = test_cluster
        .wallet
        .sign_transaction(
            &TestTransactionBuilder::new(*account, gas_objects[1], gas_price)
                .transfer_sui(None, *account)
                .build(),
        )
        .await;

    let digest = *tx.digest();

    let results = client
        .submit_transaction(SubmitTxRequest::new_transaction(tx), None)
        .await
        .expect("Failed to submit transaction");
    assert_eq!(results.results.len(), 1);
    let SubmitTxResult::Submitted { consensus_position } = results.results[0] else {
        panic!("Expected Submitted result, got: {:?}", results.results[0]);
    };

    let effects = client
        .wait_for_effects(
            WaitForEffectsRequest {
                transaction_digest: Some(digest),
                consensus_position: Some(consensus_position),
                include_details: true,
                ping_type: None,
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        matches!(effects, WaitForEffectsResponse::Rejected { .. }),
        "Expected Rejected response, got: {:?}",
        effects
    );
}
