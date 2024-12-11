// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    cmp::Ordering,
    fmt::{self, Debug, Display, Formatter},
    hash::{Hash, Hasher},
    ops::{Deref, Range, RangeInclusive},
    sync::Arc,
};

use bytes::Bytes;
use consensus_config::{AuthorityIndex, DefaultHashFunction, DIGEST_LENGTH};
use enum_dispatch::enum_dispatch;
use fastcrypto::hash::{Digest, HashFunction as _};
use serde::{Deserialize, Serialize};

use crate::{
    block::{BlockAPI, BlockRef, BlockTimestampMs, Round, Slot, VerifiedBlock},
    leader_scoring::ReputationScores,
    storage::Store,
    TransactionIndex,
};

/// Index of a commit among all consensus commits.
pub type CommitIndex = u32;

pub(crate) const GENESIS_COMMIT_INDEX: CommitIndex = 0;

/// Default wave length for all committers. A longer wave length increases the
/// chance of committing the leader under asynchrony at the cost of latency in
/// the common case.
// TODO: merge DEFAULT_WAVE_LENGTH and MINIMUM_WAVE_LENGTH into a single constant,
// because we are unlikely to change them via config in the forseeable future.
pub(crate) const DEFAULT_WAVE_LENGTH: Round = MINIMUM_WAVE_LENGTH;

/// We need at least one leader round, one voting round, and one decision round.
pub(crate) const MINIMUM_WAVE_LENGTH: Round = 3;

/// The consensus protocol operates in 'waves'. Each wave is composed of a leader
/// round, at least one voting round, and one decision round.
pub(crate) type WaveNumber = u32;

/// [`Commit`] summarizes [`CommittedSubDag`] for storage and network communications.
///
/// Validators should be able to reconstruct a sequence of CommittedSubDag from the
/// corresponding Commit and blocks referenced in the Commit.
/// A field must meet these requirements to be added to Commit:
/// - helps with recovery locally and for peers catching up.
/// - cannot be derived from a sequence of Commits and other persisted values.
///
/// For example, transactions in blocks should not be included in Commit, because they can be
/// retrieved from blocks specified in Commit. Last committed round per authority also should not
/// be included, because it can be derived from the latest value in storage and the additional
/// sequence of Commits.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[enum_dispatch(CommitAPI)]
pub(crate) enum Commit {
    V1(CommitV1),
}

impl Commit {
    /// Create a new commit.
    pub(crate) fn new(
        index: CommitIndex,
        previous_digest: CommitDigest,
        timestamp_ms: BlockTimestampMs,
        leader: BlockRef,
        blocks: Vec<BlockRef>,
    ) -> Self {
        Commit::V1(CommitV1 {
            index,
            previous_digest,
            timestamp_ms,
            leader,
            blocks,
        })
    }

    pub(crate) fn serialize(&self) -> Result<Bytes, bcs::Error> {
        let bytes = bcs::to_bytes(self)?;
        Ok(bytes.into())
    }
}

/// Accessors to Commit info.
#[enum_dispatch]
pub(crate) trait CommitAPI {
    fn round(&self) -> Round;
    fn index(&self) -> CommitIndex;
    fn previous_digest(&self) -> CommitDigest;
    fn timestamp_ms(&self) -> BlockTimestampMs;
    fn leader(&self) -> BlockRef;
    fn blocks(&self) -> &[BlockRef];
}

/// Specifies one consensus commit.
/// It is stored on disk, so it does not contain blocks which are stored individually.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub(crate) struct CommitV1 {
    /// Index of the commit.
    /// First commit after genesis has an index of 1, then every next commit has an index incremented by 1.
    index: CommitIndex,
    /// Digest of the previous commit.
    /// Set to CommitDigest::MIN for the first commit after genesis.
    previous_digest: CommitDigest,
    /// Timestamp of the commit, max of the timestamp of the leader block and previous Commit timestamp.
    timestamp_ms: BlockTimestampMs,
    /// A reference to the commit leader.
    leader: BlockRef,
    /// Refs to committed blocks, in the commit order.
    blocks: Vec<BlockRef>,
    // TODO(fastpath): record rejected transactions.
}

impl CommitAPI for CommitV1 {
    fn round(&self) -> Round {
        self.leader.round
    }

    fn index(&self) -> CommitIndex {
        self.index
    }

