// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::iter;
use std::time::Duration;
use sui_core::authority_client::AuthorityAPI;
use sui_types::error::SuiError;
use sui_types::gas::GasCostSummary;
use sui_types::messages::VerifiedTransaction;
use test_utils::authority::{get_client, spawn_test_authorities, test_authority_configs};

#[tokio::test]
async fn advance_epoch_tx_test() {
    // This test checks the following functionalities related to advance epoch transaction:
    // 1. The create_advance_epoch_tx_cert API in AuthorityState can properly sign an advance
    //    epoch transaction locally and exchange with other validators to obtain a cert.
    // 2. The timeout in the API works as expected.
    // 3. The certificate can be executed by each validator.
    let configs = test_authority_configs();
    let handles = spawn_test_authorities(iter::empty(), &configs).await;

    let tx = VerifiedTransaction::new_change_epoch(1, 0, 0, 0);
    let client0 = get_client(&configs.validator_set()[0]);
    // Make sure that validators do not accept advance epoch sent externally.
    assert!(matches!(
        client0.handle_transaction(tx.into_inner()).await,
        Err(SuiError::InvalidSystemTransaction)
    ));

    let failing_task = handles
        .first()
        .unwrap()
        .with_async(|node| async move {
            node.state()
                .create_advance_epoch_tx_cert(
                    1,
                    &GasCostSummary::new(0, 0, 0),
                    Duration::from_secs(15),
                )
                .await
        })
        .await;
    // Since we are only running the task on one validator, it will never get a quorum and hence
    // never succeed.
    assert!(failing_task.is_err());

    let tasks: Vec<_> = handles
        .iter()
        .map(|handle| {
            handle.with_async(|node| async move {
                node.state()
                    .create_advance_epoch_tx_cert(
                        1,
                        &GasCostSummary::new(0, 0, 0),
                        Duration::from_secs(1000), // A very very long time
                    )
                    .await
            })
        })
        .collect();
    let results = futures::future::join_all(tasks)
        .await
        .into_iter()
        .collect::<anyhow::Result<Vec<_>>>()
        .unwrap();
    for (handle, cert) in handles.iter().zip(results) {
        handle
            .with_async(|node| async move {
                // Check that every validator is able to execute such a cert.
                node.state().handle_certificate(&cert).await.unwrap();
            })
            .await;
    }
}

