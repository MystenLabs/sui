// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::fmt::Display;

use consensus_core::{BlockAPI, CommitDigest};
use sui_protocol_config::ProtocolConfig;
use sui_types::{digests::ConsensusCommitDigest, messages_consensus::ConsensusTransaction};

use crate::consensus_types::AuthorityIndex;

/// A list of tuples of:
/// (block origin authority index, all transactions contained in the block).
/// For each transaction, returns deserialized transaction and its serialized size.
type ConsensusOutputTransactions = Vec<(AuthorityIndex, Vec<(ConsensusTransaction, usize)>)>;

pub(crate) trait ConsensusOutputAPI: Display {
    fn reputation_score_sorted_desc(&self) -> Option<Vec<(AuthorityIndex, u64)>>;
    fn leader_round(&self) -> u64;
    fn leader_author_index(&self) -> AuthorityIndex;

    /// Returns epoch UNIX timestamp in milliseconds
    fn commit_timestamp_ms(&self) -> u64;

    /// Returns a unique global index for each committed sub-dag.
    fn commit_sub_dag_index(&self) -> u64;

    /// Returns all transactions in the commit.
    fn transactions(&self) -> ConsensusOutputTransactions;

    /// Returns the digest of consensus output.
    fn consensus_digest(&self, protocol_config: &ProtocolConfig) -> ConsensusCommitDigest;
}

impl ConsensusOutputAPI for consensus_core::CommittedSubDag {
    fn reputation_score_sorted_desc(&self) -> Option<Vec<(AuthorityIndex, u64)>> {
        if !self.reputation_scores_desc.is_empty() {
            Some(
                self.reputation_scores_desc
                    .iter()
                    .map(|(id, score)| (id.value() as AuthorityIndex, *score))
                    .collect(),
            )
        } else {
            None
        }
    }

    fn leader_round(&self) -> u64 {
        self.leader.round as u64
    }

    fn leader_author_index(&self) -> AuthorityIndex {
        self.leader.author.value() as AuthorityIndex
    }

    fn commit_timestamp_ms(&self) -> u64 {
        // TODO: Enforce ordered timestamp in Mysticeti.
        self.timestamp_ms
    }

    fn commit_sub_dag_index(&self) -> u64 {
        self.commit_ref.index.into()
    }

    fn transactions(&self) -> ConsensusOutputTransactions {
        self.blocks
            .iter()
            .map(|block| {
                let round = block.round();
                let author = block.author().value() as AuthorityIndex;
                let transactions: Vec<_> = block
                    .transactions()
                    .iter()
                    .flat_map(|tx| {
                        let transaction = bcs::from_bytes::<ConsensusTransaction>(tx.data());
                        match transaction {
                            Ok(transaction) => Some((
                                transaction,
                                tx.data().len(),
                            )),
                            Err(err) => {
                                tracing::error!("Failed to deserialize sequenced consensus transaction(this should not happen) {} from {author} at {round}", err);
                                None
                            },
                        }
                    })
                    .collect();
                (author, transactions)
            })
            .collect()
    }

    fn consensus_digest(&self, protocol_config: &ProtocolConfig) -> ConsensusCommitDigest {
        if protocol_config.mysticeti_use_committed_subdag_digest() {
            // We port CommitDigest, a consensus space object, into ConsensusCommitDigest, a sui-core space object.
            // We assume they always have the same format.
            static_assertions::assert_eq_size!(ConsensusCommitDigest, CommitDigest);
            ConsensusCommitDigest::new(self.commit_ref.digest.into_inner())
        } else {
            ConsensusCommitDigest::default()
        }
    }
}
