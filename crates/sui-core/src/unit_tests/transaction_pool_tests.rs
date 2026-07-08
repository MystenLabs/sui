// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::authority::AuthorityState;
use crate::authority::authority_tests::init_state_with_objects;
use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::consensus_adapter::consensus_tests::{test_gas_objects, test_user_transactions};
use consensus_types::block::PING_TRANSACTION_INDEX;
use fastcrypto::traits::KeyPair;
use sui_types::crypto::{AuthorityKeyPair, get_key_pair};
use sui_types::object::Object;

async fn make_pool(
    capacity: usize,
    max_pending: usize,
) -> (Arc<SuiTransactionPool>, Arc<AuthorityState>) {
    let state = TestAuthorityBuilder::new().build().await;
    let epoch_store = state.epoch_store_for_testing().clone();
    let pool = SuiTransactionPool::new(
        epoch_store,
        capacity,
        max_pending,
        TransactionPoolMetrics::new_for_tests(),
    );
    (pool, state)
}

/// A distinct consensus transaction per call (EndOfPublish keyed by a fresh authority
/// name). Used as a stand-in payload for both user- and system-classed entries; entry
/// class comes from the submission API, not the payload.
fn make_tx() -> ConsensusTransaction {
    let (_, key_pair): (_, AuthorityKeyPair) = get_key_pair();
    ConsensusTransaction::new_end_of_publish(key_pair.public().into())
}

fn tx_bytes(tx: &ConsensusTransaction) -> Vec<u8> {
    bcs::to_bytes(tx).unwrap()
}

fn block_ref(round: Round) -> BlockRef {
    BlockRef {
        author: consensus_config::AuthorityIndex::ZERO,
        round,
        digest: Default::default(),
    }
}

fn position(round: Round, index: TransactionIndex) -> ConsensusPosition {
    ConsensusPosition {
        epoch: 0,
        block: block_ref(round),
        index,
    }
}

fn processed_key(tx: &ConsensusTransaction) -> SequencedConsensusTransactionKey {
    SequencedConsensusTransactionKey::External(tx.key())
}

#[test]
fn test_pool_key_ordering() {
    let key = |class, price, seq| PoolKey {
        class,
        price: Reverse(price),
        seq,
    };
    // System entries sort before any user entry, regardless of price or age.
    assert!(key(PriorityClass::System, 0, 100) < key(PriorityClass::User, u64::MAX, 0));
    // Higher gas price first.
    assert!(key(PriorityClass::User, 300, 7) < key(PriorityClass::User, 200, 1));
    // FIFO within a price level.
    assert!(key(PriorityClass::User, 300, 1) < key(PriorityClass::User, 300, 2));
}

#[tokio::test]
async fn test_take_order_system_first_then_gas_price_fifo() {
    let (pool, _state) = make_pool(100, 100).await;
    let (u100, u300a, u200, u300b, sys) = (make_tx(), make_tx(), make_tx(), make_tx(), make_tx());
    pool.submit_user_transactions(100, vec![u100.clone()], None)
        .unwrap();
    pool.submit_user_transactions(300, vec![u300a.clone()], None)
        .unwrap();
    pool.submit_user_transactions(200, vec![u200.clone()], None)
        .unwrap();
    pool.submit_user_transactions(300, vec![u300b.clone()], None)
        .unwrap();
    pool.submit_system_transaction(sys.clone(), None).unwrap();

    let (transactions, _ack, limit) = pool.take(100, usize::MAX);
    assert_eq!(limit, LimitReached::AllTransactionsIncluded);
    let taken: Vec<_> = transactions.iter().map(|t| t.data().to_vec()).collect();
    let expected: Vec<_> = [&sys, &u300a, &u300b, &u200, &u100]
        .iter()
        .map(|t| tx_bytes(t))
        .collect();
    assert_eq!(taken, expected);
}

#[tokio::test]
async fn test_take_respects_max_count_and_bundle_atomicity() {
    let (pool, _state) = make_pool(100, 100).await;
    let bundle: Vec<_> = (0..3).map(|_| make_tx()).collect();
    pool.submit_user_transactions(200, bundle, None).unwrap();
    pool.submit_user_transactions(100, vec![make_tx()], None)
        .unwrap();

    // The 3-tx bundle doesn't fit in 2 slots; take stops rather than splitting the
    // bundle or skipping ahead past it.
    let (transactions, _ack, limit) = pool.take(2, usize::MAX);
    assert!(transactions.is_empty());
    assert_eq!(limit, LimitReached::MaxNumOfTransactions);

    let (transactions, _ack, limit) = pool.take(4, usize::MAX);
    assert_eq!(transactions.len(), 4);
    assert_eq!(limit, LimitReached::AllTransactionsIncluded);
}