    fn previous_digest(&self) -> CommitDigest {
        self.previous_digest
    }

    fn timestamp_ms(&self) -> BlockTimestampMs {
        self.timestamp_ms
    }

    fn leader(&self) -> BlockRef {
        self.leader
    }

    fn blocks(&self) -> &[BlockRef] {
        &self.blocks
    }
}

/// A commit is trusted when it is produced locally or certified by a quorum of authorities.
/// Blocks referenced by TrustedCommit are assumed to be valid.
/// Only trusted Commit can be sent to execution.
///
/// Note: clone() is relatively cheap with the underlying data refcounted.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TrustedCommit {
    inner: Arc<Commit>,

    // Cached digest and serialized value, to avoid re-computing these values.
    digest: CommitDigest,
    serialized: Bytes,
}

impl TrustedCommit {
    pub(crate) fn new_trusted(commit: Commit, serialized: Bytes) -> Self {
        let digest = Self::compute_digest(&serialized);
        Self {
            inner: Arc::new(commit),
            digest,
            serialized,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(
        index: CommitIndex,
        previous_digest: CommitDigest,
        timestamp_ms: BlockTimestampMs,
        leader: BlockRef,
        blocks: Vec<BlockRef>,
    ) -> Self {
        let commit = Commit::new(index, previous_digest, timestamp_ms, leader, blocks);
        let serialized = commit.serialize().unwrap();
        Self::new_trusted(commit, serialized)
    }

    pub(crate) fn reference(&self) -> CommitRef {
        CommitRef {
            index: self.index(),
            digest: self.digest(),
        }
    }

    pub(crate) fn digest(&self) -> CommitDigest {
        self.digest
    }

    pub(crate) fn serialized(&self) -> &Bytes {
        &self.serialized
    }

    pub(crate) fn compute_digest(serialized: &[u8]) -> CommitDigest {
        let mut hasher = DefaultHashFunction::new();
        hasher.update(serialized);
        CommitDigest(hasher.finalize().into())
    }
}

/// Allow easy access on the underlying Commit.
impl Deref for TrustedCommit {
    type Target = Commit;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// Digest of a consensus commit.
#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct CommitDigest([u8; consensus_config::DIGEST_LENGTH]);

impl CommitDigest {
    /// Lexicographic min & max digest.
    pub const MIN: Self = Self([u8::MIN; consensus_config::DIGEST_LENGTH]);
    pub const MAX: Self = Self([u8::MAX; consensus_config::DIGEST_LENGTH]);

    pub fn into_inner(self) -> [u8; consensus_config::DIGEST_LENGTH] {
        self.0
    }
}

impl Hash for CommitDigest {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(&self.0[..8]);
    }
}

impl From<CommitDigest> for Digest<{ DIGEST_LENGTH }> {
    fn from(hd: CommitDigest) -> Self {
        Digest::new(hd.0)
    }
}

impl fmt::Display for CommitDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, self.0)
                .get(0..4)
                .ok_or(fmt::Error)?
        )
    }
}

impl fmt::Debug for CommitDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, self.0)
        )
    }
}

/// Uniquely identifies a commit with its index and digest.
#[derive(Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct CommitRef {
    pub index: CommitIndex,
    pub digest: CommitDigest,
}

impl CommitRef {
    pub fn new(index: CommitIndex, digest: CommitDigest) -> Self {
        Self { index, digest }
    }
}

impl fmt::Display for CommitRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "C{}({})", self.index, self.digest)
    }
}

impl fmt::Debug for CommitRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "C{}({:?})", self.index, self.digest)
    }
}

// Represents a vote on a Commit.
pub type CommitVote = CommitRef;

/// The output of consensus to execution is an ordered list of [`CommittedSubDag`].
/// Each CommittedSubDag contains the information needed to execution transactions in
/// the consensus commit.
///
/// The application processing CommittedSubDag can arbitrarily sort the blocks within
/// each sub-dag (but using a deterministic algorithm).
#[derive(Clone, PartialEq)]
pub struct CommittedSubDag {
    /// A reference to the leader of the sub-dag
    pub leader: BlockRef,
    /// All the committed blocks that are part of this sub-dag
    pub blocks: Vec<VerifiedBlock>,
    /// Indices of rejected transactions in each block.
    pub rejected_transactions_by_block: Vec<Vec<TransactionIndex>>,
    /// The timestamp of the commit, obtained from the timestamp of the leader block.
    pub timestamp_ms: BlockTimestampMs,
    /// The reference of the commit.
    /// First commit after genesis has a index of 1, then every next commit has a
    /// index incremented by 1.
    pub commit_ref: CommitRef,
    /// Optional scores that are provided as part of the consensus output to Sui
    /// that can then be used by Sui for future submission to consensus.
    pub reputation_scores_desc: Vec<(AuthorityIndex, u64)>,
}

