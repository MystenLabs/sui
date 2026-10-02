// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, fmt::Display};

use consensus_core::{BlockAPI, CommitDigest, CommitRef, VerifiedBlock};
use consensus_types::block::{BlockRef, TransactionIndex};
use fastcrypto::hash::HashFunction as _;
use itertools::Itertools as _;
use rand::{SeedableRng as _, rngs::StdRng, seq::SliceRandom as _};
use sui_types::{
    digests::Digest,
    messages_consensus::{AuthorityIndex, ConsensusTransaction},
};

pub(crate) struct ParsedTransaction {
    // Transaction from consensus output.
    pub(crate) transaction: ConsensusTransaction,
    // Whether the transaction was rejected in voting.
    pub(crate) rejected: bool,
    // Bytes length of the serialized transaction
    pub(crate) serialized_len: usize,
}

pub(crate) trait ConsensusCommitAPI: Display {
    /// Returns the ref of consensus output.
    fn commit_ref(&self) -> CommitRef;

    fn leader_round(&self) -> u64;
    fn leader_author_index(&self) -> AuthorityIndex;

    /// Returns epoch UNIX timestamp in milliseconds
    fn commit_timestamp_ms(&self) -> u64;

    /// Returns a unique global index for each committed sub-dag.
    fn commit_sub_dag_index(&self) -> u64;

    /// Returns all accepted and rejected transactions per block in the commit in deterministic order.
    fn transactions(&self) -> Vec<(BlockRef, Vec<ParsedTransaction>)>;

    /// Like `transactions()`, but with the block order shuffled by an RNG seeded from the commit
    /// digest. Transactions within each block keep their relative order, so soft bundles remain
    /// contiguous.
    fn shuffled_transactions(&self) -> Vec<(BlockRef, Vec<ParsedTransaction>)>;

    /// Returns a debug string of all rejected transactions.
    fn rejected_transactions_digest(&self) -> Digest;
    fn rejected_transactions_debug_string(&self) -> String;
}

impl ConsensusCommitAPI for consensus_core::CommittedSubDag {
    fn commit_ref(&self) -> CommitRef {
        self.commit_ref
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

    fn transactions(&self) -> Vec<(BlockRef, Vec<ParsedTransaction>)> {
        parse_sub_dag_blocks(self, self.blocks.iter())
    }

    fn shuffled_transactions(&self) -> Vec<(BlockRef, Vec<ParsedTransaction>)> {
        let mut blocks: Vec<&VerifiedBlock> = self.blocks.iter().collect();
        shuffle_blocks_by_commit_digest(self.commit_ref.digest, &mut blocks);
        parse_sub_dag_blocks(self, blocks.into_iter())
    }

    fn rejected_transactions_digest(&self) -> Digest {
        let mut hasher = sui_types::crypto::DefaultHash::new();
        bcs::serialize_into(&mut hasher, &self.rejected_transactions_by_block).unwrap();
        hasher.finalize().digest.into()
    }

    fn rejected_transactions_debug_string(&self) -> String {
        let str = self
            .rejected_transactions_by_block
            .iter()
            .map(|(block_ref, rejected_transactions)| {
                format!(
                    "{block_ref}: [{}]",
                    rejected_transactions
                        .iter()
                        .map(|tx| tx.to_string())
                        .join(",")
                )
            })
            .join(", ");
        let digest = self.rejected_transactions_digest();
        format!("({digest}): [{str}]")
    }
}

fn parse_sub_dag_blocks<'a>(
    sub_dag: &consensus_core::CommittedSubDag,
    blocks: impl Iterator<Item = &'a VerifiedBlock>,
) -> Vec<(BlockRef, Vec<ParsedTransaction>)> {
    let no_transaction = vec![];
    blocks
        .map(|block| {
            let rejected_transactions = sub_dag
                .rejected_transactions_by_block
                .get(&block.reference())
                .unwrap_or(&no_transaction);
            (
                block.reference(),
                parse_block_transactions(block, rejected_transactions),
            )
        })
        .collect()
}