#[tokio::test]
async fn test_take_respects_max_bytes() {
    let (pool, _state) = make_pool(100, 100).await;
    let (a, b) = (make_tx(), make_tx());
    let a_size = tx_bytes(&a).len();
    pool.submit_user_transactions(200, vec![a], None).unwrap();
    pool.submit_user_transactions(100, vec![b], None).unwrap();

    let (transactions, _ack, limit) = pool.take(100, a_size);
    assert_eq!(transactions.len(), 1);
    assert_eq!(limit, LimitReached::MaxBytes);
}

#[tokio::test]
async fn test_take_user_inflight_budget_and_system_exemption() {
    // Inflight budget of 2 user transactions.
    let (pool, _state) = make_pool(100, 2).await;
    for price in [300, 200, 100] {
        pool.submit_user_transactions(price, vec![make_tx()], None)
            .unwrap();
    }

    let (transactions, ack, limit) = pool.take(100, usize::MAX);
    assert_eq!(transactions.len(), 2);
    assert_eq!(limit, LimitReached::MaxNumOfTransactions);
    ack(block_ref(1));
    assert_eq!(pool.inflight_user_transactions(), 2);

    // Budget exhausted: no user transactions are taken, but system entries are exempt.
    pool.submit_system_transaction(make_tx(), None).unwrap();
    let (transactions, _ack, _) = pool.take(100, usize::MAX);
    assert_eq!(transactions.len(), 1);

    // Settling the proposed transactions frees the budget.
    pool.note_statuses(&[
        (position(1, 0), ConsensusTxStatus::Finalized),
        (position(1, 1), ConsensusTxStatus::Finalized),
    ]);
    assert_eq!(pool.inflight_user_transactions(), 0);
    let (transactions, _ack, _) = pool.take(100, usize::MAX);
    assert_eq!(transactions.len(), 1);
}

#[tokio::test]
async fn test_ack_assigns_positions_and_notifies_waiters() {
    let (pool, _state) = make_pool(100, 100).await;
    let (rx_single, _) = pool
        .submit_user_transactions(300, vec![make_tx()], None)
        .unwrap();
    let (rx_bundle, _) = pool
        .submit_user_transactions(200, vec![make_tx(), make_tx()], None)
        .unwrap();

    let (_transactions, ack, _) = pool.take(100, usize::MAX);
    ack(block_ref(5));

    assert_eq!(rx_single.await.unwrap().unwrap(), vec![position(5, 0)]);
    assert_eq!(
        rx_bundle.await.unwrap().unwrap(),
        vec![position(5, 1), position(5, 2)]
    );
}

#[tokio::test]
async fn test_dropped_ack_returns_entries_to_pending() {
    let (pool, _state) = make_pool(100, 100).await;
    let (mut rx, _) = pool
        .submit_user_transactions(100, vec![make_tx()], None)
        .unwrap();

    let (transactions, ack, _) = pool.take(100, usize::MAX);
    assert_eq!(transactions.len(), 1);
    assert_eq!(pool.pending_user_transactions(), 0);
    drop(ack);

    // The entry is back in pending with its waiter intact, and can be taken again.
    assert_eq!(pool.pending_user_transactions(), 1);
    assert!(rx.try_recv().is_err());
    let (transactions, ack, _) = pool.take(100, usize::MAX);
    assert_eq!(transactions.len(), 1);
    ack(block_ref(1));
    assert_eq!(rx.await.unwrap().unwrap(), vec![position(1, 0)]);
}

#[tokio::test]
async fn test_eviction_when_full() {
    let (pool, _state) = make_pool(2, 100).await;
    let (rx_low, _) = pool
        .submit_user_transactions(100, vec![make_tx()], None)
        .unwrap();
    pool.submit_user_transactions(200, vec![make_tx()], None)
        .unwrap();

    // Evicts the lowest-priced entry; its waiter gets an explicit outbid error
    // carrying the evicting price.
    pool.submit_user_transactions(300, vec![make_tx()], None)
        .unwrap();
    assert_eq!(pool.pending_user_transactions(), 2);
    let err = rx_low.await.unwrap().unwrap_err();
    assert!(matches!(
        err.as_inner(),
        SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion { min_gas_price: 300 }
    ));
}