impl CommittedSubDag {
    /// Creates a new committed sub dag.
    pub fn new(
        leader: BlockRef,
        blocks: Vec<VerifiedBlock>,
        rejected_transactions_by_block: Vec<Vec<TransactionIndex>>,
        timestamp_ms: BlockTimestampMs,
        commit_ref: CommitRef,
        reputation_scores_desc: Vec<(AuthorityIndex, u64)>,
    ) -> Self {
        assert_eq!(blocks.len(), rejected_transactions_by_block.len());
        Self {
            leader,
            blocks,
            rejected_transactions_by_block,
            timestamp_ms,
            commit_ref,
            reputation_scores_desc,
        }
    }
}

// Sort the blocks of the sub-dag blocks by round number then authority index. Any
// deterministic & stable algorithm works.
pub(crate) fn sort_sub_dag_blocks(blocks: &mut [VerifiedBlock]) {
    blocks.sort_by(|a, b| {
        a.round()
            .cmp(&b.round())
            .then_with(|| a.author().cmp(&b.author()))
    })
}

impl Display for CommittedSubDag {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CommittedSubDag(leader={}, ref={}, blocks=[",
            self.leader, self.commit_ref
        )?;
        for (idx, block) in self.blocks.iter().enumerate() {
            if idx > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", block.digest())?;
        }
        write!(f, "])")
    }
}

impl fmt::Debug for CommittedSubDag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{} ([", self.leader, self.commit_ref)?;
        for block in &self.blocks {
            write!(f, "{}, ", block.reference())?;
        }
        write!(
            f,
            "];{}ms;rs{:?})",
            self.timestamp_ms, self.reputation_scores_desc
        )
    }
}

// Recovers the full CommittedSubDag from block store, based on Commit.
pub fn load_committed_subdag_from_store(
    store: &dyn Store,
    commit: TrustedCommit,
    reputation_scores_desc: Vec<(AuthorityIndex, u64)>,
) -> CommittedSubDag {
    let mut leader_block_idx = None;
    let commit_blocks = store
        .read_blocks(commit.blocks())
        .expect("We should have the block referenced in the commit data");
    let blocks = commit_blocks
        .into_iter()
        .enumerate()
        .map(|(idx, commit_block_opt)| {
            let commit_block =
                commit_block_opt.expect("We should have the block referenced in the commit data");
            if commit_block.reference() == commit.leader() {
                leader_block_idx = Some(idx);
            }
            commit_block
        })
        .collect::<Vec<_>>();
    // TODO(fastpath): recover rejected transaction indices from commit.
    let rejected_transactions = vec![vec![]; blocks.len()];
    let leader_block_idx = leader_block_idx.expect("Leader block must be in the sub-dag");
    let leader_block_ref = blocks[leader_block_idx].reference();
    CommittedSubDag::new(
        leader_block_ref,
        blocks,
        rejected_transactions,
        commit.timestamp_ms(),
        commit.reference(),
        reputation_scores_desc,
    )
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum Decision {
    Direct,
    Indirect,
}

/// The status of a leader slot from the direct and indirect commit rules.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LeaderStatus {
    Commit(VerifiedBlock),
    Skip(Slot),
    Undecided(Slot),
}

impl LeaderStatus {
    pub(crate) fn round(&self) -> Round {
        match self {
            Self::Commit(block) => block.round(),
            Self::Skip(leader) => leader.round,
            Self::Undecided(leader) => leader.round,
        }
    }

    pub(crate) fn is_decided(&self) -> bool {
        match self {
            Self::Commit(_) => true,
            Self::Skip(_) => true,
            Self::Undecided(_) => false,
        }
    }

