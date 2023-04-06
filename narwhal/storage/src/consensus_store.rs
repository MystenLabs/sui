// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use config::AuthorityIdentifier;
use std::collections::HashMap;
use store::rocks::DBMap;
use store::{Map, TypedStoreError};
use types::{
    CommittedSubDag, CommittedSubDagShell, CompressedCommittedSubDag, CompressedCommittedSubDagV2,
    Round, SequenceNumber, StoreResult,
};

/// The persistent storage of the sequencer.
pub struct ConsensusStore {
    /// The latest committed round of each validator.
    last_committed: DBMap<AuthorityIdentifier, Round>,
    /// TODO: remove once released to validators
    /// The global consensus sequence.
    committed_sub_dags_by_index: DBMap<SequenceNumber, CommittedSubDagShell>,
    /// The global consensus sequence
    committed_sub_dags_by_index_v2: DBMap<SequenceNumber, CompressedCommittedSubDag>,
}

impl ConsensusStore {
    /// Create a new consensus store structure by using already loaded maps.
    pub fn new(
        last_committed: DBMap<AuthorityIdentifier, Round>,
        sequence: DBMap<SequenceNumber, CommittedSubDagShell>,
        committed_sub_dags_map: DBMap<SequenceNumber, CompressedCommittedSubDag>,
    ) -> Self {
        Self {
            last_committed,
            committed_sub_dags_by_index: sequence,
            committed_sub_dags_by_index_v2: committed_sub_dags_map,
        }
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
        let compressed =
            CompressedCommittedSubDag::V2(CompressedCommittedSubDagV2::from_sub_dag(sub_dag));

        let mut write_batch = self.last_committed.batch();
        write_batch.insert_batch(&self.last_committed, last_committed.iter())?;
        write_batch.insert_batch(
            &self.committed_sub_dags_by_index_v2,
            std::iter::once((sub_dag.sub_dag_index, compressed)),
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
    pub fn get_latest_sub_dag(&self) -> Option<CompressedCommittedSubDag> {
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
            .map(|(_, sub_dag)| CompressedCommittedSubDag::V1(sub_dag))
    }

    /// Load all the sub dags committed with sequence number of at least `from`.
    pub fn read_committed_sub_dags_from(
        &self,
        from: &SequenceNumber,
    ) -> StoreResult<Vec<CompressedCommittedSubDag>> {
        // TODO: remove once this has been released to the validators
        // start from the previous table first to ensure we haven't missed anything.
        let mut sub_dags = self
            .committed_sub_dags_by_index
            .iter()
            .skip_to(from)?
            .map(|(_, sub_dag)| CompressedCommittedSubDag::V1(sub_dag))
            .collect::<Vec<CompressedCommittedSubDag>>();

        sub_dags.extend(
            self.committed_sub_dags_by_index_v2
                .iter()
                .skip_to(from)?
                .map(|(_, sub_dag)| sub_dag)
                .collect::<Vec<CompressedCommittedSubDag>>(),
        );

        Ok(sub_dags)
    }
}
