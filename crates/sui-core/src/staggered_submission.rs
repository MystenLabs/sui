// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Staggered consensus submission for transactions that do not restrict their proposers.
//!
//! A transaction without a usable allowed-proposers list can be submitted to every validator
//! at once, amplifying its consensus cost while paying gas once. Rejecting such transactions
//! outright would break existing clients, so instead — when duplication is detected on the
//! network — each validator derives a deterministic slot for itself from a stake-weighted
//! permutation of the committee seeded by the transaction, and delays its own submission
//! according to that slot. The first `free_slots` validators in the derived order submit
//! immediately (mirroring the proposer set an explicit list would grant), and
//! every later validator waits long enough that an earlier copy can commit first.
//! Duplication is thereby bounded by construction instead of detected after its bandwidth is already spent.
//!
//! The slot is derived from the gas object ids rather than the transaction digest: digest
//! derivation could be ground by re-signing trivial variants (nonce, budget) until each
//! variant's first slot lands on a different validator, while all such variants necessarily
//! share their gas objects. Transactions paying from an address balance have no gas objects
//! and fall back to the digest; the residual grindability there is accepted, since the
//! variants still share the sender's address balance and stay visible to sender-level
//! accounting.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use fastcrypto::hash::HashFunction;
use mysten_common::debug_fatal;
use parking_lot::RwLock;
use rand::SeedableRng as _;
use rand::rngs::StdRng;
use sui_types::base_types::ObjectID;
use sui_types::committee::{Committee, CommitteeTrait as _, EpochId};
use sui_types::crypto::DefaultHash;
use sui_types::digests::TransactionDigest;
use sui_types::transaction::{MAX_UNPAID_ALLOWED_PROPOSERS, Transaction, TransactionDataAPI as _};

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;

/// Default delay between consecutive slots beyond the free slots.
const DEFAULT_STAGGER_STEP: Duration = Duration::from_millis(350);
/// Default upper bound on any submission delay.
const DEFAULT_STAGGER_MAX_DELAY: Duration = Duration::from_secs(5);

/// Parameters of the staggering schedule.
#[derive(Debug, Clone)]
pub struct StaggerParams {
    /// Delay between consecutive slots beyond the free slots.
    pub step: Duration,
    /// Upper bound on any submission delay: all slots past `free_slots + max_delay/step`
    /// fire at `max_delay`, so a submitter never waits longer than this regardless of
    /// committee size.
    pub max_delay: Duration,
    /// Number of leading slots that submit without delay.
    pub free_slots: u64,
}

impl Default for StaggerParams {
    fn default() -> Self {
        Self {
            step: DEFAULT_STAGGER_STEP,
            max_delay: DEFAULT_STAGGER_MAX_DELAY,
            free_slots: MAX_UNPAID_ALLOWED_PROPOSERS,
        }
    }
}

/// Decides whether this validator should delay submitting a given user transaction to
/// consensus, and by how much. Inactive by default;
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

    /// The delay this validator applies before submitting a group of user transactions —
    /// a single transaction or a soft bundle — to consensus, or `None` to submit
    /// immediately. Staggering exists to remove the amplification value of omitting the
    /// allowed-proposers list, and a group is as amplifiable as its least restricted
    /// member (soft bundles are external fan-out too, and would otherwise be a wrapper
    /// that bypasses the hold): the group is held if any member could have named its
    /// proposers and did not, and only those members' identity feeds the slot seed, so a
    /// restricted companion cannot be used to re-roll the schedule.
    pub fn submission_delay(
        &self,
        txs: &[&Transaction],
        epoch_store: &AuthorityPerEpochStore,
    ) -> Option<Duration> {
        if !self.is_active() {
            return None;
        }
        if !epoch_store.protocol_config().allowed_proposers() {
            return None;
        }
        let epoch = epoch_store.epoch();
        let unrestricted: Vec<_> = txs
            .iter()
            .map(|tx| (tx.data().transaction_data(), tx.digest()))
            .filter(|(tx_data, _)| !tx_data.expiration().restricts_proposers(epoch))
            .collect();
        if unrestricted.is_empty() {
            return None;
        }
        // A non-member has no slot in the order; it also has no consensus to submit to.
        let own_index = epoch_store.own_committee_index()?;

        let gas_payment: Vec<ObjectID> = unrestricted
            .iter()
            .flat_map(|(tx_data, _)| tx_data.gas_data().payment.iter().map(|(id, _, _)| *id))
            .collect();
        let digests: Vec<&TransactionDigest> =
            unrestricted.iter().map(|(_, digest)| *digest).collect();
        let seed = stagger_seed(&gas_payment, &digests, epoch);
        let slot = stagger_slot(&seed, epoch_store.committee(), own_index);

        // SIP-45: a raised gas price pays for amplification, which maps here to that many
        // immediate slots. Matches the sizing an explicit proposer set is allowed. Soft
        // bundles enforce a uniform gas price at admission; min() is the conservative
        // choice should that ever change.
        let paid_amplification = unrestricted
            .iter()
            .map(|(tx_data, _)| tx_data.gas_data().price)
            .min()
            .expect("unrestricted is non-empty")
            / epoch_store.reference_gas_price().max(1);

        compute_delay(&self.params.read(), slot, paid_amplification)
    }
}