    pub(crate) fn into_decided_leader(self) -> Option<DecidedLeader> {
        match self {
            Self::Commit(block) => Some(DecidedLeader::Commit(block)),
            Self::Skip(slot) => Some(DecidedLeader::Skip(slot)),
            Self::Undecided(..) => None,
        }
    }
}

impl Display for LeaderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Commit(block) => write!(f, "Commit({})", block.reference()),
            Self::Skip(slot) => write!(f, "Skip({slot})"),
            Self::Undecided(slot) => write!(f, "Undecided({slot})"),
        }
    }
}

/// Decision of each leader slot.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DecidedLeader {
    Commit(VerifiedBlock),
    Skip(Slot),
}

impl DecidedLeader {
    // Slot where the leader is decided.
    pub(crate) fn slot(&self) -> Slot {
        match self {
            Self::Commit(block) => block.reference().into(),
            Self::Skip(slot) => *slot,
        }
    }

    // Converts to committed block if the decision is to commit. Returns None otherwise.
    pub(crate) fn into_committed_block(self) -> Option<VerifiedBlock> {
        match self {
            Self::Commit(block) => Some(block),
            Self::Skip(_) => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn round(&self) -> Round {
        match self {
            Self::Commit(block) => block.round(),
            Self::Skip(leader) => leader.round,
        }
    }

    #[cfg(test)]
    pub(crate) fn authority(&self) -> AuthorityIndex {
        match self {
            Self::Commit(block) => block.author(),
            Self::Skip(leader) => leader.authority,
        }
    }
}

impl Display for DecidedLeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Commit(block) => write!(f, "Commit({})", block.reference()),
            Self::Skip(slot) => write!(f, "Skip({slot})"),
        }
    }
}

/// Per-commit properties that can be regenerated from past values, and do not need to be part of
/// the Commit struct.
/// Only the latest version is needed for recovery, but more versions are stored for debugging,
/// and potentially restoring from an earlier state.
// TODO: version this struct.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CommitInfo {
    pub(crate) committed_rounds: Vec<Round>,
    pub(crate) reputation_scores: ReputationScores,
}

/// CommitRange stores a range of CommitIndex. The range contains the start (inclusive)
/// and end (inclusive) commit indices and can be ordered for use as the key of a table.
///
/// NOTE: using Range<CommitIndex> for internal representation for backward compatibility.
/// The external semantics of CommitRange is closer to RangeInclusive<CommitIndex>.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CommitRange(Range<CommitIndex>);

impl CommitRange {
    pub(crate) fn new(range: RangeInclusive<CommitIndex>) -> Self {
        // When end is CommitIndex::MAX, the range can be considered as unbounded
        // so it is ok to saturate at the end.
        Self(*range.start()..(*range.end()).saturating_add(1))
    }

    // Inclusive
    pub(crate) fn start(&self) -> CommitIndex {
        self.0.start
    }

    // Inclusive
    pub(crate) fn end(&self) -> CommitIndex {
        self.0.end.saturating_sub(1)
    }

    pub(crate) fn extend_to(&mut self, other: CommitIndex) {
        let new_end = other.saturating_add(1);
        assert!(self.0.end <= new_end);
        self.0 = self.0.start..new_end;
    }

    pub(crate) fn size(&self) -> usize {
        self.0
            .end
            .checked_sub(self.0.start)
            .expect("Range should never have end < start") as usize
    }

    /// Check whether the two ranges have the same size.
    pub(crate) fn is_equal_size(&self, other: &Self) -> bool {
        self.size() == other.size()
    }

    /// Check if the provided range is sequentially after this range.
    pub(crate) fn is_next_range(&self, other: &Self) -> bool {
        self.0.end == other.0.start
    }
}

impl Ord for CommitRange {
    fn cmp(&self, other: &Self) -> Ordering {
        self.start()
            .cmp(&other.start())
            .then_with(|| self.end().cmp(&other.end()))
    }
}