#[tokio::test]
async fn test_rejection_when_full_and_price_too_low() {
    let (pool, _state) = make_pool(2, 100).await;
    pool.submit_user_transactions(100, vec![make_tx()], None)
        .unwrap();
    pool.submit_user_transactions(200, vec![make_tx()], None)
        .unwrap();

    // Strictly lower price: rejected with the current minimum.
    let err = pool
        .submit_user_transactions(50, vec![make_tx()], None)
        .unwrap_err();
    assert!(matches!(
        err.as_inner(),
        SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion { min_gas_price: 100 }
    ));

    // Equal price does not evict.
    let err = pool
        .submit_user_transactions(100, vec![make_tx()], None)
        .unwrap_err();
    assert!(matches!(
        err.as_inner(),
        SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion { min_gas_price: 100 }
    ));
    assert_eq!(pool.pending_user_transactions(), 2);
}

#[tokio::test]
async fn test_gasless_evicted_first() {
    let (pool, _state) = make_pool(2, 100).await;
    let (rx_gasless, _) = pool
        .submit_user_transactions(0, vec![make_tx()], None)
        .unwrap();
    pool.submit_user_transactions(1000, vec![make_tx()], None)
        .unwrap();

    pool.submit_user_transactions(2000, vec![make_tx()], None)
        .unwrap();
    let err = rx_gasless.await.unwrap().unwrap_err();
    assert!(matches!(
        err.as_inner(),
        SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion { .. }
    ));
}

#[tokio::test]
async fn test_system_entries_never_evicted_and_capacity_exempt() {
    let (pool, _state) = make_pool(1, 100).await;
    pool.submit_system_transaction(make_tx(), None).unwrap();
    pool.submit_system_transaction(make_tx(), None).unwrap();
    // Capacity 1 counts user transactions only.
    pool.submit_user_transactions(100, vec![make_tx()], None)
        .unwrap();
    assert_eq!(pool.pending_user_transactions(), 1);

    // A higher-priced user transaction can only evict the user entry, never the
    // system entries.
    pool.submit_user_transactions(200, vec![make_tx()], None)
        .unwrap();
    let (transactions, _ack, _) = pool.take(100, usize::MAX);
    assert_eq!(transactions.len(), 3);
}

#[tokio::test]
async fn test_duplicate_coalesces_onto_entry() {
    let (pool, _state) = make_pool(100, 100).await;
    let tx = make_tx();
    let (rx1, newly1) = pool
        .submit_user_transactions(100, vec![tx.clone()], None)
        .unwrap();
    let (rx2, newly2) = pool
        .submit_user_transactions(100, vec![tx.clone()], None)
        .unwrap();
    assert!(newly1);
    assert!(!newly2);
    // One entry, not two.
    assert_eq!(pool.pending_user_transactions(), 1);

    let (transactions, ack, _) = pool.take(100, usize::MAX);
    assert_eq!(transactions.len(), 1);
    ack(block_ref(2));

    // Both waiters share the same position.
    assert_eq!(rx1.await.unwrap().unwrap(), vec![position(2, 0)]);
    assert_eq!(rx2.await.unwrap().unwrap(), vec![position(2, 0)]);
}

#[tokio::test]
async fn test_duplicate_of_proposed_entry_gets_position_immediately() {
    let (pool, _state) = make_pool(100, 100).await;
    let tx = make_tx();
    pool.submit_user_transactions(100, vec![tx.clone()], None)
        .unwrap();
    let (_transactions, ack, _) = pool.take(100, usize::MAX);
    ack(block_ref(3));

    let (rx, newly) = pool
        .submit_user_transactions(100, vec![tx.clone()], None)
        .unwrap();
    assert!(!newly);
    assert_eq!(rx.await.unwrap().unwrap(), vec![position(3, 0)]);
}

#[tokio::test]
async fn test_partial_bundle_overlap_admitted_as_separate_entry() {
    let (pool, _state) = make_pool(100, 100).await;
    let (a, b) = (make_tx(), make_tx());
    let (_rx1, newly1) = pool
        .submit_user_transactions(100, vec![a.clone()], None)
        .unwrap();
    assert!(newly1);
    // A bundle sharing a key with an existing entry cannot coalesce (key sets don't
    // match exactly); it is admitted alongside, flagged as a duplicate.
    let (_rx2, newly2) = pool
        .submit_user_transactions(100, vec![a.clone(), b.clone()], None)
        .unwrap();
    assert!(!newly2);
    assert_eq!(pool.pending_user_transactions(), 3);
}