/// The delay for `slot`, given that `paid_amplification` immediate slots were paid for
/// beyond the default free slots. The first slot past the free slots waits one step.
fn compute_delay(params: &StaggerParams, slot: u64, paid_amplification: u64) -> Option<Duration> {
    let free_slots = params.free_slots.max(paid_amplification);
    if slot < free_slots {
        return None;
    }
    let steps = slot - free_slots + 1;
    if steps > u32::MAX as u64 {
        // Unreachable: the slot is bounded by the committee size.
        debug_fatal!("stagger step count {steps} overflows u32");
    }
    let steps = steps.min(u32::MAX as u64) as u32;
    Some(params.step.saturating_mul(steps).min(params.max_delay))
}

impl Default for StaggeredSubmission {
    fn default() -> Self {
        Self::new()
    }
}

/// Seed of the slot permutation for one logical transaction or soft bundle.
///
/// Gas object ids (and, in the fallback, digests) are sorted before hashing: their order
/// inside a transaction or bundle is signer-controlled, and must not offer another
/// grinding dimension.
fn stagger_seed(
    gas_payment: &[ObjectID],
    tx_digests: &[&TransactionDigest],
    epoch: EpochId,
) -> [u8; 32] {
    let mut hasher = DefaultHash::new();
    hasher.update(b"staggered_submission");
    hasher.update(epoch.to_le_bytes());
    if gas_payment.is_empty() {
        let mut digests: Vec<&TransactionDigest> = tx_digests.to_vec();
        digests.sort();
        for digest in digests {
            hasher.update(digest.inner());
        }
    } else {
        let mut ids: Vec<ObjectID> = gas_payment.to_vec();
        ids.sort();
        for id in ids {
            hasher.update(id);
        }
    }
    hasher.finalize().into()
}

