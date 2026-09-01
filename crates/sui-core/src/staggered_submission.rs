// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Staggered consensus submission for transactions that do not restrict their proposers.
//!
//! A transaction without a usable allowed-proposers list can be submitted to every validator
//! at once, amplifying its consensus cost while paying gas once. Rejecting such transactions
//! outright would break existing clients, so instead — when duplication is detected on the
//! network — each validator derives a deterministic rank for itself from a stake-weighted
//! permutation of the committee seeded by the transaction, and delays its own submission by
//! that rank. The first `free_slots` validators in the derived
//! order submit immediately (mirroring the proposer set an explicit list would grant), and
//! every later validator waits long enough that an earlier copy can commit first, at which
//! point the pending submission is dropped (see the processed-notify race in
//! `ConsensusAdapter::submit_and_wait_inner`). Duplication is thereby bounded by construction
//! instead of detected after its bandwidth is already spent.
//!
//! The rank is derived from the gas object ids rather than the transaction digest: digest
//! derivation could be ground by re-signing trivial variants (nonce, budget) until each
//! variant's first slot lands on a different validator, while all such variants necessarily
//! share their gas objects. Transactions paying from an address balance have no gas objects
//! and fall back to the digest; the residual grindability there is accepted, since the
//! variants still share the sender's address balance and stay visible to sender-level
//! accounting.
//!
//! Staggering is local policy: it is not enforced at block verification, and a byzantine
//! validator can ignore it. The attack it closes is submitter-driven amplification through
//! honest validators.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use fastcrypto::hash::HashFunction;
use parking_lot::RwLock;
use rand::SeedableRng as _;
use rand::rngs::StdRng;
use sui_types::base_types::ObjectID;
use sui_types::committee::{Committee, CommitteeTrait as _, EpochId};
use sui_types::crypto::DefaultHash;
use sui_types::digests::TransactionDigest;
use sui_types::transaction::{MAX_UNPAID_ALLOWED_PROPOSERS, Transaction, TransactionDataAPI as _};

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;

/// Parameters of the staggering schedule.
#[derive(Debug, Clone)]
pub struct StaggerParams {
    /// Delay between consecutive ranks beyond the free slots.
    pub step: Duration,
    /// Upper bound on any submission delay: all ranks past `free_slots + max_delay/step`
    /// fire at `max_delay`, so a submitter never waits longer than this regardless of
    /// committee size.
    pub max_delay: Duration,
    /// Number of leading ranks that submit without delay. Raised per transaction by paid
    /// SIP-45 amplification.
    pub free_slots: u64,
}

impl Default for StaggerParams {
    fn default() -> Self {
        Self {
            step: Duration::from_millis(250),
            max_delay: Duration::from_secs(2),
            free_slots: MAX_UNPAID_ALLOWED_PROPOSERS,
        }
    }
}

/// Decides whether this validator should delay submitting a given user transaction to
/// consensus, and by how much. Inactive by default; activation is currently manual (node
/// config / operator), and will be driven by a commit-derived duplication signal so that all
/// honest validators flip in lockstep.
pub struct StaggeredSubmission {
    active: AtomicBool,
    params: RwLock<StaggerParams>,
}

impl StaggeredSubmission {
    pub fn new() -> Self {
        Self {
            active: AtomicBool::new(false),
            params: RwLock::new(StaggerParams::default()),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    pub fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::Relaxed);
    }

    #[cfg(test)]
    pub fn set_params_for_testing(&self, params: StaggerParams) {
        *self.params.write() = params;
    }