#[tokio::test]
async fn test_note_processed_settles_pending_with_retriable_error() {
    let (pool, _state) = make_pool(100, 100).await;
    let tx = make_tx();
    let (rx, _) = pool
        .submit_user_transactions(100, vec![tx.clone()], None)
        .unwrap();

    // The transaction was observed processed (e.g. committed from another validator's
    // block) before ever being proposed here: the waiter is answered early and the
    // entry never reaches consensus.
    pool.note_processed(std::iter::once(&processed_key(&tx)));
    let err = rx.await.unwrap().unwrap_err();
    assert!(matches!(
        err.as_inner(),
        SuiErrorKind::TransactionProcessing { .. }
    ));
    assert_eq!(pool.pending_user_transactions(), 0);
    let (transactions, _ack, _) = pool.take(100, usize::MAX);
    assert!(transactions.is_empty());
}

#[tokio::test]
async fn test_note_processed_settles_proposed_entry() {
    let (pool, _state) = make_pool(100, 100).await;
    let tx = make_tx();
    let (rx, _) = pool
        .submit_user_transactions(100, vec![tx.clone()], None)
        .unwrap();
    let (_transactions, ack, _) = pool.take(100, usize::MAX);
    ack(block_ref(1));
    assert_eq!(rx.await.unwrap().unwrap(), vec![position(1, 0)]);
    assert_eq!(pool.inflight_user_transactions(), 1);

    pool.note_processed(std::iter::once(&processed_key(&tx)));
    assert_eq!(pool.inflight_user_transactions(), 0);
}

#[tokio::test]
async fn test_note_statuses_settles_only_matching_position() {
    let (pool, _state) = make_pool(100, 100).await;
    pool.submit_user_transactions(100, vec![make_tx()], None)
        .unwrap();
    let (_transactions, ack, _) = pool.take(100, usize::MAX);
    ack(block_ref(7));

    // A status for the same index in a different block (e.g. the same digest
    // submitted by another validator) must not settle this entry.
    pool.note_statuses(&[(position(8, 0), ConsensusTxStatus::Finalized)]);
    assert_eq!(pool.inflight_user_transactions(), 1);

    // Vote-rejected transactions settle solely via their position status.
    pool.note_statuses(&[(position(7, 0), ConsensusTxStatus::Rejected)]);
    assert_eq!(pool.inflight_user_transactions(), 0);
}

#[tokio::test]
async fn test_bundle_settles_when_all_transactions_settle() {
    let (pool, _state) = make_pool(100, 100).await;
    pool.submit_user_transactions(100, vec![make_tx(), make_tx()], None)
        .unwrap();
    let (_transactions, ack, _) = pool.take(100, usize::MAX);
    ack(block_ref(1));

    // Partial settlement keeps the entry (and its inflight accounting) alive.
    pool.note_statuses(&[(position(1, 0), ConsensusTxStatus::Finalized)]);
    assert_eq!(pool.inflight_user_transactions(), 2);
    pool.note_statuses(&[(position(1, 1), ConsensusTxStatus::Dropped)]);
    assert_eq!(pool.inflight_user_transactions(), 0);
}

#[tokio::test]
async fn test_gc_requeues_system_and_drops_user_entries() {
    let (pool, _state) = make_pool(100, 100).await;
    let sys = make_tx();
    pool.submit_system_transaction(sys.clone(), None).unwrap();
    pool.submit_user_transactions(100, vec![make_tx()], None)
        .unwrap();
    let (transactions, ack, _) = pool.take(100, usize::MAX);
    assert_eq!(transactions.len(), 2);
    ack(block_ref(3));

    // The block was garbage collected: the system entry requeues, the user entry is
    // dropped (its client resolves via WaitForEffects position expiry and resubmits).
    pool.notify_committed(vec![], 3);
    assert_eq!(pool.inflight_user_transactions(), 0);
    let (transactions, _ack, _) = pool.take(100, usize::MAX);
    let taken: Vec<_> = transactions.iter().map(|t| t.data().to_vec()).collect();
    assert_eq!(taken, vec![tx_bytes(&sys)]);
}

