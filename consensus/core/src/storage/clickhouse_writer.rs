// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;

use clickhouse::{Client, Row};
use consensus_config::{AuthorityIndex, ClickHouseParameters};
use consensus_types::block::{BlockRef, Round, TransactionIndex};
use serde::Serialize;
use tokio::sync::mpsc;
use tracing::warn;

use super::{CommitInfo, Store, WriteBatch};
use crate::block::{BlockAPI as _, VerifiedBlock};
use crate::commit::{CommitAPI as _, CommitIndex, CommitRef, TrustedCommit};
use crate::error::ConsensusResult;

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Row, Serialize)]
pub(crate) struct BlockRow {
    pub epoch: u64,
    pub round: u32,
    pub author: u32,
    pub digest: String,
    pub timestamp_ms: u64,
    pub num_transactions: u32,
    pub num_ancestors: u32,
    pub num_commit_votes: u32,
    pub num_misbehavior_reports: u32,
    pub serialized_size_bytes: u32,
    pub raw_bytes: Vec<u8>,
}

#[derive(Row, Serialize)]
pub(crate) struct CommitRow {
    pub epoch: u64,
    pub commit_index: u32,
    pub commit_digest: String,
    pub previous_digest: String,
    pub timestamp_ms: u64,
    pub leader_round: u32,
    pub leader_author: u32,
    pub leader_digest: String,
    pub num_blocks: u32,
    pub serialized_size_bytes: u32,
    pub raw_bytes: Vec<u8>,
}

#[derive(Row, Serialize)]
pub(crate) struct CommitInfoRow {
    pub epoch: u64,
    pub commit_index: u32,
    pub commit_digest: String,
    pub committed_rounds: Vec<u32>,
    pub reputation_scores: Vec<u64>,
    pub score_commit_range_start: u32,
    pub score_commit_range_end: u32,
}

#[derive(Row, Serialize)]
pub(crate) struct FinalizedCommitRow {
    pub epoch: u64,
    pub commit_index: u32,
    pub commit_digest: String,
    pub num_blocks_with_rejections: u32,
    pub total_rejected_transactions: u32,
}

pub(crate) struct ClickHouseWriteBatch {
    pub blocks: Vec<BlockRow>,
    pub commits: Vec<CommitRow>,
    pub commit_info: Vec<CommitInfoRow>,
    pub finalized_commits: Vec<FinalizedCommitRow>,
}

impl ClickHouseWriteBatch {
    fn is_empty(&self) -> bool {
        self.blocks.is_empty()
            && self.commits.is_empty()
            && self.commit_info.is_empty()
            && self.finalized_commits.is_empty()
    }
}

pub(crate) struct DualWriteStore {
    inner: Arc<dyn Store>,
    sender: mpsc::Sender<ClickHouseWriteBatch>,
    epoch: u64,
    include_raw_bytes: bool,
}

impl DualWriteStore {
    pub fn new(
        inner: Arc<dyn Store>,
        sender: mpsc::Sender<ClickHouseWriteBatch>,
        epoch: u64,
        include_raw_bytes: bool,
    ) -> Self {
        Self {
            inner,
            sender,
            epoch,
            include_raw_bytes,
        }
    }

    fn extract_block_row(&self, block: &VerifiedBlock) -> BlockRow {
        BlockRow {
            epoch: block.epoch(),
            round: block.round(),
            author: block.author().value() as u32,
            digest: hex_encode(&block.digest().0),
            timestamp_ms: block.timestamp_ms(),
            num_transactions: block.transactions().len() as u32,
            num_ancestors: block.ancestors().len() as u32,
            num_commit_votes: block.commit_votes().len() as u32,
            num_misbehavior_reports: block.misbehavior_reports().len() as u32,
            serialized_size_bytes: block.serialized().len() as u32,
            raw_bytes: if self.include_raw_bytes {
                block.serialized().to_vec()
            } else {
                vec![]
            },
        }
    }

    fn extract_commit_row(&self, commit: &TrustedCommit) -> CommitRow {
        CommitRow {
            epoch: self.epoch,
            commit_index: commit.index(),
            commit_digest: hex_encode(&commit.digest().into_inner()),
            previous_digest: hex_encode(&commit.previous_digest().into_inner()),
            timestamp_ms: commit.timestamp_ms(),
            leader_round: commit.leader().round,
            leader_author: commit.leader().author.value() as u32,
            leader_digest: hex_encode(&commit.leader().digest.0),
            num_blocks: commit.blocks().len() as u32,
            serialized_size_bytes: commit.serialized().len() as u32,
            raw_bytes: if self.include_raw_bytes {
                commit.serialized().to_vec()
            } else {
                vec![]
            },
        }
    }