/// Shuffles `blocks` in place using an RNG seeded from `commit_digest`, so every validator
/// derives the same order for the same commit. `StdRng` is already relied upon for
/// protocol-level determinism (see `Committee::shuffle_by_stake_from_tx_digest`).
pub(crate) fn shuffle_blocks_by_commit_digest<T>(commit_digest: CommitDigest, blocks: &mut [T]) {
    let mut rng = StdRng::from_seed(commit_digest.into_inner());
    blocks.shuffle(&mut rng);
}

pub(crate) fn parse_block_transactions(
    block: &VerifiedBlock,
    rejected_transactions: &[TransactionIndex],
) -> Vec<ParsedTransaction> {
    let round = block.round();
    let authority = block.author().value() as AuthorityIndex;

    let rejected_transaction_indices = BTreeSet::from_iter(rejected_transactions.iter().cloned());
    block
        .transactions()
        .iter().enumerate()
        .map(|(index, tx)| {
            let transaction = match bcs::from_bytes::<ConsensusTransaction>(tx.data()) {
                Ok(transaction) => transaction,
                Err(err) => {
                    panic!("Failed to deserialize sequenced consensus transaction(this should not happen) {err} from {authority} at {round}");
                },
            };
            // System transactions are always accepted; only user transactions can be rejected.
            let rejected = transaction.is_user_transaction() && rejected_transaction_indices.contains(&(index as TransactionIndex));
            ParsedTransaction {
                transaction,
                rejected,
                serialized_len: tx.data().len(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use consensus_core::{CommittedSubDag, TestBlock, Transaction};
    use sui_types::crypto::{AuthorityKeyPair, KeypairTraits as _, get_key_pair};

    use super::*;

    fn make_sub_dag(num_blocks: u8, txs_per_block: u8, digest: CommitDigest) -> CommittedSubDag {
        let (_, keypair): (_, AuthorityKeyPair) = get_key_pair();
        let authority = keypair.public().into();
        let blocks: Vec<VerifiedBlock> = (0..num_blocks)
            .map(|b| {
                let transactions = (0..txs_per_block)
                    .map(|t| {
                        let mut tx = ConsensusTransaction::new_end_of_publish(authority);
                        tx.tracking_id = [b, t, 0, 0, 0, 0, 0, 0];
                        Transaction::new(bcs::to_bytes(&tx).unwrap())
                    })
                    .collect();
                VerifiedBlock::new_for_test(
                    TestBlock::new(10, b as u32)
                        .set_transactions(transactions)
                        .build(),
                )
            })
            .collect();
        let leader = blocks[0].reference();
        CommittedSubDag::new(leader, blocks, 0, CommitRef::new(1, digest))
    }

    fn tracking_ids(
        parsed: &[(BlockRef, Vec<ParsedTransaction>)],
    ) -> Vec<(BlockRef, Vec<[u8; 8]>)> {
        parsed
            .iter()
            .map(|(block_ref, txs)| {
                (
                    *block_ref,
                    txs.iter().map(|tx| tx.transaction.tracking_id).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn shuffled_transactions_permutes_blocks_deterministically() {
        let digest_a = CommitDigest::MAX;
        let sub_dag = make_sub_dag(8, 3, digest_a);

        let unshuffled = tracking_ids(&sub_dag.transactions());
        let shuffled = tracking_ids(&sub_dag.shuffled_transactions());

        // Same seed, same order.
        assert_eq!(shuffled, tracking_ids(&sub_dag.shuffled_transactions()));
        // The block order actually changes, but each block's contents do not.
        assert_ne!(shuffled, unshuffled);
        let mut sorted_shuffled = shuffled.clone();
        sorted_shuffled.sort();
        let mut sorted_unshuffled = unshuffled.clone();
        sorted_unshuffled.sort();
        assert_eq!(sorted_shuffled, sorted_unshuffled);

        // Different commit digest, different order.
        let sub_dag_b = make_sub_dag(8, 3, CommitDigest::MIN);
        let shuffled_b = tracking_ids(&sub_dag_b.shuffled_transactions());
        assert_ne!(
            shuffled.iter().map(|(b, _)| *b).collect::<Vec<_>>(),
            shuffled_b.iter().map(|(b, _)| *b).collect::<Vec<_>>()
        );
    }
}