/// This validator's slot in a stake-weighted permutation of the committee derived from
/// `seed`: members are drawn without replacement with probability proportional to voting
/// power, the same primitive as `Committee::shuffle_by_stake_from_tx_digest` and the
/// consensus leader schedule. Weighting by stake makes early-slot honesty track honest
/// *stake* (the BFT assumption) rather than validator count, makes splitting stake across
/// seats buy no extra slot-0 share, and lands the implied submission load on validators in
/// proportion to their stake.
fn stagger_slot(seed: &[u8; 32], committee: &Committee, own_index: u32) -> u64 {
    let own_name = committee
        .authority_by_index(own_index)
        .expect("own_index is a committee member");
    let mut rng = StdRng::from_seed(*seed);
    let shuffled = committee.shuffle_by_stake_with_rng(None, None, &mut rng);
    shuffled
        .iter()
        .position(|name| name == own_name)
        .expect("every committee member appears in the shuffle") as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_seed(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn slots(committee: &Committee, seed: &[u8; 32]) -> Vec<u64> {
        (0..committee.num_members() as u32)
            .map(|index| stagger_slot(seed, committee, index))
            .collect()
    }

    #[test]
    fn slot_is_a_permutation() {
        for seed_byte in 0..3u8 {
            let seed = test_seed(seed_byte);
            for committee_size in [1usize, 4, 7, 100] {
                let (committee, _) = Committee::new_simple_test_committee_of_size(committee_size);
                let mut slots = slots(&committee, &seed);
                slots.sort();
                assert_eq!(slots, (0..committee_size as u64).collect::<Vec<_>>());
            }
        }
    }

    #[test]
    fn slot_is_deterministic_and_seed_sensitive() {
        let (committee, _) = Committee::new_simple_test_committee_of_size(100);
        assert_eq!(
            stagger_slot(&test_seed(1), &committee, 42),
            stagger_slot(&test_seed(1), &committee, 42)
        );
        // Different seeds must not produce the same permutation. Compare the full
        // permutations (a single slot can collide legitimately).
        assert_ne!(
            slots(&committee, &test_seed(1)),
            slots(&committee, &test_seed(2))
        );
    }

    #[test]
    fn seed_ignores_gas_payment_order() {
        let id_a = ObjectID::from_single_byte(1);
        let id_b = ObjectID::from_single_byte(2);
        let digest = TransactionDigest::default();
        assert_eq!(
            stagger_seed(&[id_a, id_b], &[&digest], 5),
            stagger_seed(&[id_b, id_a], &[&digest], 5),
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
            stagger_seed(&[id], &[&digest_a], 5),
            stagger_seed(&[id], &[&digest_b], 5),
        );
        // Without gas objects (address-balance gas), the digests are the fallback.
        assert_ne!(
            stagger_seed(&[], &[&digest_a], 5),
            stagger_seed(&[], &[&digest_b], 5),
        );
        // The fallback ignores member order, which is submitter-controlled in a bundle.
        assert_eq!(
            stagger_seed(&[], &[&digest_a, &digest_b], 5),
            stagger_seed(&[], &[&digest_b, &digest_a], 5),
        );
    }

    #[test]
    fn delay_schedule() {
        let params = StaggerParams {
            step: Duration::from_millis(250),
            max_delay: Duration::from_secs(2),
            free_slots: 3,
        };
        // Free slots submit immediately; the first held slot waits one step.
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
        // The delay is capped: distant slots all fire at max_delay.
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
            stagger_seed(&[id], &[&digest], 5),
            stagger_seed(&[id], &[&digest], 6),
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

    /// A 4-validator state, plus gas objects ground so that this validator's derived slot
    /// for a transaction paying with `staggered_gas` is past slot 0 (i.e. the transaction
    /// is held when staggering is active, since a reference-price transaction has exactly
    /// one immediate slot), while a transaction paying with `immediate_gas` is not. The
    /// remaining candidate objects (all owned by `sender`) are returned for tests that
    /// need to grind their own picks.
    pub(super) async fn setup() -> (
        Arc<AuthorityState>,
        SuiAddress,
        AccountKeyPair,
        Object,
        Object,
        Vec<Object>,
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
        let slot_of = |object: &Object| {
            let seed = stagger_seed(&[object.id()], &[], epoch);
            stagger_slot(&seed, epoch_store.committee(), own_index)
        };
        // 32 candidates make both picks overwhelmingly likely (miss odds ~(1/4)^32 and
        // ~(3/4)^32 respectively).
        let staggered_gas = candidates
            .iter()
            .find(|o| slot_of(o) >= 1)
            .expect("no candidate gas object lands past slot 0")
            .clone();
        let immediate_gas = candidates
            .iter()
            .find(|o| slot_of(o) == 0)
            .expect("no candidate gas object lands first")
            .clone();

        (
            state,
            sender,
            keypair,
            staggered_gas,
            immediate_gas,
            candidates,
        )
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
    async fn staggered_submission_delays_held_slots_only() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut c| {
            c.set_allowed_proposers_for_testing(true);
            c
        });
        let (state, sender, keypair, staggered_gas, immediate_gas, _) = setup().await;

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

        // Slot 0 for its gas object: submits immediately even while staggering is active.
        let tx = test_user_transaction(&state, sender, &keypair, immediate_gas, vec![]).await;
        let elapsed = submit_and_wait(&adapter, &state, tx).await;
        assert_eq!(client.submit_times.lock().len(), 1);
        assert!(elapsed < STEP, "immediate slot was delayed by {elapsed:?}");

        // Past slot 0: held for at least one step.
        let tx =
            test_user_transaction(&state, sender, &keypair, staggered_gas.clone(), vec![]).await;
        let elapsed = submit_and_wait(&adapter, &state, tx).await;
        assert_eq!(client.submit_times.lock().len(), 2);
        assert!(
            elapsed >= STEP,
            "held slot submitted after only {elapsed:?}"
        );

        // Inactive: the same held slot submits immediately. A new gas coin was created by
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
        let (state, sender, keypair, staggered_gas, _, _) = setup().await;
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
        let (state, sender, keypair, staggered_gas, _, _) = setup().await;
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
    async fn immediate_slot_and_inactive_not_held() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut c| {
            c.set_allowed_proposers_for_testing(true);
            c
        });
        let (state, sender, keypair, staggered_gas, immediate_gas, _) = setup().await;
        let epoch_store = state.epoch_store_for_testing();
        let pool = pool_for(&state);

        let staggered = epoch_store.staggered_submission();
        staggered.set_params_for_testing(StaggerParams {
            step: Duration::from_secs(60),
            max_delay: Duration::from_secs(60),
            free_slots: 1,
        });
        staggered.set_active(true);

        // Slot 0 for its gas object: proposed on the next take while staggering is active.
        let (_receiver, _) = insert(&pool, &state, sender, &keypair, immediate_gas).await;
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert_eq!(transactions.len(), 1, "immediate slot was held");
        ack(BlockRef::new(
            1,
            AuthorityIndex::new_for_test(0),
            BlockDigest::MIN,
        ));

        // Inactive: a slot that would be held is proposed immediately.
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
        let (state, sender, keypair, staggered_gas, _, _) = setup().await;
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
    async fn soft_bundle_with_unrestricted_members_held() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut c| {
            c.set_allowed_proposers_for_testing(true);
            c
        });
        let (state, sender, keypair, _, immediate_gas, candidates) = setup().await;
        let epoch_store = state.epoch_store_for_testing();
        let pool = pool_for(&state);

        let staggered = epoch_store.staggered_submission();
        staggered.set_params_for_testing(StaggerParams {
            step: Duration::from_millis(250),
            max_delay: Duration::from_millis(500),
            free_slots: 1,
        });
        staggered.set_active(true);

        // A soft bundle's slot derives from the union of its members' gas objects:
        // grind a companion so the bundle is held even though one member would land
        // first on its own. Soft bundles are external fan-out and must not bypass the hold.
        let epoch = epoch_store.epoch();
        let own_index = epoch_store.own_committee_index().unwrap();
        let companion = candidates
            .iter()
            .filter(|object| object.id() != immediate_gas.id())
            .find(|object| {
                let seed = stagger_seed(&[immediate_gas.id(), object.id()], &[], epoch);
                stagger_slot(&seed, epoch_store.committee(), own_index) >= 1
            })
            .expect("no companion makes the bundle land past slot 0")
            .clone();

        let tx_a = test_user_transaction(&state, sender, &keypair, immediate_gas, vec![]).await;
        let tx_b = test_user_transaction(&state, sender, &keypair, companion, vec![]).await;
        let bundle = vec![
            ConsensusTransaction::new_user_transaction_v2_message(&state.name, tx_a.into()),
            ConsensusTransaction::new_user_transaction_v2_message(&state.name, tx_b.into()),
        ];
        let (_receiver, newly_inserted) = pool.try_insert(pool.epoch(), 1, bundle).unwrap();
        assert!(newly_inserted);
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert!(
            transactions.is_empty(),
            "unrestricted soft bundle was not held"
        );
        drop(ack);

        // Past the (capped) delay the bundle is proposed atomically.
        tokio::time::sleep(Duration::from_millis(600)).await;
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert_eq!(
            transactions.len(),
            2,
            "released bundle was not proposed whole"
        );
        // The dropped ack requeues the bundle; close() resolves it before the pool drops.
        drop(ack);
        pool.close();
    }
}
