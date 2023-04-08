// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{NodeStorage, StoreResult};
use config::AuthorityIdentifier;
use std::collections::HashMap;
use store::rocks::{open_cf, DBMap, MetricConf, ReadWriteOptions};
use store::{reopen, Map, TypedStoreError};
use types::{
    CommittedSubDag, CommittedSubDagShell, ConsensusCommit, ConsensusCommitV2, Round,
    SequenceNumber,
};

/// The persistent storage of the sequencer.
pub struct ConsensusStore {
    /// The latest committed round of each validator.
    last_committed: DBMap<AuthorityIdentifier, Round>,
    /// TODO: remove once released to validators
    /// The global consensus sequence.
    committed_sub_dags_by_index: DBMap<SequenceNumber, CommittedSubDagShell>,
    /// The global consensus sequence
    committed_sub_dags_by_index_v2: DBMap<SequenceNumber, ConsensusCommit>,
}

impl ConsensusStore {
    /// Create a new consensus store structure by using already loaded maps.
    pub fn new(
        last_committed: DBMap<AuthorityIdentifier, Round>,
        sequence: DBMap<SequenceNumber, CommittedSubDagShell>,
        committed_sub_dags_map: DBMap<SequenceNumber, ConsensusCommit>,
    ) -> Self {
        Self {
            last_committed,
            committed_sub_dags_by_index: sequence,
            committed_sub_dags_by_index_v2: committed_sub_dags_map,
        }
    }

    pub fn new_for_tests() -> Self {
        let rocksdb = open_cf(
            tempfile::tempdir().unwrap(),
            None,
            MetricConf::default(),
            &[
                NodeStorage::LAST_COMMITTED_CF,
                NodeStorage::SUB_DAG_INDEX_CF,
                NodeStorage::COMMITTED_SUB_DAG_INDEX_CF,
            ],
        )
        .expect("Cannot open database");
        let (last_committed_map, sub_dag_index_map, committed_sub_dag_map) = reopen!(&rocksdb, NodeStorage::LAST_COMMITTED_CF;<AuthorityIdentifier, Round>, NodeStorage::SUB_DAG_INDEX_CF;<SequenceNumber, CommittedSubDagShell>, NodeStorage::COMMITTED_SUB_DAG_INDEX_CF;<SequenceNumber, ConsensusCommit>);
        Self::new(last_committed_map, sub_dag_index_map, committed_sub_dag_map)
    }

    /// Clear the store.
    pub fn clear(&self) -> StoreResult<()> {
        self.last_committed.clear()?;
        self.committed_sub_dags_by_index.clear()?;
        self.committed_sub_dags_by_index_v2.clear()?;
        Ok(())
    }

    /// Persist the consensus state.
    pub fn write_consensus_state(
        &self,
        last_committed: &HashMap<AuthorityIdentifier, Round>,
        sub_dag: &CommittedSubDag,
    ) -> Result<(), TypedStoreError> {
        let commit = ConsensusCommit::V2(ConsensusCommitV2::from_sub_dag(sub_dag));

        let mut write_batch = self.last_committed.batch();
        write_batch.insert_batch(&self.last_committed, last_committed.iter())?;
        write_batch.insert_batch(
            &self.committed_sub_dags_by_index_v2,
            std::iter::once((sub_dag.sub_dag_index, commit)),
        )?;
        write_batch.write()
    }

    /// Load the last committed round of each validator.
    pub fn read_last_committed(&self) -> HashMap<AuthorityIdentifier, Round> {
        self.last_committed.iter().collect()
    }

    /// Gets the latest sub dag index from the store
    pub fn get_latest_sub_dag_index(&self) -> SequenceNumber {
        if let Some(s) = self
            .committed_sub_dags_by_index_v2
            .iter()
            .skip_to_last()
            .next()
            .map(|(seq, _)| seq)
        {
            return s;
        }

        // TODO: remove once this has been released to the validators
        // If nothing has been found on v2, just fallback on the previous storage
        self.committed_sub_dags_by_index
            .iter()
            .skip_to_last()
            .next()
            .map(|(seq, _)| seq)
            .unwrap_or_default()
    }