    fn extract_commit_info_row(&self, commit_ref: &CommitRef, info: &CommitInfo) -> CommitInfoRow {
        CommitInfoRow {
            epoch: self.epoch,
            commit_index: commit_ref.index,
            commit_digest: hex_encode(&commit_ref.digest.into_inner()),
            committed_rounds: info.committed_rounds.clone(),
            reputation_scores: info.reputation_scores.scores_per_authority.clone(),
            score_commit_range_start: info.reputation_scores.commit_range.start(),
            score_commit_range_end: info.reputation_scores.commit_range.end(),
        }
    }

    fn extract_finalized_commit_row(
        &self,
        commit_ref: &CommitRef,
        rejected_txns: &BTreeMap<BlockRef, Vec<TransactionIndex>>,
    ) -> FinalizedCommitRow {
        FinalizedCommitRow {
            epoch: self.epoch,
            commit_index: commit_ref.index,
            commit_digest: hex_encode(&commit_ref.digest.into_inner()),
            num_blocks_with_rejections: rejected_txns.len() as u32,
            total_rejected_transactions: rejected_txns.values().map(|v| v.len() as u32).sum(),
        }
    }

    fn extract_metadata(&self, write_batch: &WriteBatch) -> ClickHouseWriteBatch {
        ClickHouseWriteBatch {
            blocks: write_batch
                .blocks
                .iter()
                .map(|b| self.extract_block_row(b))
                .collect(),
            commits: write_batch
                .commits
                .iter()
                .map(|c| self.extract_commit_row(c))
                .collect(),
            commit_info: write_batch
                .commit_info
                .iter()
                .map(|(r, i)| self.extract_commit_info_row(r, i))
                .collect(),
            finalized_commits: write_batch
                .finalized_commits
                .iter()
                .map(|(r, t)| self.extract_finalized_commit_row(r, t))
                .collect(),
        }
    }
}

impl Store for DualWriteStore {
    fn write(&self, write_batch: WriteBatch) -> ConsensusResult<()> {
        // Extract metadata by borrowing before the batch is moved to the inner store.
        let ch_batch = self.extract_metadata(&write_batch);
        self.inner.write(write_batch)?;
        if !ch_batch.is_empty() {
            if let Err(e) = self.sender.try_send(ch_batch) {
                warn!(
                    "ClickHouse debug writer channel full or closed, dropping batch: {}",
                    e
                );
            }
        }
        Ok(())
    }

    fn read_blocks(&self, refs: &[BlockRef]) -> ConsensusResult<Vec<Option<VerifiedBlock>>> {
        self.inner.read_blocks(refs)
    }

    fn contains_blocks(&self, refs: &[BlockRef]) -> ConsensusResult<Vec<bool>> {
        self.inner.contains_blocks(refs)
    }

    fn scan_blocks_by_author(
        &self,
        author: AuthorityIndex,
        start_round: Round,
    ) -> ConsensusResult<Vec<VerifiedBlock>> {
        self.inner.scan_blocks_by_author(author, start_round)
    }

    fn scan_last_blocks_by_author(
        &self,
        author: AuthorityIndex,
        num_of_rounds: u64,
        before_round: Option<Round>,
    ) -> ConsensusResult<Vec<VerifiedBlock>> {
        self.inner
            .scan_last_blocks_by_author(author, num_of_rounds, before_round)
    }

    fn read_last_commit(&self) -> ConsensusResult<Option<TrustedCommit>> {
        self.inner.read_last_commit()
    }

    fn scan_commits(
        &self,
        range: crate::commit::CommitRange,
    ) -> ConsensusResult<Vec<TrustedCommit>> {
        self.inner.scan_commits(range)
    }

    fn read_commit_votes(&self, commit_index: CommitIndex) -> ConsensusResult<Vec<BlockRef>> {
        self.inner.read_commit_votes(commit_index)
    }

    fn read_last_commit_info(&self) -> ConsensusResult<Option<(CommitRef, CommitInfo)>> {
        self.inner.read_last_commit_info()
    }

    fn read_last_finalized_commit(&self) -> ConsensusResult<Option<CommitRef>> {
        self.inner.read_last_finalized_commit()
    }

    fn read_rejected_transactions(
        &self,
        commit_ref: CommitRef,
    ) -> ConsensusResult<Option<BTreeMap<BlockRef, Vec<TransactionIndex>>>> {
        self.inner.read_rejected_transactions(commit_ref)
    }
}