    /// The delay this validator applies before submitting `tx` to consensus, or `None` to
    /// submit immediately. Applies only to transactions that could have named their
    /// proposers and did not: staggering exists to remove the amplification value of
    /// omitting the list, so a transaction that restricts its proposers — or one on a
    /// network where restricting them is not yet possible — is never delayed.
    pub fn submission_delay(
        &self,
        tx: &Transaction,
        epoch_store: &AuthorityPerEpochStore,
    ) -> Option<Duration> {
        if !self.is_active() {
            return None;
        }
        if !epoch_store.protocol_config().allowed_proposers() {
            return None;
        }
        let epoch = epoch_store.epoch();
        let tx_data = tx.data().transaction_data();
        if tx_data.expiration().restricts_proposers(epoch) {
            return None;
        }
        // A non-member has no rank in the order; it also has no consensus to submit to.
        let own_index = epoch_store.own_committee_index()?;

        let gas_payment: Vec<ObjectID> = tx_data
            .gas_data()
            .payment
            .iter()
            .map(|(id, _, _)| *id)
            .collect();
        let seed = stagger_seed(&gas_payment, tx.digest(), epoch);
        let rank = stagger_rank(&seed, epoch_store.committee(), own_index);

        // SIP-45: a raised gas price pays for amplification, which maps here to that many
        // immediate slots. Matches the sizing an explicit proposer set is allowed.
        let paid_amplification =
            tx_data.gas_data().price / epoch_store.reference_gas_price().max(1);

        compute_delay(&self.params.read(), rank, paid_amplification)
    }
}

/// The delay for `rank`, given that `paid_amplification` immediate slots were paid for
/// beyond the default free slots. The first rank past the free slots waits one step.
fn compute_delay(params: &StaggerParams, rank: u32, paid_amplification: u64) -> Option<Duration> {
    let free_slots = params.free_slots.max(paid_amplification);
    if (rank as u64) < free_slots {
        return None;
    }
    let steps = (rank as u64 - free_slots + 1).min(u32::MAX as u64) as u32;
    Some(params.step.saturating_mul(steps).min(params.max_delay))
}

impl Default for StaggeredSubmission {
    fn default() -> Self {
        Self::new()
    }
}

/// Seed of the rank permutation for one logical transaction.
///
/// Gas object ids are sorted before hashing: their order inside the transaction is
/// signer-controlled, and must not offer another grinding dimension.
fn stagger_seed(
    gas_payment: &[ObjectID],
    tx_digest: &TransactionDigest,
    epoch: EpochId,
) -> [u8; 32] {
    let mut hasher = DefaultHash::new();
    hasher.update(b"staggered_submission");
    hasher.update(epoch.to_le_bytes());
    if gas_payment.is_empty() {
        hasher.update(tx_digest.inner());
    } else {
        let mut ids: Vec<ObjectID> = gas_payment.to_vec();
        ids.sort();
        for id in ids {
            hasher.update(id);
        }
    }
    hasher.finalize().into()
}