/*

use futures::future::join_all;
use multiaddr::Multiaddr;
use prometheus::Registry;
use std::sync::Arc;
use std::time::Duration;
use sui_config::{NetworkConfig, ValidatorInfo};
use sui_core::authority_active::checkpoint_driver::{
    checkpoint_process_step, CheckpointProcessControl,
};
use sui_node::SuiNodeHandle;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::committee::Committee;
use sui_types::crypto::{
    generate_proof_of_possession, get_key_pair, AuthorityKeyPair, AuthoritySignature,
    KeypairTraits, NetworkKeyPair,
};
use sui_types::error::SuiResult;
use sui_types::messages::{CallArg, ExecutionStatus, ObjectArg, TransactionEffects};
use sui_types::messages_checkpoint::AuthenticatedCheckpoint;
use sui_types::object::Object;
use sui_types::{SUI_SYSTEM_STATE_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION};
use test_utils::authority::{get_object, start_node, test_authority_configs};
use test_utils::messages::{make_transfer_sui_transaction, move_transaction};
use test_utils::objects::{generate_gas_object_with_balance, test_gas_objects};
use test_utils::test_account_keys;
use test_utils::transaction::{
    publish_counter_package, submit_shared_object_transaction,
    submit_shared_object_transaction_with_committee, submit_single_owner_transaction,
};

use sui_macros::sim_test;

#[sim_test]
async fn reconfig_end_to_end_tests() {
    let mut configs = test_authority_configs();
    for c in configs.validator_configs.iter_mut() {
        // Turn off checkpoint process so that we can have fine control over it in the test.
        c.enable_checkpoint = false;
        c.enable_reconfig = true;
    }

    let configs = Arc::new(configs);
    let validator_info = configs.validator_set();
    let mut gas_objects = test_gas_objects();
    let validator_stake = generate_gas_object_with_balance(100000000000000);

    let handles = init_validators(&configs, gas_objects.clone(), &validator_stake).await;
    let orig_committee = handles[0].with(|node| node.state().committee.load().clone());

    // get sui system state and confirm it matches network info
    let configs = configs.clone();
    let (framework_object_ref, old_committee_size) = handles[0]
        .with_async(|node| async move {
            let state = node.state();

            let sui_system_state = state.get_sui_system_state_object().await.unwrap();
            let mut net_addrs_from_chain: Vec<Multiaddr> = Vec::new();
            let old_committee_size = sui_system_state.validators.next_epoch_validators.len();
            for validator in sui_system_state.validators.active_validators {
                let address = Multiaddr::try_from(validator.metadata.net_address);
                net_addrs_from_chain.push(address.unwrap());
            }
            let mut net_addrs_from_config = Vec::new();
            for validator in configs.validator_configs() {
                net_addrs_from_config.push(validator.network_address.clone());
            }
            assert_eq!(net_addrs_from_config.len(), net_addrs_from_chain.len());
            net_addrs_from_config.sort();
            net_addrs_from_chain.sort();
            for (conf, chain) in net_addrs_from_config.iter().zip(&net_addrs_from_chain) {
                assert_eq!(conf, chain);
            }

            (
                state.get_framework_object_ref().await.unwrap(),
                old_committee_size,
            )
        })
        .await;

    let effects = create_and_register_new_validator(
        framework_object_ref,
        &mut gas_objects,
        validator_stake.compute_object_reference(),
        validator_info,
    )
    .await
    .unwrap();
    assert!(effects.status.is_ok());

    let expected_committee = handles[0]
        .with_async(|node| async move {
            let state = node.state();
            let sui_system_state = state.get_sui_system_state_object().await.unwrap();
            let new_committee_size = sui_system_state.validators.next_epoch_validators.len();
            assert_eq!(old_committee_size + 1, new_committee_size);
            sui_system_state.get_next_epoch_committee().voting_rights
        })
        .await;

    let (sender, key_pair) = test_account_keys().pop().unwrap();
    let object_ref = gas_objects.pop().unwrap().compute_object_reference();
    let transaction = make_transfer_sui_transaction(
        object_ref,
        SuiAddress::random_for_testing_only(),
        None,
        sender,
        &key_pair,
    );
    let owned_tx_digest = *transaction.digest();

    let package_ref = publish_counter_package(gas_objects.pop().unwrap(), validator_info).await;

    let publish_tx = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "create",
        package_ref,
        /* arguments */ Vec::default(),
    );
    let effects = submit_single_owner_transaction(publish_tx, validator_info).await;
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
    let ((counter_id, counter_initial_shared_version, _), _) = effects.created[0];
    increment_counter(
        gas_objects.pop().unwrap(),
        package_ref,
        counter_id,
        counter_initial_shared_version,
        validator_info,
        &orig_committee,
    )
    .await;

    // lock a transaction on one validator.
    handles[0]
        .with_async(|node| async move {
            node.state().handle_transaction(transaction).await.unwrap();
            node.state()
                .handle_transaction_info_request(owned_tx_digest.into())
                .await
                .unwrap()
                .signed_transaction
                .unwrap();
        })
        .await;

    fast_forward_to_ready_for_reconfig_start(&handles).await;
    // Start epoch change and halt all validators.
    start_epoch_change(&handles).await;

    fast_forward_to_ready_for_reconfig_finish(&handles).await;

    for handle in &handles {
        handle.with(|node| {
            // Check that the last checkpoint contains the committee of the next epoch.
            if let AuthenticatedCheckpoint::Certified(cert) = node
                .active()
                .state
                .checkpoints
                .lock()
                .latest_stored_checkpoint()
                .unwrap()
            {
                assert_eq!(
                    cert.summary.next_epoch_committee.unwrap(),
                    expected_committee
                );
            } else {
                unreachable!("Expecting checkpoint cert");
            }
        });
    }

    let results: Vec<_> = handles
        .iter()
        .map(|handle| {
            handle.with_async(|node| async {
                node.active().finish_epoch_change().await.unwrap();
            })
        })
        .collect();

    futures::future::join_all(results).await;

    // refresh the system state and network addresses
    handles[0]
        .with_async(|node| async move {
            let sui_system_state = node.state().get_sui_system_state_object().await.unwrap();
            assert_eq!(sui_system_state.epoch, 1);
            // We should now have one more active validator.
            assert_eq!(sui_system_state.validators.active_validators.len(), 5);
        })
        .await;

    handles[0]
        .with_async(|node| async move {
            // verify validator has forgotten about locked tx.
            assert!(node
                .state()
                .handle_transaction_info_request(owned_tx_digest.into())
                .await
                .unwrap()
                .signed_transaction
                .is_none());
        })
        .await;

    let new_committee = handles[0].with(|node| node.state().committee.load().clone());
    increment_counter(
        gas_objects.pop().unwrap(),
        package_ref,
        counter_id,
        counter_initial_shared_version,
        validator_info,
        &new_committee,
    )
    .await;

    for h in &handles {
        h.with_async(|node| async move {
            let objref = node
                .state()
                .get_latest_parent_entry(counter_id)
                .await
                .unwrap()
                .unwrap()
                .0;
            // counter was:
            // - v1 at creation
            // - v2 after first increment.
            // - should now be v3 after increment in new epoch.
            assert_eq!(objref.1.value(), 3);
        })
        .await;
    }
}