    /// Returns thet latest subdag committed. If none is committed yet, then
    /// None is returned instead.
    pub fn get_latest_sub_dag(&self) -> Option<ConsensusCommit> {
        if let Some(sub_dag) = self
            .committed_sub_dags_by_index_v2
            .iter()
            .skip_to_last()
            .next()
            .map(|(_, sub_dag)| sub_dag)
        {
            return Some(sub_dag);
        }

        // TODO: remove once this has been released to the validators
        // If nothing has been found to the v2 table, just fallback to the previous one. We expect this
        // to happen only after validator has upgraded. After that point the v2 table will populated
        // and an entry should be found there.
        self.committed_sub_dags_by_index
            .iter()
            .skip_to_last()
            .next()
            .map(|(_, sub_dag)| ConsensusCommit::V1(sub_dag))
    }

    /// Load all the sub dags committed with sequence number of at least `from`.
    pub fn read_committed_sub_dags_from(
        &self,
        from: &SequenceNumber,
    ) -> StoreResult<Vec<ConsensusCommit>> {
        // TODO: remove once this has been released to the validators
        // start from the previous table first to ensure we haven't missed anything.
        let mut sub_dags = self
            .committed_sub_dags_by_index
            .iter()
            .skip_to(from)?
            .map(|(_, sub_dag)| ConsensusCommit::V1(sub_dag))
            .collect::<Vec<ConsensusCommit>>();

        sub_dags.extend(
            self.committed_sub_dags_by_index_v2
                .iter()
                .skip_to(from)?
                .map(|(_, sub_dag)| sub_dag)
                .collect::<Vec<ConsensusCommit>>(),
        );

        Ok(sub_dags)
    }
}

#[cfg(test)]
mod test {
    use crate::ConsensusStore;
    use store::Map;
    use types::{CommittedSubDagShell, ConsensusCommit, ConsensusCommitV2, TimestampMs};

    #[tokio::test]
    async fn test_v1_v2_backwards_compatibility() {
        let store = ConsensusStore::new_for_tests();

        // Create few sub dags of V1 and write in the committed_sub_dags_by_index storage
        for i in 0..3 {
            let s = CommittedSubDagShell {
                certificates: vec![],
                leader: Default::default(),
                leader_round: 2,
                sub_dag_index: i,
                reputation_score: Default::default(),
            };

            store
                .committed_sub_dags_by_index
                .insert(&s.sub_dag_index, &s)
                .unwrap();
        }

        // Create few sub dags of V2 and write in the committed_sub_dags_by_index_v2 storage
        for i in 3..6 {
            let s = ConsensusCommitV2 {
                certificates: vec![],
                leader: Default::default(),
                leader_round: 2,
                sub_dag_index: i,
                reputation_score: Default::default(),
                commit_timestamp: i,
            };

            store
                .committed_sub_dags_by_index_v2
                .insert(&s.sub_dag_index.clone(), &ConsensusCommit::V2(s))
                .unwrap();
        }

        // Read from index 0, all the sub dags should be returned
        let sub_dags = store.read_committed_sub_dags_from(&0).unwrap();

        assert_eq!(sub_dags.len(), 6);

        for (index, sub_dag) in sub_dags.iter().enumerate() {
            assert_eq!(sub_dag.sub_dag_index(), index as u64);
            if index < 3 {
                assert_eq!(sub_dag.commit_timestamp(), 0);
            } else {
                assert_eq!(sub_dag.commit_timestamp(), index as TimestampMs);
            }
        }

        // Read the last sub dag, and the sub dag with index 5 should be returned
        let last_sub_dag = store.get_latest_sub_dag();
        assert_eq!(last_sub_dag.unwrap().sub_dag_index(), 5);

        // Read the last sub dag index
        let index = store.get_latest_sub_dag_index();
        assert_eq!(index, 5);
    }
}