#[tokio::test]
async fn test_sequenced_block_is_not_garbage_collected() {
    let (pool, _state) = make_pool(100, 100).await;
    pool.submit_user_transactions(100, vec![make_tx()], None)
        .unwrap();
    let (_transactions, ack, _) = pool.take(100, usize::MAX);
    ack(block_ref(3));

    // The block committed: entries are retained awaiting their status callbacks even
    // though the GC round has passed the block's round.
    pool.notify_committed(vec![block_ref(3)], 3);
    assert_eq!(pool.inflight_user_transactions(), 1);

    pool.note_statuses(&[(position(3, 0), ConsensusTxStatus::Finalized)]);
    assert_eq!(pool.inflight_user_transactions(), 0);
}

#[tokio::test]
async fn test_best_effort_expires_instead_of_being_taken() {
    let (pool, _state) = make_pool(100, 100).await;
    pool.submit_system_transaction(make_tx(), Some(Instant::now() - Duration::from_millis(1)))
        .unwrap();

    let (transactions, _ack, _) = pool.take(100, usize::MAX);
    assert!(transactions.is_empty());
    // The expired entry was settled, not retained.
    let (transactions, _ack, _) = pool.take(100, usize::MAX);
    assert!(transactions.is_empty());
}

#[tokio::test]
async fn test_best_effort_dropped_on_garbage_collection() {
    let (pool, _state) = make_pool(100, 100).await;
    pool.submit_system_transaction(make_tx(), Some(Instant::now() + Duration::from_secs(60)))
        .unwrap();
    let (transactions, ack, _) = pool.take(100, usize::MAX);
    assert_eq!(transactions.len(), 1);
    ack(block_ref(3));

    // Unlike a regular system entry, a best-effort entry does not requeue after GC.
    pool.notify_committed(vec![], 3);
    let (transactions, _ack, _) = pool.take(100, usize::MAX);
    assert!(transactions.is_empty());
}

#[tokio::test]
async fn test_shutdown_fails_waiters_and_rejects_submissions() {
    let (pool, _state) = make_pool(100, 100).await;
    let (rx_pending, _) = pool
        .submit_user_transactions(100, vec![make_tx()], None)
        .unwrap();
    let (rx_proposed, _) = pool
        .submit_user_transactions(200, vec![make_tx()], None)
        .unwrap();
    let tx3 = make_tx();
    let (_transactions, ack, _) = pool.take(1, usize::MAX);
    ack(block_ref(1));
    // The proposed waiter already has its position.
    assert!(rx_proposed.await.unwrap().is_ok());

    pool.shutdown();
    let err = rx_pending.await.unwrap().unwrap_err();
    assert!(matches!(
        err.as_inner(),
        SuiErrorKind::ValidatorHaltedAtEpochEnd
    ));
    assert_eq!(pool.pending_user_transactions(), 0);
    assert_eq!(pool.inflight_user_transactions(), 0);

    let err = pool
        .submit_user_transactions(100, vec![tx3], None)
        .unwrap_err();
    assert!(matches!(
        err.as_inner(),
        SuiErrorKind::ValidatorHaltedAtEpochEnd
    ));
    let (transactions, _ack, _) = pool.take(100, usize::MAX);
    assert!(transactions.is_empty());
}

#[tokio::test]
async fn test_ping_gets_position_at_next_block() {
    let (pool, _state) = make_pool(100, 100).await;
    let (rx, _) = pool.submit_user_transactions(0, vec![], None).unwrap();

    let (transactions, ack, _) = pool.take(100, usize::MAX);
    // Pings contribute no transactions to the block.
    assert!(transactions.is_empty());
    ack(block_ref(2));

    let positions = rx.await.unwrap().unwrap();
    assert_eq!(positions, vec![ConsensusPosition::ping(0, block_ref(2))]);
    assert_eq!(positions[0].index, PING_TRANSACTION_INDEX);
}

#[tokio::test]
async fn test_context_rotate_shuts_down_old_pool() {
    let state = TestAuthorityBuilder::new().build().await;
    let epoch_store = state.epoch_store_for_testing().clone();
    let context = TransactionPoolContext::new(
        CheckpointStore::new_for_tests(),
        state.name,
        100,
        TransactionPoolMetrics::new_for_tests(),
        epoch_store.clone(),
    );
    let old_pool = context.current_pool();
    let (rx, _) = old_pool
        .submit_user_transactions(100, vec![make_tx()], None)
        .unwrap();

    let new_pool = context.rotate_for_epoch(epoch_store);
    assert!(Arc::ptr_eq(&context.current_pool(), &new_pool));
    assert!(!Arc::ptr_eq(&old_pool, &new_pool));
    let err = rx.await.unwrap().unwrap_err();
    assert!(matches!(
        err.as_inner(),
        SuiErrorKind::ValidatorHaltedAtEpochEnd
    ));
}