impl PartialOrd for CommitRange {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl From<RangeInclusive<CommitIndex>> for CommitRange {
    fn from(range: RangeInclusive<CommitIndex>) -> Self {
        Self::new(range)
    }
}

/// Display CommitRange as an inclusive range.
impl Debug for CommitRange {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "CommitRange({}..={})", self.start(), self.end())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        block::TestBlock,
        context::Context,
        storage::{mem_store::MemStore, WriteBatch},
    };

    #[tokio::test]
    async fn test_new_subdag_from_commit() {
        let store = Arc::new(MemStore::new());
        let context = Arc::new(Context::new_for_test(4).0);
        let wave_length = DEFAULT_WAVE_LENGTH;

        // Populate fully connected test blocks for round 0 ~ 3, authorities 0 ~ 3.
        let first_wave_rounds: u32 = wave_length;
        let num_authorities: u32 = 4;

        let mut blocks = Vec::new();
        let (genesis_references, genesis): (Vec<_>, Vec<_>) = context
            .committee
            .authorities()
            .map(|index| {
                let author_idx = index.0.value() as u32;
                let block = TestBlock::new(0, author_idx).build();
                VerifiedBlock::new_for_test(block)
            })
            .map(|block| (block.reference(), block))
            .unzip();
        // TODO: avoid writing genesis blocks?
        store.write(WriteBatch::default().blocks(genesis)).unwrap();
        blocks.append(&mut genesis_references.clone());

        let mut ancestors = genesis_references;
        let mut leader = None;
        for round in 1..=first_wave_rounds {
            let mut new_ancestors = vec![];
            for author in 0..num_authorities {
                let base_ts = round as BlockTimestampMs * 1000;
                let block = VerifiedBlock::new_for_test(
                    TestBlock::new(round, author)
                        .set_timestamp_ms(base_ts + (author + round) as u64)
                        .set_ancestors(ancestors.clone())
                        .build(),
                );
                store
                    .write(WriteBatch::default().blocks(vec![block.clone()]))
                    .unwrap();
                new_ancestors.push(block.reference());
                blocks.push(block.reference());

                // only write one block for the final round, which is the leader
                // of the committed subdag.
                if round == first_wave_rounds {
                    leader = Some(block.clone());
                    break;
                }
            }
            ancestors = new_ancestors;
        }

        let leader_block = leader.unwrap();
        let leader_ref = leader_block.reference();
        let commit_index = 1;
        let commit = TrustedCommit::new_for_test(
            commit_index,
            CommitDigest::MIN,
            leader_block.timestamp_ms(),
            leader_ref,
            blocks.clone(),
        );
        let subdag = load_committed_subdag_from_store(store.as_ref(), commit.clone(), vec![]);
        assert_eq!(subdag.leader, leader_ref);
        assert_eq!(subdag.timestamp_ms, leader_block.timestamp_ms());
        assert_eq!(
            subdag.blocks.len(),
            (num_authorities * wave_length) as usize + 1
        );
        assert_eq!(subdag.commit_ref, commit.reference());
        assert_eq!(subdag.reputation_scores_desc, vec![]);
    }

    #[tokio::test]
    async fn test_commit_range() {
        telemetry_subscribers::init_for_testing();
        let mut range1 = CommitRange::new(1..=5);
        let range2 = CommitRange::new(2..=6);
        let range3 = CommitRange::new(5..=10);
        let range4 = CommitRange::new(6..=10);
        let range5 = CommitRange::new(6..=9);
        let range6 = CommitRange::new(1..=1);

        assert_eq!(range1.start(), 1);
        assert_eq!(range1.end(), 5);

        // Test range size
        assert_eq!(range1.size(), 5);
        assert_eq!(range3.size(), 6);
        assert_eq!(range6.size(), 1);

        // Test next range check
        assert!(!range1.is_next_range(&range2));
        assert!(!range1.is_next_range(&range3));
        assert!(range1.is_next_range(&range4));
        assert!(range1.is_next_range(&range5));

        // Test equal size range check
        assert!(range1.is_equal_size(&range2));
        assert!(!range1.is_equal_size(&range3));
        assert!(range1.is_equal_size(&range4));
        assert!(!range1.is_equal_size(&range5));

        // Test range ordering
        assert!(range1 < range2);
        assert!(range2 < range3);
        assert!(range3 < range4);
        assert!(range5 < range4);

        // Test extending range
        range1.extend_to(10);
        assert_eq!(range1.start(), 1);
        assert_eq!(range1.end(), 10);
        assert_eq!(range1.size(), 10);

        range1.extend_to(20);
        assert_eq!(range1.start(), 1);
        assert_eq!(range1.end(), 20);
        assert_eq!(range1.size(), 20);
    }
}
