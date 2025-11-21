// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::time::Duration;

use super::*;
use crate::authority::{AuthorityState, authority_tests::init_state_with_objects};

use crate::consensus_test_utils::make_consensus_adapter_for_test;
use crate::mock_consensus::with_block_status;
use consensus_core::BlockStatus;
use consensus_types::block::{BlockRef, PING_TRANSACTION_INDEX};
use fastcrypto::traits::KeyPair;
use move_core_types::{account_address::AccountAddress, ident_str};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng, thread_rng};
use sui_macros::sim_test;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::crypto::{AccountKeyPair, deterministic_random_account_key};
use sui_types::gas::GasCostSummary;
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointContents, CheckpointSignatureMessage, CheckpointSummary,
    SignedCheckpointSummary,
};
use sui_types::transaction::SharedObjectMutability;
use sui_types::utils::{make_committee_key_num, to_sender_signed_transaction};
use sui_types::{
    base_types::{ExecutionDigests, ObjectID, SuiAddress},
    object::Object,
    transaction::{
        CallArg, CertifiedTransaction, ObjectArg, TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS,
        TransactionData, VerifiedTransaction,
    },
};
use tokio::time::sleep;

/// Fixture: a few test gas objects.
pub fn test_gas_objects() -> Vec<Object> {
    thread_local! {
        static GAS_OBJECTS: Vec<Object> = (0..4)
            .map(|_| {
                let gas_object_id = ObjectID::random();
                let (owner, _) = deterministic_random_account_key();
                Object::with_id_owner_for_testing(gas_object_id, owner)
            })
            .collect();
    }

    GAS_OBJECTS.with(|v| v.clone())
}

/// Fixture: create a few test certificates containing a shared object.
pub async fn test_certificates(
    authority: &AuthorityState,
    shared_object: Object,
) -> Vec<CertifiedTransaction> {
    test_certificates_with_gas_objects(authority, &test_gas_objects(), shared_object).await
}

/// Fixture: create a few test certificates containing a shared object using specified gas objects.
pub async fn test_certificates_with_gas_objects(
    authority: &AuthorityState,
    gas_objects: &[Object],
    shared_object: Object,
) -> Vec<CertifiedTransaction> {
    let epoch_store = authority.load_epoch_store_one_call_per_task();
    let (sender, keypair) = deterministic_random_account_key();
    let rgp = epoch_store.reference_gas_price();

    let mut certificates = Vec::new();
    let shared_object_arg = ObjectArg::SharedObject {
        id: shared_object.id(),
        initial_shared_version: shared_object.version(),
        mutability: SharedObjectMutability::Mutable,
    };
    for gas_object in gas_objects {
        // Object digest may be different in genesis than originally generated.
        let gas_object = authority.get_object(&gas_object.id()).await.unwrap();
        // Make a sample transaction.
        let module = "object_basics";
        let function = "create";

        let data = TransactionData::new_move_call(
            sender,
            SUI_FRAMEWORK_PACKAGE_ID,
            ident_str!(module).to_owned(),
            ident_str!(function).to_owned(),
            /* type_args */ vec![],
            gas_object.compute_object_reference(),
            /* args */
            vec![
                CallArg::Object(shared_object_arg),
                CallArg::Pure(16u64.to_le_bytes().to_vec()),
                CallArg::Pure(bcs::to_bytes(&AccountAddress::from(sender)).unwrap()),
            ],
            rgp * TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS,
            rgp,
        )
        .unwrap();

        let transaction = epoch_store
            .verify_transaction(to_sender_signed_transaction(data, &keypair))
            .unwrap();

        // Submit the transaction and assemble a certificate.
        let response = authority
            .handle_transaction(&epoch_store, transaction.clone())
            .await
            .unwrap();
        let vote = response.status.into_signed_for_testing();
        let certificate = CertifiedTransaction::new(
            transaction.into_message(),
            vec![vote.clone()],
            &authority.clone_committee_for_testing(),
        )
        .unwrap();
        certificates.push(certificate);
    }
    certificates
}

