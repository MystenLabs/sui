// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use super::*;
use crate::authority::{authority_tests::init_state_with_objects, AuthorityState};
use crate::checkpoints::CheckpointServiceNoop;
use crate::consensus_handler::SequencedConsensusTransaction;
use crate::mock_consensus::with_block_status;
use consensus_core::{BlockRef, BlockStatus};
use fastcrypto::traits::KeyPair;
use move_core_types::{account_address::AccountAddress, ident_str};
use parking_lot::Mutex;
use rand::rngs::StdRng;
use rand::SeedableRng;
use sui_types::crypto::{deterministic_random_account_key, AccountKeyPair};
use sui_types::gas::GasCostSummary;
use sui_types::messages_checkpoint::{
    CheckpointContents, CheckpointSignatureMessage, CheckpointSummary, SignedCheckpointSummary,
};
use sui_types::utils::{make_committee_key, to_sender_signed_transaction};
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::{
    base_types::{ExecutionDigests, ObjectID, SuiAddress},
    object::Object,
    transaction::{
        CallArg, CertifiedTransaction, ObjectArg, TransactionData, VerifiedTransaction,
        TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS,
    },
};

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
        mutable: true,
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
            if obj.is_shared() {
                ObjectArg::SharedObject {
                    id: obj.id(),
                    initial_shared_version: obj.version(),
                    mutable: true,
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

pub fn make_consensus_adapter_for_test(
    state: Arc<AuthorityState>,
    process_via_checkpoint: HashSet<TransactionDigest>,
    execute: bool,
    mock_block_status_receivers: Vec<BlockStatusReceiver>,
) -> Arc<ConsensusAdapter> {
    let metrics = ConsensusAdapterMetrics::new_test();

    #[derive(Clone)]
    struct SubmitDirectly {
        state: Arc<AuthorityState>,
        process_via_checkpoint: HashSet<TransactionDigest>,
        execute: bool,
        mock_block_status_receivers: Arc<Mutex<Vec<BlockStatusReceiver>>>,
    }

    #[async_trait::async_trait]
    impl ConsensusClient for SubmitDirectly {
        async fn submit(
            &self,
            transactions: &[ConsensusTransaction],
            epoch_store: &Arc<AuthorityPerEpochStore>,
        ) -> SuiResult<BlockStatusReceiver> {
            let sequenced_transactions: Vec<SequencedConsensusTransaction> = transactions
                .iter()
                .map(|txn| SequencedConsensusTransaction::new_test(txn.clone()))
                .collect();

            let checkpoint_service = Arc::new(CheckpointServiceNoop {});
            let mut transactions = Vec::new();
            let mut executed_via_checkpoint = 0;

            for tx in sequenced_transactions {
                if let Some(transaction_digest) = tx.transaction.executable_transaction_digest() {
                    if self.process_via_checkpoint.contains(&transaction_digest) {
                        epoch_store
                            .insert_finalized_transactions(vec![transaction_digest].as_slice(), 10)
                            .expect("Should not fail");
                        executed_via_checkpoint += 1;
                    } else {
                        transactions.extend(
                            epoch_store
                                .process_consensus_transactions_for_tests(
                                    vec![tx],
                                    &checkpoint_service,
                                    self.state.get_object_cache_reader().as_ref(),
                                    self.state.get_transaction_cache_reader().as_ref(),
                                    &self.state.metrics,
                                    true,
                                )
                                .await?,
                        );
                    }
                } else if let SequencedConsensusTransactionKey::External(
                    ConsensusTransactionKey::CheckpointSignature(_, checkpoint_sequence_number),
                ) = tx.transaction.key()
                {
                    epoch_store.notify_synced_checkpoint(checkpoint_sequence_number);
                } else {
                    transactions.extend(
                        epoch_store
                            .process_consensus_transactions_for_tests(
                                vec![tx],
                                &checkpoint_service,
                                self.state.get_object_cache_reader().as_ref(),
                                self.state.get_transaction_cache_reader().as_ref(),
                                &self.state.metrics,
                                true,
                            )
                            .await?,
                    );
                }
            }

            assert_eq!(
                executed_via_checkpoint,
                self.process_via_checkpoint.len(),
                "Some transactions were not executed via checkpoint"
            );

            if self.execute {
                self.state
                    .transaction_manager()
                    .enqueue(transactions, epoch_store);
            }

            assert!(
                !self.mock_block_status_receivers.lock().is_empty(),
                "No mock submit responses left"
            );
            Ok(self.mock_block_status_receivers.lock().remove(0))
        }
    }
    let epoch_store = state.epoch_store_for_testing();
    // Make a new consensus adapter instance.
    Arc::new(ConsensusAdapter::new(
        Arc::new(SubmitDirectly {
            state: state.clone(),
            process_via_checkpoint,
            execute,
            mock_block_status_receivers: Arc::new(Mutex::new(mock_block_status_receivers)),
        }),
        state.name,
        Arc::new(ConnectionMonitorStatusForTests {}),
        100_000,
        100_000,
        None,
        None,
        metrics,
        epoch_store.protocol_config().clone(),
    ))
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
        )
        .unwrap();
    waiter.await.unwrap();
}

#[tokio::test]
async fn submit_checkpoint_signature_to_consensus_adapter() {
    telemetry_subscribers::init_for_testing();

    let mut rng = StdRng::seed_from_u64(1_100);
    let (keys, committee) = make_committee_key(&mut rng);

    // Initialize an authority
    let state = init_state_with_objects(vec![]).await;
    let epoch_store = state.epoch_store_for_testing();

    // Make a new consensus adapter instance.
    let adapter = make_consensus_adapter_for_test(
        state,
        HashSet::new(),
        false,
        vec![with_block_status(BlockStatus::Sequenced(BlockRef::MIN))],
    );

    let checkpoint_summary = CheckpointSummary::new(
        &ProtocolConfig::get_for_max_version_UNSAFE(),
        1,
        2,
        10,
        &CheckpointContents::new_with_digests_only_for_tests([ExecutionDigests::random()]),
        None,
        GasCostSummary::default(),
        None,
        100,
        Vec::new(),
    );

    let authority_key = &keys[0];
    let authority = authority_key.public().into();
    let signed_checkpoint_summary = SignedCheckpointSummary::new(
        committee.epoch,
        checkpoint_summary,
        authority_key,
        authority,
    );

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
        )
        .unwrap();
    waiter.await.unwrap();
}