#[tokio::test]
async fn test_note_executed_in_checkpoint_settles_user_transaction() {
    let mut objects = test_gas_objects();
    let shared_object = Object::shared_for_testing();
    objects.push(shared_object.clone());
    let state = init_state_with_objects(objects).await;
    let transaction = test_user_transactions(&state, shared_object)
        .await
        .pop()
        .unwrap();
    let digest = *transaction.tx().digest();
    let consensus_tx =
        ConsensusTransaction::new_user_transaction_v2_message(&state.name, transaction.into());

    let epoch_store = state.epoch_store_for_testing().clone();
    let pool = SuiTransactionPool::new(
        epoch_store,
        100,
        100,
        TransactionPoolMetrics::new_for_tests(),
    );
    let (rx, _) = pool
        .submit_user_transactions(1000, vec![consensus_tx], None)
        .unwrap();

    // The transaction executed via a certified checkpoint (e.g. state sync) — the
    // waiter is answered with a retriable processing error carrying the digest.
    pool.note_executed_in_checkpoint(&[digest]);
    let err = rx.await.unwrap().unwrap_err();
    assert!(matches!(
        err.as_inner(),
        SuiErrorKind::TransactionProcessing { digest: d, .. } if *d == digest
    ));
    assert_eq!(pool.pending_user_transactions(), 0);
}

#[tokio::test]
async fn test_context_suppresses_already_processed_submission() {
    let mut objects = test_gas_objects();
    let shared_object = Object::shared_for_testing();
    objects.push(shared_object.clone());
    let state = init_state_with_objects(objects).await;
    let transaction = test_user_transactions(&state, shared_object)
        .await
        .pop()
        .unwrap();
    let digest = *transaction.tx().digest();
    let consensus_tx =
        ConsensusTransaction::new_user_transaction_v2_message(&state.name, transaction.into());

    let epoch_store = state.epoch_store_for_testing().clone();
    // Simulate the transaction already appearing in consensus output before this
    // submission.
    epoch_store.test_insert_user_signature(digest, vec![]);

    let context = TransactionPoolContext::new(
        CheckpointStore::new_for_tests(),
        state.name,
        100,
        TransactionPoolMetrics::new_for_tests(),
        epoch_store.clone(),
    );
    let err = context
        .submit_for_positions(1000, vec![consensus_tx], &epoch_store, None)
        .unwrap_err();
    assert!(matches!(
        err.as_inner(),
        SuiErrorKind::TransactionProcessing { digest: d, .. } if *d == digest
    ));
    // Nothing entered the pool.
    assert_eq!(context.current_pool().pending_user_transactions(), 0);
}

#[tokio::test]
async fn test_context_end_to_end_submit_and_get_positions() {
    let state = TestAuthorityBuilder::new().build().await;
    let epoch_store = state.epoch_store_for_testing().clone();
    let context = TransactionPoolContext::new(
        CheckpointStore::new_for_tests(),
        state.name,
        100,
        TransactionPoolMetrics::new_for_tests(),
        epoch_store.clone(),
    );

    // Drive the consensus side: propose a block as soon as the pool has content.
    let pool = context.current_pool();
    let driver = tokio::spawn(async move {
        loop {
            let (transactions, ack, _) = pool.take(100, usize::MAX);
            if !transactions.is_empty() {
                ack(block_ref(9));
                return;
            }
            drop(ack);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    });

    let positions = context
        .submit_and_get_positions(vec![make_tx()], 100, &epoch_store, None)
        .await
        .unwrap();
    assert_eq!(positions, vec![position(9, 0)]);
    driver.await.unwrap();
}

#[tokio::test]
async fn test_submit_to_consensus_dedups_system_message() {
    let (pool, state) = make_pool(100, 100).await;
    let epoch_store = state.epoch_store_for_testing().clone();
    let context = TransactionPoolContext::new(
        CheckpointStore::new_for_tests(),
        state.name,
        100,
        TransactionPoolMetrics::new_for_tests(),
        epoch_store.clone(),
    );
    drop(pool);

    let tx = make_tx();
    context
        .submit_to_consensus(std::slice::from_ref(&tx), &epoch_store)
        .unwrap();
    // Same key coalesces instead of adding a second entry.
    context.submit_to_consensus(&[tx], &epoch_store).unwrap();
    let (transactions, _ack, _) = context.current_pool().take(100, usize::MAX);
    assert_eq!(transactions.len(), 1);
}