/// Fixture: creates a transaction using the specified gas and input objects.
pub async fn test_user_transaction(
    authority: &AuthorityState,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    gas_object: Object,
    input_objects: Vec<Object>,
) -> VerifiedTransaction {
    let epoch_store = authority.load_epoch_store_one_call_per_task();
    let rgp = epoch_store.reference_gas_price();

    // Object digest may be different in genesis than originally generated.
    let gas_object = authority.get_object(&gas_object.id()).await.unwrap();
    let mut input_objs = vec![];
    for obj in input_objects {
        input_objs.push(authority.get_object(&obj.id()).await.unwrap());
    }

    let mut object_args: Vec<_> = input_objs
        .into_iter()
        .map(|obj| {
            if obj.is_consensus() {
                ObjectArg::SharedObject {
                    id: obj.id(),
                    initial_shared_version: obj.version(),
                    mutability: SharedObjectMutability::Mutable,
                }
            } else {
                ObjectArg::ImmOrOwnedObject(obj.compute_object_reference())
            }
        })
        .map(CallArg::Object)
        .collect();
    object_args.extend(vec![
        CallArg::Pure(16u64.to_le_bytes().to_vec()),
        CallArg::Pure(bcs::to_bytes(&AccountAddress::from(sender)).unwrap()),
    ]);

    // Make a sample transaction.
    let module = "object_basics";
    let function = "create";

    let data = TransactionData::new_move_call(
        sender,
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!(module).to_owned(),
        ident_str!(function).to_owned(),
        /* type_args */ vec![],
        gas_object.compute_object_reference(),
        object_args,
        rgp * TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS,
        rgp,
    )
    .unwrap();

    epoch_store
        .verify_transaction(to_sender_signed_transaction(data, keypair))
        .unwrap()
}

#[tokio::test]
async fn submit_transaction_to_consensus_adapter() {
    telemetry_subscribers::init_for_testing();

    // Initialize an authority with a (owned) gas object and a shared object; then
    // make a test certificate.
    let mut objects = test_gas_objects();
    let shared_object = Object::shared_for_testing();
    objects.push(shared_object.clone());
    let state = init_state_with_objects(objects).await;
    let certificate = test_certificates(&state, shared_object)
        .await
        .pop()
        .unwrap();
    let epoch_store = state.epoch_store_for_testing();

    // Make a new consensus adapter instance.
    let block_status_receivers = vec![
        with_block_status(BlockStatus::GarbageCollected(BlockRef::MIN)),
        with_block_status(BlockStatus::GarbageCollected(BlockRef::MIN)),
        with_block_status(BlockStatus::GarbageCollected(BlockRef::MIN)),
        with_block_status(BlockStatus::Sequenced(BlockRef::MIN)),
    ];
    let adapter = make_consensus_adapter_for_test(
        state.clone(),
        HashSet::new(),
        false,
        block_status_receivers,
    );

    // Submit the transaction and ensure the adapter reports success to the caller. Note
    // that consensus may drop some transactions (so we may need to resubmit them).
    let transaction = ConsensusTransaction::new_certificate_message(&state.name, certificate);
    let waiter = adapter
        .submit(
            transaction.clone(),
            Some(&epoch_store.get_reconfig_state_read_lock_guard()),
            &epoch_store,
            None,
            None,
        )
        .unwrap();
    waiter.await.unwrap();
}

#[tokio::test]
async fn submit_multiple_transactions_to_consensus_adapter() {
    telemetry_subscribers::init_for_testing();

    // Initialize an authority with a (owned) gas object and a shared object; then
    // make a test certificate.
    let mut objects = test_gas_objects();
    let shared_object = Object::shared_for_testing();
    objects.push(shared_object.clone());
    let state = init_state_with_objects(objects).await;
    let certificates = test_certificates(&state, shared_object).await;
    let epoch_store = state.epoch_store_for_testing();

    // Mark the first two transactions to be "executed via checkpoint" and the other two to appear via consensus output.
    assert_eq!(certificates.len(), 4);

    let mut process_via_checkpoint = HashSet::new();
    process_via_checkpoint.insert(*certificates[0].digest());
    process_via_checkpoint.insert(*certificates[1].digest());

    // Make a new consensus adapter instance.
    let adapter = make_consensus_adapter_for_test(
        state.clone(),
        process_via_checkpoint,
        false,
        vec![with_block_status(BlockStatus::Sequenced(BlockRef::MIN))],
    );

    // Submit the transaction and ensure the adapter reports success to the caller. Note
    // that consensus may drop some transactions (so we may need to resubmit them).
    let transactions = certificates
        .into_iter()
        .map(|certificate| ConsensusTransaction::new_certificate_message(&state.name, certificate))
        .collect::<Vec<_>>();

    let waiter = adapter
        .submit_batch(
            &transactions,
            Some(&epoch_store.get_reconfig_state_read_lock_guard()),
            &epoch_store,
            None,
            None,
        )
        .unwrap();
    waiter.await.unwrap();
}