#[sim_test]
async fn reconfig_last_checkpoint_sync_missing_tx() {
    let mut configs = test_authority_configs();
    for c in configs.validator_configs.iter_mut() {
        // Turn off checkpoint process so that we can have fine control over it in the test.
        c.enable_checkpoint = false;
        c.enable_reconfig = true;
    }
    let validator_info = configs.validator_set();
    let mut gas_objects = test_gas_objects();
    let validator_stake = generate_gas_object_with_balance(100000000000000);

    let handles = init_validators(&configs, gas_objects.clone(), &validator_stake).await;

    fast_forward_to_ready_for_reconfig_start(&handles).await;

    let (sender, key_pair) = test_account_keys().pop().unwrap();
    let object_ref = gas_objects.pop().unwrap().compute_object_reference();
    let transaction = make_transfer_sui_transaction(
        object_ref,
        SuiAddress::random_for_testing_only(),
        None,
        sender,
        &key_pair,
    );
    // Only send the transaction to validator 0, but not other validators.
    // Since gossip is disabled by default, validator 1-3 will not see it.
    submit_single_owner_transaction(transaction, &validator_info[0..1]).await;
    tokio::time::sleep(Duration::from_secs(10)).await;
    for (idx, validator) in validator_info.iter().enumerate() {
        // Check that the object is mutated on validator 0 only.
        assert_eq!(
            get_object(validator, object_ref.0).await.version(),
            SequenceNumber::from(u64::from(idx == 0))
        );
    }

    // Start epoch change and halt all validators.
    start_epoch_change(&handles).await;

    // Create a proposal on validator 0, which ensures that the transaction above will be included
    // in the checkpoint.
    handles[0].with(|node| node.state().checkpoints.lock().set_proposal(0).unwrap());

    // Only validator 1 and 2 will participate the checkpoint progress, which will use fragments
    // involving validator 0, 1, 2. Since validator 1 and 2 don't have the above transaction
    // executed, they will actively sync and execute it. This exercises the code path where we can
    // execute a transaction from a pending checkpoint even when validator is halted.
    let futures = handles[1..3].iter().map(|handle| {
        handle.with_async(|node| async move {
            let active = node.active().clone();
            while !active
                .state
                .checkpoints
                .lock()
                .is_ready_to_finish_epoch_change()
            {
                let _ =
                    checkpoint_process_step(active.clone(), &CheckpointProcessControl::default())
                        .await;
            }
        })
    });
    // Wait for all validators to be ready for epoch change.
    join_all(futures).await;

    // Now that we have a new checkpoint cert formed for the last checkpoint, check that
    // validator 3 is able to also sync and execute the above transaction and finish epoch change.
    // This exercises the code path where a validator can execute transactions from a checkpoint
    // cert even when the validator is halted.
    handles[3]
        .with_async(|node| async move {
            while !node
                .state()
                .checkpoints
                .lock()
                .is_ready_to_finish_epoch_change()
            {
                let _ = checkpoint_process_step(
                    node.active().clone(),
                    &CheckpointProcessControl::default(),
                )
                .await;
            }
        })
        .await;
}