// ---------------------------------------------------------------------------
// ClickHouseWriter — async background task
// ---------------------------------------------------------------------------

const CREATE_BLOCKS_TABLE: &str = "
CREATE TABLE IF NOT EXISTS consensus_blocks
(
    epoch                    UInt64,
    round                    UInt32,
    author                   UInt32,
    digest                   String,
    timestamp_ms             UInt64,
    num_transactions         UInt32,
    num_ancestors            UInt32,
    num_commit_votes         UInt32,
    num_misbehavior_reports  UInt32,
    serialized_size_bytes    UInt32,
    raw_bytes                String,
    inserted_at              DateTime DEFAULT now()
)
ENGINE = MergeTree()
PARTITION BY epoch
ORDER BY (epoch, author, round, digest)
TTL inserted_at + INTERVAL 48 HOUR
SETTINGS index_granularity = 8192
";

const CREATE_COMMITS_TABLE: &str = "
CREATE TABLE IF NOT EXISTS consensus_commits
(
    epoch                UInt64,
    commit_index         UInt32,
    commit_digest        String,
    previous_digest      String,
    timestamp_ms         UInt64,
    leader_round         UInt32,
    leader_author        UInt32,
    leader_digest        String,
    num_blocks           UInt32,
    serialized_size_bytes UInt32,
    raw_bytes            String,
    inserted_at          DateTime DEFAULT now()
)
ENGINE = MergeTree()
PARTITION BY epoch
ORDER BY (epoch, commit_index, commit_digest)
TTL inserted_at + INTERVAL 48 HOUR
SETTINGS index_granularity = 8192
";

const CREATE_COMMIT_INFO_TABLE: &str = "
CREATE TABLE IF NOT EXISTS consensus_commit_info
(
    epoch                    UInt64,
    commit_index             UInt32,
    commit_digest            String,
    committed_rounds         Array(UInt32),
    reputation_scores        Array(UInt64),
    score_commit_range_start UInt32,
    score_commit_range_end   UInt32,
    inserted_at              DateTime DEFAULT now()
)
ENGINE = MergeTree()
PARTITION BY epoch
ORDER BY (epoch, commit_index, commit_digest)
TTL inserted_at + INTERVAL 48 HOUR
SETTINGS index_granularity = 8192
";

const CREATE_FINALIZED_COMMITS_TABLE: &str = "
CREATE TABLE IF NOT EXISTS consensus_finalized_commits
(
    epoch                        UInt64,
    commit_index                 UInt32,
    commit_digest                String,
    num_blocks_with_rejections   UInt32,
    total_rejected_transactions  UInt32,
    inserted_at                  DateTime DEFAULT now()
)
ENGINE = MergeTree()
PARTITION BY epoch
ORDER BY (epoch, commit_index, commit_digest)
TTL inserted_at + INTERVAL 48 HOUR
SETTINGS index_granularity = 8192
";

pub(crate) struct ClickHouseWriter {
    client: Client,
    receiver: mpsc::Receiver<ClickHouseWriteBatch>,
}

impl ClickHouseWriter {
    pub fn new(
        config: &ClickHouseParameters,
        receiver: mpsc::Receiver<ClickHouseWriteBatch>,
    ) -> Self {
        let client = Client::default()
            .with_url(&config.url)
            .with_database(&config.database)
            .with_compression(clickhouse::Compression::Lz4);
        Self { client, receiver }
    }

    async fn create_tables(&self) {
        for ddl in [
            CREATE_BLOCKS_TABLE,
            CREATE_COMMITS_TABLE,
            CREATE_COMMIT_INFO_TABLE,
            CREATE_FINALIZED_COMMITS_TABLE,
        ] {
            if let Err(e) = self.client.query(ddl).execute().await {
                warn!("ClickHouse debug writer: failed to create table: {}", e);
            }
        }
    }

    async fn insert_batch(&self, batch: ClickHouseWriteBatch) {
        if !batch.blocks.is_empty() {
            if let Err(e) = self.insert_rows("consensus_blocks", &batch.blocks).await {
                warn!("ClickHouse debug writer: failed to insert blocks: {}", e);
            }
        }
        if !batch.commits.is_empty() {
            if let Err(e) = self.insert_rows("consensus_commits", &batch.commits).await {
                warn!("ClickHouse debug writer: failed to insert commits: {}", e);
            }
        }
        if !batch.commit_info.is_empty() {
            if let Err(e) = self
                .insert_rows("consensus_commit_info", &batch.commit_info)
                .await
            {
                warn!(
                    "ClickHouse debug writer: failed to insert commit_info: {}",
                    e
                );
            }
        }
        if !batch.finalized_commits.is_empty() {
            if let Err(e) = self
                .insert_rows("consensus_finalized_commits", &batch.finalized_commits)
                .await
            {
                warn!(
                    "ClickHouse debug writer: failed to insert finalized_commits: {}",
                    e
                );
            }
        }
    }