/// This validator's position in a stake-weighted permutation of the committee derived from
/// `seed`: members are drawn without replacement with probability proportional to voting
/// power, the same primitive as `Committee::shuffle_by_stake_from_tx_digest` and the
/// consensus leader schedule. Weighting by stake makes early-slot honesty track honest
/// *stake* (the BFT assumption) rather than validator count, makes splitting stake across
/// seats buy no extra slot-0 share, and lands the implied submission load on validators in
/// proportion to their stake.
fn stagger_rank(seed: &[u8; 32], committee: &Committee, own_index: u32) -> u32 {
    let own_name = committee
        .authority_by_index(own_index)
        .expect("own_index is a committee member");
    let mut rng = StdRng::from_seed(*seed);
    let shuffled = committee.shuffle_by_stake_with_rng(None, None, &mut rng);
    shuffled
        .iter()
        .position(|name| name == own_name)
        .expect("every committee member appears in the shuffle") as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_seed(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn ranks(committee: &Committee, seed: &[u8; 32]) -> Vec<u32> {
        (0..committee.num_members() as u32)
            .map(|index| stagger_rank(seed, committee, index))
            .collect()
    }

    #[test]
    fn rank_is_a_permutation() {
        for seed_byte in 0..3u8 {
            let seed = test_seed(seed_byte);
            for committee_size in [1usize, 4, 7, 100] {
                let (committee, _) = Committee::new_simple_test_committee_of_size(committee_size);
                let mut ranks = ranks(&committee, &seed);
                ranks.sort();
                assert_eq!(ranks, (0..committee_size as u32).collect::<Vec<_>>());
            }
        }
    }

    #[test]
    fn rank_is_deterministic_and_seed_sensitive() {
        let (committee, _) = Committee::new_simple_test_committee_of_size(100);
        assert_eq!(
            stagger_rank(&test_seed(1), &committee, 42),
            stagger_rank(&test_seed(1), &committee, 42)
        );
        // Different seeds must not produce the same permutation. Compare the full
        // permutations (a single rank can collide legitimately).
        assert_ne!(
            ranks(&committee, &test_seed(1)),
            ranks(&committee, &test_seed(2))
        );
    }

    #[test]
    fn rank_is_stake_weighted() {
        // One member holds 85% of the voting power; it must take slot 0 for the large
        // majority of seeds. The bound is loose (the exact count is deterministic but
        // depends on the rand version) while a uniform permutation would put the heavy
        // member first only ~25% of the time.
        let (committee, _) =
            Committee::new_simple_test_committee_with_normalized_voting_power(vec![
                8500, 500, 500, 500,
            ]);
        let heavy_index = (0..4)
            .find(|&index| committee.weight(committee.authority_by_index(index).unwrap()) == 8500)
            .unwrap();
        let heavy_first = (0..=u8::MAX)
            .filter(|&byte| stagger_rank(&test_seed(byte), &committee, heavy_index) == 0)
            .count();
        assert!(
            heavy_first > 150,
            "heavy member ranked first only {heavy_first}/256 times"
        );
    }

    #[test]
    fn seed_ignores_gas_payment_order() {
        let id_a = ObjectID::from_single_byte(1);
        let id_b = ObjectID::from_single_byte(2);
        let digest = TransactionDigest::default();
        assert_eq!(
            stagger_seed(&[id_a, id_b], &digest, 5),
            stagger_seed(&[id_b, id_a], &digest, 5),
        );
    }

    #[test]
    fn seed_depends_on_gas_objects_not_digest() {
        let id = ObjectID::from_single_byte(1);
        let digest_a = TransactionDigest::random();
        let digest_b = TransactionDigest::random();
        // With gas objects, the digest is irrelevant: trivially re-signed variants of the
        // same logical transaction land on the same schedule.
        assert_eq!(
            stagger_seed(&[id], &digest_a, 5),
            stagger_seed(&[id], &digest_b, 5),
        );
        // Without gas objects (address-balance gas), the digest is the fallback.
        assert_ne!(
            stagger_seed(&[], &digest_a, 5),
            stagger_seed(&[], &digest_b, 5),
        );
    }

    #[test]
    fn delay_schedule() {
        let params = StaggerParams {
            step: Duration::from_millis(250),
            max_delay: Duration::from_secs(2),
            free_slots: 3,
        };
        // Free slots submit immediately; the first held rank waits one step.
        assert_eq!(compute_delay(&params, 0, 1), None);
        assert_eq!(compute_delay(&params, 2, 1), None);
        assert_eq!(
            compute_delay(&params, 3, 1),
            Some(Duration::from_millis(250))
        );
        assert_eq!(
            compute_delay(&params, 4, 1),
            Some(Duration::from_millis(500))
        );
        // The delay is capped: distant ranks all fire at max_delay.
        assert_eq!(compute_delay(&params, 11, 1), Some(Duration::from_secs(2)));
        assert_eq!(compute_delay(&params, 120, 1), Some(Duration::from_secs(2)));
        // Paid amplification widens the free slots, and never narrows them.
        assert_eq!(compute_delay(&params, 4, 5), None);
        assert_eq!(
            compute_delay(&params, 5, 5),
            Some(Duration::from_millis(250))
        );
        assert_eq!(compute_delay(&params, 2, 1), None);
    }

    #[test]
    fn seed_depends_on_epoch() {
        let id = ObjectID::from_single_byte(1);
        let digest = TransactionDigest::default();
        assert_ne!(
            stagger_seed(&[id], &digest, 5),
            stagger_seed(&[id], &digest, 6),
        );
    }
}

#[cfg(test)]
mod adapter_tests {
    use std::num::NonZeroUsize;
    use std::sync::Arc;
    use std::time::Instant;

    use consensus_core::BlockStatus;
    use consensus_types::block::BlockRef;
    use parking_lot::Mutex;
    use sui_protocol_config::ProtocolConfig;
    use sui_types::base_types::{ObjectID, SuiAddress};
    use sui_types::crypto::{AccountKeyPair, deterministic_random_account_key};
    use sui_types::error::SuiResult;
    use sui_types::messages_consensus::{ConsensusPosition, ConsensusTransaction};
    use sui_types::object::Object;
    use sui_types::transaction::VerifiedTransactionWithAliases;

    use super::*;
    use crate::authority::AuthorityState;
    use crate::authority::test_authority_builder::TestAuthorityBuilder;
    use crate::consensus_adapter::consensus_tests::test_user_transaction;
    use crate::consensus_adapter::{BlockStatusReceiver, ConsensusClient};
    use crate::consensus_handler::SequencedConsensusTransaction;
    use crate::consensus_test_utils::make_consensus_adapter_with_client_for_test;
    use crate::mock_consensus::with_block_status;

    struct RecordingClient {
        submit_times: Mutex<Vec<Instant>>,
    }

    #[async_trait::async_trait]
    impl ConsensusClient for RecordingClient {
        async fn submit(
            &self,
            transactions: &[ConsensusTransaction],
            epoch_store: &Arc<AuthorityPerEpochStore>,
        ) -> SuiResult<(Vec<ConsensusPosition>, BlockStatusReceiver)> {
            self.submit_times.lock().push(Instant::now());
            // Mark the transactions processed so the adapter's processed-notify race
            // resolves and the submission task completes.
            let keys: Vec<_> = transactions
                .iter()
                .map(|t| SequencedConsensusTransaction::new_test(t.clone()).key())
                .collect();
            epoch_store.process_notifications(keys.iter());
            let positions = (0..transactions.len())
                .map(|index| ConsensusPosition {
                    epoch: epoch_store.epoch(),
                    index: index as u16,
                    block: BlockRef::MIN,
                })
                .collect();
            Ok((
                positions,
                with_block_status(BlockStatus::Sequenced(BlockRef::MIN)),
            ))
        }
    }

    pub(super) const COMMITTEE_SIZE: u32 = 4;

    /// A 4-validator state, plus gas objects ground so that this validator's derived rank
    /// for a transaction paying with `staggered_gas` is past slot 0 (i.e. the transaction
    /// is held when staggering is active, since a reference-price transaction has exactly
    /// one immediate slot), while a transaction paying with `immediate_gas` is not.
    pub(super) async fn setup() -> (
        Arc<AuthorityState>,
        SuiAddress,
        AccountKeyPair,
        Object,
        Object,
    ) {
        let (sender, keypair) = deterministic_random_account_key();
        let candidates: Vec<Object> = (0..32)
            .map(|_| Object::with_id_owner_for_testing(ObjectID::random(), sender))
            .collect();

        let network_config =
            sui_swarm_config::network_config_builder::ConfigBuilder::new_with_temp_dir()
                .committee_size(NonZeroUsize::new(COMMITTEE_SIZE as usize).unwrap())
                .with_objects(candidates.clone())
                .build();
        let state = TestAuthorityBuilder::new()
            .with_network_config(&network_config, 0)
            .build()
            .await;

        let epoch_store = state.epoch_store_for_testing();
        let epoch = epoch_store.epoch();
        let own_index = epoch_store.own_committee_index().unwrap();
        let rank_of = |object: &Object| {
            let seed = stagger_seed(&[object.id()], &TransactionDigest::default(), epoch);
            stagger_rank(&seed, epoch_store.committee(), own_index)
        };
        // 32 candidates make both picks overwhelmingly likely (miss odds ~(1/4)^32 and
        // ~(3/4)^32 respectively).
        let staggered_gas = candidates
            .iter()
            .find(|o| rank_of(o) >= 1)
            .expect("no candidate gas object ranks past slot 0")
            .clone();
        let immediate_gas = candidates
            .iter()
            .find(|o| rank_of(o) == 0)
            .expect("no candidate gas object ranks first")
            .clone();

        (state, sender, keypair, staggered_gas, immediate_gas)
    }

    async fn submit_and_wait(
        adapter: &Arc<crate::consensus_adapter::ConsensusAdapter>,
        state: &Arc<AuthorityState>,
        transaction: VerifiedTransactionWithAliases,
    ) -> Duration {
        let epoch_store = state.epoch_store_for_testing();
        let consensus_tx =
            ConsensusTransaction::new_user_transaction_v2_message(&state.name, transaction.into());
        let start = Instant::now();
        let waiter = {
            let guard = epoch_store.get_reconfig_state_read_lock_guard();
            adapter
                .submit(consensus_tx, Some(&guard), &epoch_store, None, None)
                .unwrap()
        };
        waiter.await.unwrap();
        start.elapsed()
    }

    #[tokio::test]
    async fn staggered_submission_delays_held_ranks_only() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut c| {
            c.set_allowed_proposers_for_testing(true);
            c
        });
        let (state, sender, keypair, staggered_gas, immediate_gas) = setup().await;

        let client = Arc::new(RecordingClient {
            submit_times: Mutex::new(vec![]),
        });
        let adapter = make_consensus_adapter_with_client_for_test(&state, client.clone(), 100);

        const STEP: Duration = Duration::from_millis(1000);
        let epoch_store = state.epoch_store_for_testing();
        let staggered = epoch_store.staggered_submission();
        staggered.set_params_for_testing(StaggerParams {
            step: STEP,
            max_delay: Duration::from_secs(10),
            free_slots: 1,
        });
        staggered.set_active(true);

        // Rank 0 for its gas object: submits immediately even while staggering is active.
        let tx = test_user_transaction(&state, sender, &keypair, immediate_gas, vec![]).await;
        let elapsed = submit_and_wait(&adapter, &state, tx).await;
        assert_eq!(client.submit_times.lock().len(), 1);
        assert!(elapsed < STEP, "immediate slot was delayed by {elapsed:?}");

        // Rank past slot 0: held for at least one step.
        let tx =
            test_user_transaction(&state, sender, &keypair, staggered_gas.clone(), vec![]).await;
        let elapsed = submit_and_wait(&adapter, &state, tx).await;
        assert_eq!(client.submit_times.lock().len(), 2);
        assert!(
            elapsed >= STEP,
            "held rank submitted after only {elapsed:?}"
        );

        // Inactive: the same held rank submits immediately. A new gas coin was created by
        // the previous transfer, so reuse of the original gas object is fine: the previous
        // transaction is already processed, but this one has a fresh digest.
        staggered.set_active(false);
        let gas = state.get_object(&staggered_gas.id()).unwrap();
        let tx = test_user_transaction(&state, sender, &keypair, gas, vec![]).await;
        let elapsed = submit_and_wait(&adapter, &state, tx).await;
        assert_eq!(client.submit_times.lock().len(), 3);
        assert!(elapsed < STEP, "inactive staggering delayed by {elapsed:?}");
    }

    #[tokio::test]
    async fn held_submission_dropped_when_processed() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut c| {
            c.set_allowed_proposers_for_testing(true);
            c
        });
        let (state, sender, keypair, staggered_gas, _) = setup().await;
        let epoch_store = state.epoch_store_for_testing();

        let client = Arc::new(RecordingClient {
            submit_times: Mutex::new(vec![]),
        });
        let adapter = make_consensus_adapter_with_client_for_test(&state, client.clone(), 100);

        // A hold long enough that the test can only pass by cancellation.
        let staggered = epoch_store.staggered_submission();
        staggered.set_params_for_testing(StaggerParams {
            step: Duration::from_secs(60),
            max_delay: Duration::from_secs(60),
            free_slots: 1,
        });
        staggered.set_active(true);

        let tx = test_user_transaction(&state, sender, &keypair, staggered_gas, vec![]).await;
        let consensus_tx =
            ConsensusTransaction::new_user_transaction_v2_message(&state.name, tx.into());
        let key = SequencedConsensusTransaction::new_test(consensus_tx.clone()).key();

        let waiter = adapter
            .submit(
                consensus_tx,
                Some(&epoch_store.get_reconfig_state_read_lock_guard()),
                &epoch_store,
                None,
                None,
            )
            .unwrap();

        // While the transaction is held, another validator's copy commits.
        tokio::time::sleep(Duration::from_millis(200)).await;
        epoch_store.process_notifications(std::iter::once(&key));

        waiter.await.unwrap();
        assert!(
            client.submit_times.lock().is_empty(),
            "held submission should have been dropped, not submitted"
        );
    }
}