#[sim_test]
async fn submit_checkpoint_signature_to_consensus_adapter() {
    telemetry_subscribers::init_for_testing();

    let mut rng = StdRng::seed_from_u64(1_100);
    let (keys, committee) = make_committee_key_num(1, &mut rng);

    // Initialize an authority
    let state = init_state_with_objects(vec![]).await;
    let epoch_store = state.epoch_store_for_testing();

    // Make a new consensus adapter instance.
    let adapter = make_consensus_adapter_for_test(
        state.clone(),
        HashSet::new(),
        false,
        vec![with_block_status(BlockStatus::Sequenced(BlockRef::MIN))],
    );

    let checkpoint_summary = CheckpointSummary::new(
        &ProtocolConfig::get_for_max_version_UNSAFE(),
        0,
        2,
        10,
        &CheckpointContents::new_with_digests_only_for_tests([ExecutionDigests::random()]),
        None,
        GasCostSummary::default(),
        None,
        100,
        Vec::new(),
        Vec::new(),
    );

    let authority_key = &keys[0];
    let authority = authority_key.public().into();
    let signed_checkpoint_summary = SignedCheckpointSummary::new(
        committee.epoch,
        checkpoint_summary.clone(),
        authority_key,
        authority,
    );

    let checkpoint_cert = CertifiedCheckpointSummary::new(
        checkpoint_summary,
        vec![signed_checkpoint_summary.auth_sig().clone()],
        &committee,
    )
    .unwrap();

    let verified_checkpoint_summary = checkpoint_cert.try_into_verified(&committee).unwrap();

    let t1 = tokio::spawn({
        let state = state.clone();
        let verified_checkpoint_summary = verified_checkpoint_summary.clone();

        async move {
            let delay = Duration::from_millis(thread_rng().gen_range(0..1000));
            sleep(delay).await;
            state
                .checkpoint_store
                .insert_verified_checkpoint(&verified_checkpoint_summary)
                .unwrap();
            state
                .checkpoint_store
                .update_highest_synced_checkpoint(&verified_checkpoint_summary)
                .unwrap();
        }
    });

    let t2 = tokio::spawn(async move {
        let transactions = vec![ConsensusTransaction::new_checkpoint_signature_message(
            CheckpointSignatureMessage {
                summary: signed_checkpoint_summary,
            },
        )];

        let waiter = adapter
            .submit_batch(
                &transactions,
                Some(&epoch_store.get_reconfig_state_read_lock_guard()),
                &epoch_store,
                None,
                None,
            )
            .unwrap();
        waiter.await.unwrap();
    });

    t1.await.unwrap();
    t2.await.unwrap();
}

#[tokio::test]
async fn submit_empty_array_of_transactions_to_consensus_adapter() {
    telemetry_subscribers::init_for_testing();

    // Initialize an authority
    let state = init_state_with_objects(vec![]).await;
    let epoch_store = state.epoch_store_for_testing();

    // Make a new consensus adapter instance.
    let adapter = make_consensus_adapter_for_test(state.clone(), HashSet::new(), false, vec![]);

    // Submit the transaction and ensure the adapter reports success to the caller. Note
    // that consensus may drop some transactions (so we may need to resubmit them).
    let (tx_consensus_position, rx_consensus_position) = oneshot::channel();
    let waiter = adapter
        .submit_batch(
            &[],
            Some(&epoch_store.get_reconfig_state_read_lock_guard()),
            &epoch_store,
            Some(tx_consensus_position),
            None,
        )
        .unwrap();
    waiter.await.unwrap();

    let consensus_position = rx_consensus_position.await.unwrap();
    assert_eq!(
        consensus_position,
        vec![ConsensusPosition {
            epoch: epoch_store.epoch(),
            block: BlockRef::MIN,
            index: PING_TRANSACTION_INDEX,
        }]
    );
}