    async fn insert_rows<T: Row + Serialize>(
        &self,
        table: &str,
        rows: &[T],
    ) -> Result<(), clickhouse::error::Error> {
        let mut inserter = self.client.insert(table)?;
        for row in rows {
            inserter.write(row).await?;
        }
        inserter.end().await
    }

    pub async fn run(mut self) {
        self.create_tables().await;

        while let Some(batch) = self.receiver.recv().await {
            // Coalesce ready batches to reduce ClickHouse round-trips.
            let mut coalesced = batch;
            for _ in 0..64 {
                match self.receiver.try_recv() {
                    Ok(extra) => {
                        coalesced.blocks.extend(extra.blocks);
                        coalesced.commits.extend(extra.commits);
                        coalesced.commit_info.extend(extra.commit_info);
                        coalesced.finalized_commits.extend(extra.finalized_commits);
                    }
                    Err(_) => break,
                }
            }

            self.insert_batch(coalesced).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::TestBlock;
    use crate::commit::TrustedCommit;
    use crate::leader_scoring::ReputationScores;
    use crate::storage::mem_store::MemStore;
    use consensus_config::AuthorityIndex;
    use consensus_types::block::BlockRef;

    fn make_test_block() -> VerifiedBlock {
        VerifiedBlock::new_for_test(
            TestBlock::new(1, 0)
                .set_ancestors(vec![BlockRef::MIN])
                .build(),
        )
    }

    fn make_test_commit() -> TrustedCommit {
        TrustedCommit::new_for_test(
            1,
            Default::default(),
            1000,
            BlockRef::MIN,
            vec![BlockRef::MIN],
        )
    }

    #[test]
    fn test_dual_write_store_forwards_to_inner() {
        let mem_store = Arc::new(MemStore::new());
        let (sender, mut receiver) = mpsc::channel(16);
        let dual = DualWriteStore::new(mem_store.clone(), sender, 42, false);

        let block = make_test_block();
        let block_ref = block.reference();

        let batch = WriteBatch::new(vec![block], vec![], vec![], vec![]);
        dual.write(batch).unwrap();

        // Verify inner store received the block.
        let read = mem_store.read_blocks(&[block_ref]).unwrap();
        assert!(read[0].is_some());

        // Verify channel received extracted metadata.
        let ch_batch = receiver.try_recv().unwrap();
        assert_eq!(ch_batch.blocks.len(), 1);
        assert_eq!(ch_batch.blocks[0].round, 1);
        assert_eq!(ch_batch.blocks[0].author, 0);
        // Block epoch comes from the block itself (0 for test blocks).
        assert_eq!(ch_batch.blocks[0].epoch, 0);
    }

    #[test]
    fn test_dual_write_store_channel_full_does_not_block() {
        let mem_store = Arc::new(MemStore::new());
        let (sender, _receiver) = mpsc::channel(1);
        let dual = DualWriteStore::new(mem_store.clone(), sender, 0, false);

        // Fill the channel.
        let block1 = make_test_block();
        let batch1 = WriteBatch::new(vec![block1], vec![], vec![], vec![]);
        dual.write(batch1).unwrap();

        // Second write should succeed (inner store) even though channel is full.
        let block2 = make_test_block();
        let block2_ref = block2.reference();
        let batch2 = WriteBatch::new(vec![block2], vec![], vec![], vec![]);
        dual.write(batch2).unwrap();

        // Inner store still has the block.
        let read = mem_store.read_blocks(&[block2_ref]).unwrap();
        assert!(read[0].is_some());
    }

    #[test]
    fn test_metadata_extraction_blocks() {
        let mem_store = Arc::new(MemStore::new());
        let (sender, mut receiver) = mpsc::channel(16);
        let dual = DualWriteStore::new(mem_store, sender, 10, false);

        let block = make_test_block();
        let batch = WriteBatch::new(vec![block.clone()], vec![], vec![], vec![]);
        dual.write(batch).unwrap();

        let ch_batch = receiver.try_recv().unwrap();
        let row = &ch_batch.blocks[0];
        assert_eq!(row.epoch, block.epoch());
        assert_eq!(row.round, block.round());
        assert_eq!(row.author, block.author().value() as u32);
        assert_eq!(row.digest, hex_encode(&block.digest().0));
        assert_eq!(row.num_transactions, block.transactions().len() as u32);
        assert_eq!(row.num_ancestors, block.ancestors().len() as u32);
        assert_eq!(row.serialized_size_bytes, block.serialized().len() as u32);
        assert!(row.raw_bytes.is_empty());
    }

    #[test]
    fn test_metadata_extraction_with_raw_bytes() {
        let mem_store = Arc::new(MemStore::new());
        let (sender, mut receiver) = mpsc::channel(16);
        let dual = DualWriteStore::new(mem_store, sender, 10, true);

        let block = make_test_block();
        let expected_bytes = block.serialized().to_vec();
        let batch = WriteBatch::new(vec![block], vec![], vec![], vec![]);
        dual.write(batch).unwrap();

        let ch_batch = receiver.try_recv().unwrap();
        assert_eq!(ch_batch.blocks[0].raw_bytes, expected_bytes);
    }

    #[test]
    fn test_metadata_extraction_commits() {
        let mem_store = Arc::new(MemStore::new());
        let (sender, mut receiver) = mpsc::channel(16);
        let dual = DualWriteStore::new(mem_store, sender, 5, false);

        let commit = make_test_commit();
        let batch = WriteBatch::new(vec![], vec![commit.clone()], vec![], vec![]);
        dual.write(batch).unwrap();

        let ch_batch = receiver.try_recv().unwrap();
        assert_eq!(ch_batch.commits.len(), 1);
        let row = &ch_batch.commits[0];
        assert_eq!(row.epoch, 5);
        assert_eq!(row.commit_index, commit.index());
        assert_eq!(row.num_blocks, commit.blocks().len() as u32);
    }

    #[test]
    fn test_metadata_extraction_commit_info() {
        let mem_store = Arc::new(MemStore::new());
        let (sender, mut receiver) = mpsc::channel(16);
        let dual = DualWriteStore::new(mem_store, sender, 7, false);

        let commit_ref = CommitRef::new(3, Default::default());
        let scores = ReputationScores::new((1..=10).into(), vec![100, 200, 300]);
        let info = CommitInfo {
            committed_rounds: vec![1, 2, 3],
            reputation_scores: scores,
        };

        let batch = WriteBatch::new(vec![], vec![], vec![(commit_ref, info)], vec![]);
        dual.write(batch).unwrap();

        let ch_batch = receiver.try_recv().unwrap();
        assert_eq!(ch_batch.commit_info.len(), 1);
        let row = &ch_batch.commit_info[0];
        assert_eq!(row.epoch, 7);
        assert_eq!(row.commit_index, 3);
        assert_eq!(row.committed_rounds, vec![1, 2, 3]);
        assert_eq!(row.reputation_scores, vec![100, 200, 300]);
        assert_eq!(row.score_commit_range_start, 1);
        assert_eq!(row.score_commit_range_end, 10);
    }

    #[test]
    fn test_metadata_extraction_finalized_commits() {
        let mem_store = Arc::new(MemStore::new());
        let (sender, mut receiver) = mpsc::channel(16);
        let dual = DualWriteStore::new(mem_store, sender, 1, false);

        let commit_ref = CommitRef::new(5, Default::default());
        let mut rejected = BTreeMap::new();
        rejected.insert(BlockRef::MIN, vec![0, 2, 4]);
        rejected.insert(
            BlockRef::new(1, AuthorityIndex::ZERO, Default::default()),
            vec![1],
        );

        let batch = WriteBatch::new(vec![], vec![], vec![], vec![(commit_ref, rejected)]);
        dual.write(batch).unwrap();

        let ch_batch = receiver.try_recv().unwrap();
        assert_eq!(ch_batch.finalized_commits.len(), 1);
        let row = &ch_batch.finalized_commits[0];
        assert_eq!(row.commit_index, 5);
        assert_eq!(row.num_blocks_with_rejections, 2);
        assert_eq!(row.total_rejected_transactions, 4);
    }

    #[test]
    fn test_empty_batch_no_channel_send() {
        let mem_store = Arc::new(MemStore::new());
        let (sender, mut receiver) = mpsc::channel(16);
        let dual = DualWriteStore::new(mem_store, sender, 0, false);

        let batch = WriteBatch::default();
        dual.write(batch).unwrap();

        assert!(receiver.try_recv().is_err());
    }
}