/// Staggering through the pull-based transaction pool: entries are held inside the pool
/// and skipped by `take()` until eligible, rather than delayed before submission.
#[cfg(test)]
mod pool_tests {
    use std::sync::Arc;

    use consensus_config::AuthorityIndex;
    use consensus_core::TransactionPool as _;
    use consensus_types::block::{BlockDigest, BlockRef};
    use sui_protocol_config::ProtocolConfig;
    use sui_types::messages_consensus::ConsensusTransaction;
    use sui_types::object::Object;

    use super::adapter_tests::setup;
    use super::*;
    use crate::admission_queue::AdmissionQueueMetrics;
    use crate::authority::AuthorityState;
    use crate::consensus_adapter::consensus_tests::test_user_transaction;
    use crate::consensus_handler::SequencedConsensusTransaction;
    use crate::consensus_transaction_pool::ConsensusTransactionPool;

    fn pool_for(state: &Arc<AuthorityState>) -> Arc<ConsensusTransactionPool> {
        Arc::new(ConsensusTransactionPool::new_for_tests(
            state.epoch_store_for_testing().clone(),
            10,
            Arc::new(AdmissionQueueMetrics::new_for_tests()),
        ))
    }

    async fn insert(
        pool: &ConsensusTransactionPool,
        state: &Arc<AuthorityState>,
        sender: sui_types::base_types::SuiAddress,
        keypair: &sui_types::crypto::AccountKeyPair,
        gas: Object,
    ) -> (
        crate::consensus_transaction_pool::PositionReceiver,
        ConsensusTransaction,
    ) {
        let tx = test_user_transaction(state, sender, keypair, gas, vec![]).await;
        let consensus_tx =
            ConsensusTransaction::new_user_transaction_v2_message(&state.name, tx.into());
        let (receiver, newly_inserted) = pool
            .try_insert(pool.epoch(), 1, vec![consensus_tx.clone()])
            .unwrap();
        assert!(newly_inserted);
        (receiver, consensus_tx)
    }

