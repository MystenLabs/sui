// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::time::Duration;

use super::*;
use crate::authority::{AuthorityState, authority_tests::init_state_with_objects};

use crate::authority::consensus_tx_status_cache::{
    CONSENSUS_STATUS_RETENTION_ROUNDS, ConsensusTxStatus,
};
use crate::consensus_test_utils::{
    make_consensus_adapter_for_test, make_consensus_adapter_for_test_with_submit_limit,
    make_consensus_adapter_with_client_for_test,
};
use crate::mock_consensus::with_block_status;
use consensus_core::BlockStatus;
use consensus_types::block::{BlockRef, PING_TRANSACTION_INDEX};
use fastcrypto::traits::KeyPair;
use move_core_types::{account_address::AccountAddress, ident_str};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng, thread_rng};
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::crypto::{AccountKeyPair, deterministic_random_account_key};
use sui_types::error::SuiErrorKind;
use sui_types::gas::GasCostSummary;
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointContents, CheckpointSignatureMessage, CheckpointSummary,
    SignedCheckpointSummary,
};
use sui_types::transaction::SharedObjectMutability;
use sui_types::transaction::VerifiedTransactionWithAliases;
use sui_types::utils::{make_committee_key_num, to_sender_signed_transaction};
use sui_types::{
    base_types::{ExecutionDigests, ObjectID, SuiAddress},
    object::Object,
    transaction::{CallArg, ObjectArg, TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS, TransactionData},
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

/// Fixture: create a few test user transactions containing a shared object.
pub async fn test_user_transactions(
    authority: &AuthorityState,
    shared_object: Object,
) -> Vec<VerifiedTransactionWithAliases> {
    test_user_transactions_with_gas_objects(authority, &test_gas_objects(), shared_object).await
}

/// Fixture: create a few test user transactions containing a shared object using specified gas objects.
pub async fn test_user_transactions_with_gas_objects(
    authority: &AuthorityState,
    gas_objects: &[Object],
    shared_object: Object,
) -> Vec<VerifiedTransactionWithAliases> {
    let epoch_store = authority.load_epoch_store_one_call_per_task();
    let (sender, keypair) = deterministic_random_account_key();
    let rgp = epoch_store.reference_gas_price();

    let mut transactions = Vec::new();
    let shared_object_arg = ObjectArg::SharedObject {
        id: shared_object.id(),
        initial_shared_version: shared_object.version(),
        mutability: SharedObjectMutability::Mutable,
    };
    for gas_object in gas_objects {
        // Object digest may be different in genesis than originally generated.
        let gas_object = authority.get_object(&gas_object.id()).unwrap();
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
            .verify_transaction_with_current_aliases(to_sender_signed_transaction(data, &keypair))
            .unwrap();

        // Validate and acquire locks (MFP voting phase)
        authority
            .handle_vote_transaction(&epoch_store, transaction.tx().clone())
            .unwrap();
        transactions.push(transaction);
    }
    transactions
}

/// Fixture: creates a transaction using the specified gas and input objects.
pub async fn test_user_transaction(
    authority: &AuthorityState,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    gas_object: Object,
    input_objects: Vec<Object>,
) -> VerifiedTransactionWithAliases {
    let epoch_store = authority.load_epoch_store_one_call_per_task();
    let rgp = epoch_store.reference_gas_price();

    // Object digest may be different in genesis than originally generated.
    let gas_object = authority.get_object(&gas_object.id()).unwrap();
    let mut input_objs = vec![];
    for obj in input_objects {
        input_objs.push(authority.get_object(&obj.id()).unwrap());
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
        .verify_transaction_with_current_aliases(to_sender_signed_transaction(data, keypair))
        .unwrap()
}

#[tokio::test]
async fn submit_transaction_to_consensus_adapter() {
    telemetry_subscribers::init_for_testing();

    // Initialize an authority with a (owned) gas object and a shared object; then
    // make a test transaction.
    let mut objects = test_gas_objects();
    let shared_object = Object::shared_for_testing();
    objects.push(shared_object.clone());
    let state = init_state_with_objects(objects).await;
    let transaction = test_user_transactions(&state, shared_object)
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

    // Submit the transaction using UserTransactionV2 message.
    // Note that consensus may drop some transactions (so we may need to resubmit them).
    let consensus_tx =
        ConsensusTransaction::new_user_transaction_v2_message(&state.name, transaction.into());
    let waiter = adapter
        .submit(
            consensus_tx.clone(),
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
    // make test transactions.
    let mut objects = test_gas_objects();
    let shared_object = Object::shared_for_testing();
    objects.push(shared_object.clone());
    let state = init_state_with_objects(objects).await;
    let transactions = test_user_transactions(&state, shared_object).await;
    let epoch_store = state.epoch_store_for_testing();

    // Mark the first two transactions to be "executed via checkpoint" and the other two to appear via consensus output.
    assert_eq!(transactions.len(), 4);

    let mut process_via_checkpoint = HashSet::new();
    process_via_checkpoint.insert(*transactions[0].tx().digest());
    process_via_checkpoint.insert(*transactions[1].tx().digest());

    // Make a new consensus adapter instance.
    let adapter = make_consensus_adapter_for_test(
        state.clone(),
        process_via_checkpoint,
        false,
        vec![with_block_status(BlockStatus::Sequenced(BlockRef::MIN))],
    );

    // Submit the transactions using UserTransactionV2 messages.
    // Note that consensus may drop some transactions (so we may need to resubmit them).
    let consensus_transactions = transactions
        .into_iter()
        .map(|tx| ConsensusTransaction::new_user_transaction_v2_message(&state.name, tx.into()))
        .collect::<Vec<_>>();

    let waiter = adapter
        .submit_batch(
            &consensus_transactions,
            Some(&epoch_store.get_reconfig_state_read_lock_guard()),
            &epoch_store,
            None,
            None,
        )
        .unwrap();
    waiter.await.unwrap();
}

#[sim_test]
async fn system_message_bypasses_exhausted_submit_semaphore() {
    telemetry_subscribers::init_for_testing();

    let mut objects = test_gas_objects();
    let shared_object = Object::shared_for_testing();
    objects.push(shared_object.clone());
    let state = init_state_with_objects(objects).await;
    let user_tx = test_user_transactions(&state, shared_object)
        .await
        .pop()
        .unwrap();
    let epoch_store = state.epoch_store_for_testing();

    // Zero submit permits: any transaction that must acquire the submit semaphore
    // blocks indefinitely. The one mock block status is reserved for the system
    // message, which is the only submission expected to reach consensus.
    let adapter = make_consensus_adapter_for_test_with_submit_limit(
        state.clone(),
        HashSet::new(),
        false,
        vec![with_block_status(BlockStatus::Sequenced(BlockRef::MIN))],
        0,
    );

    // A user transaction cannot acquire a permit, so it blocks.
    let user_consensus_tx =
        ConsensusTransaction::new_user_transaction_v2_message(&state.name, user_tx.into());
    let mut user_waiter = adapter
        .submit(
            user_consensus_tx,
            Some(&epoch_store.get_reconfig_state_read_lock_guard()),
            &epoch_store,
            None,
            None,
        )
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(2), &mut user_waiter)
            .await
            .is_err(),
        "user transaction must block on the exhausted submit semaphore"
    );
    // The submission is parked on the semaphore forever; abort it so it does not
    // outlive the test.
    user_waiter.abort();

    // The system message bypasses the semaphore and completes.
    let end_of_publish = ConsensusTransaction::new_end_of_publish(state.name);
    let system_waiter = adapter
        .submit(end_of_publish, None, &epoch_store, None, None)
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), system_waiter)
        .await
        .expect("system message must not block on an exhausted submit semaphore")
        .unwrap();
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
        let transactions = vec![ConsensusTransaction::new_checkpoint_signature_message_v2(
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

    let consensus_position = rx_consensus_position.await.unwrap().unwrap();
    assert_eq!(
        consensus_position,
        vec![ConsensusPosition {
            epoch: epoch_store.epoch(),
            block: BlockRef::MIN,
            index: PING_TRANSACTION_INDEX,
        }]
    );
}

// When a transaction that is requesting a consensus position (e.g. mfp) has already been
// processed via consensus output, the adapter must NOT resubmit it. Instead it reports that
// the transaction is already processing, so the caller can return a retriable error to the
// client rather than a (now meaningless) consensus position.
#[tokio::test]
async fn submit_already_processed_transaction_returns_processing_error() {
    telemetry_subscribers::init_for_testing();

    // Initialize an authority with gas and a shared object, then build a user transaction.
    let mut objects = test_gas_objects();
    let shared_object = Object::shared_for_testing();
    objects.push(shared_object.clone());
    let state = init_state_with_objects(objects).await;
    let transaction = test_user_transactions(&state, shared_object)
        .await
        .into_iter()
        .next()
        .unwrap();
    let epoch_store = state.epoch_store_for_testing();
    let tx_digest = *transaction.tx().digest();

    // Simulate the transaction already appearing in consensus output before this submission.
    epoch_store.test_insert_user_signature(tx_digest, vec![]);

    // A mock block status receiver is provided so that, if the adapter were to (incorrectly)
    // submit, it could complete and return positions, surfacing the regression as a failure.
    let adapter = make_consensus_adapter_for_test(
        state.clone(),
        HashSet::new(),
        false,
        vec![with_block_status(BlockStatus::Sequenced(BlockRef::MIN))],
    );

    let consensus_tx =
        ConsensusTransaction::new_user_transaction_v2_message(&state.name, transaction.into());

    let result = adapter
        .submit_and_get_positions(vec![consensus_tx], &epoch_store, None)
        .await;

    match result {
        Err(err) => assert!(
            matches!(
                err.as_inner(),
                SuiErrorKind::TransactionProcessing { digest, .. } if *digest == tx_digest
            ),
            "expected TransactionProcessing error, got: {err}"
        ),
        Ok(positions) => {
            panic!("expected TransactionProcessing error, got positions: {positions:?}")
        }
    }
}

fn test_block_ref(round: u32) -> BlockRef {
    BlockRef {
        author: consensus_config::AuthorityIndex::ZERO,
        round,
        digest: Default::default(),
    }
}

fn test_position(
    epoch: sui_types::base_types::EpochId,
    round: u32,
    index: u16,
) -> ConsensusPosition {
    ConsensusPosition {
        epoch,
        block: test_block_ref(round),
        index,
    }
}

/// Test client that sequences every submission but never fires processed notifications,
/// mimicking the real consensus path for a transaction that reaches consensus output
/// without being finalized (voted rejected, or dropped post-consensus): the commit
/// handler assigns the position a terminal status but never records the digest as
/// consensus-processed. The nth submission lands in a block with `block_rounds[n]`
/// (clamped to the last entry), making resubmissions observable as distinct positions.
struct SequenceOnlyClient {
    block_rounds: Vec<u32>,
    submission_count: std::sync::atomic::AtomicUsize,
}

impl SequenceOnlyClient {
    fn new(block_rounds: Vec<u32>) -> Self {
        Self {
            block_rounds,
            submission_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn submissions(&self) -> usize {
        self.submission_count
            .load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl ConsensusClient for SequenceOnlyClient {
    async fn submit(
        &self,
        transactions: &[ConsensusTransaction],
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<(Vec<ConsensusPosition>, BlockStatusReceiver)> {
        let n = self
            .submission_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let round = self.block_rounds[n.min(self.block_rounds.len() - 1)];
        let positions = (0..transactions.len())
            .map(|index| test_position(epoch_store.epoch(), round, index as u16))
            .collect();
        Ok((
            positions,
            with_block_status(BlockStatus::Sequenced(test_block_ref(round))),
        ))
    }
}

async fn wait_for_condition(mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !condition() {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("condition not reached in time");
}

// A transaction that consensus sequences but votes to reject never gets its digest
// recorded as consensus-processed. The adapter must settle the submission via the
// per-position Rejected status instead of holding its inflight slot and submit
// semaphore permit until end of epoch.
#[tokio::test]
async fn adapter_settles_submission_on_rejected_position_status() {
    telemetry_subscribers::init_for_testing();

    let mut objects = test_gas_objects();
    let shared_object = Object::shared_for_testing();
    objects.push(shared_object.clone());
    let state = init_state_with_objects(objects).await;
    let transaction = test_user_transactions(&state, shared_object)
        .await
        .pop()
        .unwrap();
    let epoch_store = state.epoch_store_for_testing();

    let client = Arc::new(SequenceOnlyClient::new(vec![1]));
    let adapter = make_consensus_adapter_with_client_for_test(&state, client, 100);

    let consensus_tx =
        ConsensusTransaction::new_user_transaction_v2_message(&state.name, transaction.into());
    let waiter = adapter
        .submit(
            consensus_tx,
            Some(&epoch_store.get_reconfig_state_read_lock_guard()),
            &epoch_store,
            None,
            None,
        )
        .unwrap();

    // The commit handler's side of the contract: the sequenced position receives a
    // terminal status.
    epoch_store
        .consensus_tx_status_cache
        .set_transaction_status(
            test_position(epoch_store.epoch(), 1, 0),
            ConsensusTxStatus::Rejected,
        );

    tokio::time::timeout(Duration::from_secs(10), waiter)
        .await
        .expect("adapter task should settle when its position is rejected")
        .unwrap();
    assert_eq!(adapter.num_inflight_transactions(), 0);
}

// A soft bundle settles once every position has a terminal status, even when the
// outcomes are mixed and some transactions are never recorded as processed.
#[tokio::test]
async fn adapter_settles_soft_bundle_on_mixed_position_statuses() {
    telemetry_subscribers::init_for_testing();

    let mut objects = test_gas_objects();
    let shared_object = Object::shared_for_testing();
    objects.push(shared_object.clone());
    let state = init_state_with_objects(objects).await;
    let transactions = test_user_transactions(&state, shared_object).await;
    let epoch_store = state.epoch_store_for_testing();

    let client = Arc::new(SequenceOnlyClient::new(vec![1]));
    let adapter = make_consensus_adapter_with_client_for_test(&state, client, 100);

    let consensus_transactions = transactions
        .into_iter()
        .take(2)
        .map(|tx| ConsensusTransaction::new_user_transaction_v2_message(&state.name, tx.into()))
        .collect::<Vec<_>>();
    let waiter = adapter
        .submit_batch(
            &consensus_transactions,
            Some(&epoch_store.get_reconfig_state_read_lock_guard()),
            &epoch_store,
            None,
            None,
        )
        .unwrap();

    epoch_store
        .consensus_tx_status_cache
        .set_transaction_status(
            test_position(epoch_store.epoch(), 1, 0),
            ConsensusTxStatus::Finalized,
        );
    epoch_store
        .consensus_tx_status_cache
        .set_transaction_status(
            test_position(epoch_store.epoch(), 1, 1),
            ConsensusTxStatus::Rejected,
        );

    tokio::time::timeout(Duration::from_secs(10), waiter)
        .await
        .expect("adapter task should settle when all bundle positions have terminal statuses")
        .unwrap();
    assert_eq!(adapter.num_inflight_transactions(), 0);
}

// If a position expires from the status cache before its status is read (the validator
// lagged more than the retention window behind consensus), the adapter must handle it
// like garbage collection and resubmit rather than waiting forever.
#[tokio::test]
async fn adapter_resubmits_on_expired_position_status() {
    telemetry_subscribers::init_for_testing();

    let mut objects = test_gas_objects();
    let shared_object = Object::shared_for_testing();
    objects.push(shared_object.clone());
    let state = init_state_with_objects(objects).await;
    let transaction = test_user_transactions(&state, shared_object)
        .await
        .pop()
        .unwrap();
    let epoch_store = state.epoch_store_for_testing();

    // First submission lands in block round 1; the resubmission in a much later round
    // immune to the expiry below.
    let client = Arc::new(SequenceOnlyClient::new(vec![1, 10_000]));
    let adapter = make_consensus_adapter_with_client_for_test(&state, client.clone(), 100);

    let consensus_tx =
        ConsensusTransaction::new_user_transaction_v2_message(&state.name, transaction.into());
    let waiter = adapter
        .submit(
            consensus_tx,
            Some(&epoch_store.get_reconfig_state_read_lock_guard()),
            &epoch_store,
            None,
            None,
        )
        .unwrap();

    let client_clone = client.clone();
    wait_for_condition(move || client_clone.submissions() == 1).await;

    // Expire the first position without ever setting its status. The expiry watch
    // publishes the previous committed leader round, so two updates are needed for
    // round 1 to fall out of the retention window.
    epoch_store
        .consensus_tx_status_cache
        .update_last_committed_leader_round(CONSENSUS_STATUS_RETENTION_ROUNDS + 100);
    epoch_store
        .consensus_tx_status_cache
        .update_last_committed_leader_round(CONSENSUS_STATUS_RETENTION_ROUNDS + 101);

    // The adapter treats the expired position like garbage collection and resubmits.
    let client_clone = client.clone();
    wait_for_condition(move || client_clone.submissions() == 2).await;

    // Settle the resubmitted position.
    epoch_store
        .consensus_tx_status_cache
        .set_transaction_status(
            test_position(epoch_store.epoch(), 10_000, 0),
            ConsensusTxStatus::Finalized,
        );

    tokio::time::timeout(Duration::from_secs(10), waiter)
        .await
        .expect("adapter task should settle after resubmission")
        .unwrap();
    assert_eq!(adapter.num_inflight_transactions(), 0);
}