async fn create_and_register_new_validator(
    framework_pkg: ObjectRef,
    gas_objects: &mut Vec<Object>,
    validator_stake: ObjectRef,
    validator_info: &[ValidatorInfo],
) -> SuiResult<TransactionEffects> {
    let (new_validator, new_validator_pop) = get_new_validator();

    let validator_tx = move_transaction(
        gas_objects.pop().unwrap(),
        "sui_system",
        "request_add_validator",
        framework_pkg,
        vec![
            CallArg::Object(ObjectArg::SharedObject {
                id: SUI_SYSTEM_STATE_OBJECT_ID,
                initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
            }),
            CallArg::Pure(bcs::to_bytes(&new_validator.protocol_key()).unwrap()),
            CallArg::Pure(bcs::to_bytes(&new_validator.network_key()).unwrap()),
            CallArg::Pure(bcs::to_bytes(&new_validator_pop.as_ref()).unwrap()),
            CallArg::Pure(
                bcs::to_bytes(format!("Validator{}", new_validator.sui_address()).as_bytes())
                    .unwrap(),
            ),
            CallArg::Pure(bcs::to_bytes(&new_validator.network_address).unwrap()),
            CallArg::Object(ObjectArg::ImmOrOwnedObject(validator_stake)),
            CallArg::Pure(bcs::to_bytes(&new_validator.gas_price()).unwrap()),
            CallArg::Pure(bcs::to_bytes(&new_validator.commission_rate()).unwrap()),
        ],
    );
    submit_shared_object_transaction(validator_tx, validator_info).await
}

pub fn get_new_validator() -> (ValidatorInfo, AuthoritySignature) {
    let keypair: AuthorityKeyPair = get_key_pair().1;
    let worker_keypair: NetworkKeyPair = get_key_pair().1;
    let network_keypair: NetworkKeyPair = get_key_pair().1;
    let account_keypair = test_account_keys().pop().unwrap().1;
    let pop = generate_proof_of_possession(&keypair, account_keypair.public().into());
    (
        ValidatorInfo {
            name: "".to_string(),
            protocol_key: keypair.public().into(),
            worker_key: worker_keypair.public().clone(),
            account_key: account_keypair.public().clone().into(),
            network_key: network_keypair.public().clone(),
            stake: 1,
            delegation: 0,
            gas_price: 1,
            commission_rate: 0,
            network_address: sui_config::utils::new_network_address(),
            narwhal_primary_address: sui_config::utils::new_network_address(),
            narwhal_worker_address: sui_config::utils::new_network_address(),
            narwhal_consensus_address: sui_config::utils::new_network_address(),
        },
        pop,
    )
}

async fn init_validators(
    configs: &NetworkConfig,
    gas_objects: Vec<Object>,
    validator_stake: &Object,
) -> Vec<SuiNodeHandle> {
    let mut handles = Vec::new();
    for validator in configs.validator_configs() {
        let handle = start_node(validator, Registry::new()).await;
        let gas_objects = gas_objects.clone();
        let validator_stake = validator_stake.clone();
        handle
            .with_async(|node| async move {
                let state = node.state();

                for gas in gas_objects {
                    state.insert_genesis_object(gas).await;
                }
                state.insert_genesis_object(validator_stake).await;
            })
            .await;
        handles.push(handle);
    }
    handles
}

async fn start_epoch_change(handles: &[SuiNodeHandle]) {
    for handle in handles {
        handle
            .with_async(|node| async move {
                node.active().start_epoch_change().await.unwrap();
            })
            .await;
    }
}

async fn fast_forward_to_ready_for_reconfig_start(handles: &[SuiNodeHandle]) {
    let futures = handles.iter().map(|handle| {
        handle.with_async(|node| async move {
            let active = node.active().clone();
            while !active
                .state
                .checkpoints
                .lock()
                .is_ready_to_start_epoch_change()
            {
                let _ =
                    checkpoint_process_step(active.clone(), &CheckpointProcessControl::default())
                        .await;
            }
        })
    });
    // Wait for all validators to be ready for epoch change.
    join_all(futures).await;
}

async fn fast_forward_to_ready_for_reconfig_finish(handles: &[SuiNodeHandle]) {
    let futures = handles.iter().map(|handle| {
        handle.with_async(|node| async move {
            let active = node.active().clone();
            while !active
                .state
                .checkpoints
                .lock()
                .is_ready_to_finish_epoch_change()
            {
                let _ =
                    checkpoint_process_step(active.clone(), &CheckpointProcessControl::default())
                        .await;
            }
        })
    });

    // Wait for all validators to be ready for epoch change.
    join_all(futures).await;
}

async fn increment_counter(
    gas_object: Object,
    package_ref: ObjectRef,
    counter_id: ObjectID,
    initial_shared_version: SequenceNumber,
    validator_info: &[ValidatorInfo],
    committee: &Committee,
) {
    // Make a transaction to increment the counter.
    let transaction = move_transaction(
        gas_object,
        "counter",
        "increment",
        package_ref,
        vec![CallArg::Object(ObjectArg::SharedObject {
            id: counter_id,
            initial_shared_version,
        })],
    );
    let effects =
        submit_shared_object_transaction_with_committee(transaction, validator_info, committee)
            .await
            .unwrap();
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
}
*/