    #[tokio::test]
    async fn staggered_entry_held_until_eligible() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut c| {
            c.set_allowed_proposers_for_testing(true);
            c
        });
        let (state, sender, keypair, staggered_gas, _) = setup().await;
        let epoch_store = state.epoch_store_for_testing();
        let pool = pool_for(&state);

        let staggered = epoch_store.staggered_submission();
        staggered.set_params_for_testing(StaggerParams {
            step: Duration::from_millis(250),
            max_delay: Duration::from_millis(500),
            free_slots: 1,
        });
        staggered.set_active(true);

        let (_receiver, _) = insert(&pool, &state, sender, &keypair, staggered_gas).await;

        // Held: not proposed, but still queued (occupying pool capacity).
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert!(transactions.is_empty(), "held entry was proposed");
        drop(ack);
        assert_eq!(pool.queue_depth("user"), 1);

        // Past the (capped) delay the entry is proposed as usual.
        tokio::time::sleep(Duration::from_millis(600)).await;
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert_eq!(transactions.len(), 1, "eligible entry was not proposed");
        // The dropped ack requeues the entry; close() resolves it before the pool drops.
        drop(ack);
        pool.close();
    }

    #[tokio::test]
    async fn immediate_rank_and_inactive_not_held() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut c| {
            c.set_allowed_proposers_for_testing(true);
            c
        });
        let (state, sender, keypair, staggered_gas, immediate_gas) = setup().await;
        let epoch_store = state.epoch_store_for_testing();
        let pool = pool_for(&state);

        let staggered = epoch_store.staggered_submission();
        staggered.set_params_for_testing(StaggerParams {
            step: Duration::from_secs(60),
            max_delay: Duration::from_secs(60),
            free_slots: 1,
        });
        staggered.set_active(true);

        // Rank 0 for its gas object: proposed on the next take while staggering is active.
        let (_receiver, _) = insert(&pool, &state, sender, &keypair, immediate_gas).await;
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert_eq!(transactions.len(), 1, "immediate slot was held");
        ack(BlockRef::new(
            1,
            AuthorityIndex::new_for_test(0),
            BlockDigest::MIN,
        ));

        // Inactive: a rank that would be held is proposed immediately.
        staggered.set_active(false);
        let (_receiver, _) = insert(&pool, &state, sender, &keypair, staggered_gas).await;
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert_eq!(transactions.len(), 1, "inactive staggering held an entry");
        // The dropped ack requeues the entry; close() resolves it before the pool drops.
        drop(ack);
        pool.close();
    }

    #[tokio::test]
    async fn held_entry_resolved_when_copy_processed() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut c| {
            c.set_allowed_proposers_for_testing(true);
            c
        });
        let (state, sender, keypair, staggered_gas, _) = setup().await;
        let epoch_store = state.epoch_store_for_testing();
        let pool = pool_for(&state);

        // A hold long enough that the entry can only leave the pool by resolution.
        let staggered = epoch_store.staggered_submission();
        staggered.set_params_for_testing(StaggerParams {
            step: Duration::from_secs(60),
            max_delay: Duration::from_secs(60),
            free_slots: 1,
        });
        staggered.set_active(true);

        let (receiver, consensus_tx) = insert(&pool, &state, sender, &keypair, staggered_gas).await;
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert!(transactions.is_empty(), "held entry was proposed");
        drop(ack);

        // While the entry is held, another validator's copy is processed. The next
        // take resolves it without proposing it or waiting out the hold.
        let key = SequencedConsensusTransaction::new_test(consensus_tx).key();
        epoch_store.process_notifications(std::iter::once(&key));
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert!(transactions.is_empty(), "processed entry was proposed");
        drop(ack);
        assert_eq!(pool.queue_depth("user"), 0);
        assert!(
            receiver.await.unwrap().is_err(),
            "held entry should resolve as already processed"
        );
    }

    #[tokio::test]
    async fn disarming_releases_held_entries() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut c| {
            c.set_allowed_proposers_for_testing(true);
            c
        });
        let (state, sender, keypair, staggered_gas, _) = setup().await;
        let epoch_store = state.epoch_store_for_testing();
        let pool = pool_for(&state);

        let staggered = epoch_store.staggered_submission();
        staggered.set_params_for_testing(StaggerParams {
            step: Duration::from_secs(60),
            max_delay: Duration::from_secs(60),
            free_slots: 1,
        });
        staggered.set_active(true);

        let (_receiver, _) = insert(&pool, &state, sender, &keypair, staggered_gas).await;
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert!(transactions.is_empty(), "held entry was proposed");
        drop(ack);

        // Disarming releases entries stamped while the mode was armed, without
        // waiting out their remaining delay.
        staggered.set_active(false);
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert_eq!(transactions.len(), 1, "disarming did not release the entry");
        drop(ack);
        pool.close();
    }

    #[tokio::test]
    async fn soft_bundle_not_held() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut c| {
            c.set_allowed_proposers_for_testing(true);
            c
        });
        let (state, sender, keypair, staggered_gas, immediate_gas) = setup().await;
        let epoch_store = state.epoch_store_for_testing();
        let pool = pool_for(&state);

        let staggered = epoch_store.staggered_submission();
        staggered.set_params_for_testing(StaggerParams {
            step: Duration::from_secs(60),
            max_delay: Duration::from_secs(60),
            free_slots: 1,
        });
        staggered.set_active(true);

        // A soft bundle containing a transaction that would be held on its own is
        // exempt from staggering and proposed on the next take.
        let tx_a = test_user_transaction(&state, sender, &keypair, staggered_gas, vec![]).await;
        let tx_b = test_user_transaction(&state, sender, &keypair, immediate_gas, vec![]).await;
        let bundle = vec![
            ConsensusTransaction::new_user_transaction_v2_message(&state.name, tx_a.into()),
            ConsensusTransaction::new_user_transaction_v2_message(&state.name, tx_b.into()),
        ];
        let (_receiver, newly_inserted) = pool.try_insert(pool.epoch(), 1, bundle).unwrap();
        assert!(newly_inserted);
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert_eq!(transactions.len(), 2, "soft bundle was held");
        // The dropped ack requeues the bundle; close() resolves it before the pool drops.
        drop(ack);
        pool.close();
    }
}
