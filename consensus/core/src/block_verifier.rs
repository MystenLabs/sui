// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, sync::Arc};

use crate::{
    block::{
        genesis_blocks, BlockAPI, BlockRef, BlockTimestampMs, SignedBlock, VerifiedBlock,
        GENESIS_ROUND,
    },
    context::Context,
    error::{ConsensusError, ConsensusResult},
    transaction::TransactionVerifier,
    Round, TransactionIndex,
};

pub(crate) trait BlockVerifier: Send + Sync + 'static {
    /// Verifies a block and its transactions, checking signatures, size limits,
    /// and transaction validity. All honest validators should produce the same verification
    /// outcome for the same block, so any verification error should be due to equivocation.
    ///
    /// When Mysticeti fastpath is enabled, it also votes on the transactions in verified blocks,
    /// and can return a non-empty list of rejected transaction indices. Different honest
    /// validators may vote differently on transactions.
    fn verify_and_vote(&self, block: &SignedBlock) -> ConsensusResult<Vec<TransactionIndex>>;

    /// Verifies a block w.r.t. ancestor blocks.
    /// This is called after a block has complete causal history locally,
    /// and is ready to be accepted into the DAG.
    ///
    /// Caller must make sure ancestors corresponse to block.ancestors() 1-to-1, in the same order.
    fn check_ancestors(
        &self,
        block: &VerifiedBlock,
        ancestors: &[Option<VerifiedBlock>],
        gc_enabled: bool,
        gc_round: Round,
    ) -> ConsensusResult<()>;
}

/// `SignedBlockVerifier` checks the validity of a block.
///
/// Blocks that fail verification at one honest authority will be rejected by all other honest
/// authorities as well. The means invalid blocks, and blocks with an invalid ancestor, will never
/// be accepted into the DAG.
pub(crate) struct SignedBlockVerifier {
    context: Arc<Context>,
    genesis: BTreeSet<BlockRef>,
    transaction_verifier: Arc<dyn TransactionVerifier>,
}

impl SignedBlockVerifier {
    pub(crate) fn new(
        context: Arc<Context>,
        transaction_verifier: Arc<dyn TransactionVerifier>,
    ) -> Self {
        let genesis = genesis_blocks(context.clone())
            .into_iter()
            .map(|b| b.reference())
            .collect();
        Self {
            context,
            genesis,
            transaction_verifier,
        }
    }

    fn verify_block(&self, block: &SignedBlock) -> ConsensusResult<()> {
        let committee = &self.context.committee;
        // The block must belong to the current epoch and have valid authority index,
        // before having its signature verified.
        if block.epoch() != committee.epoch() {
            return Err(ConsensusError::WrongEpoch {
                expected: committee.epoch(),
                actual: block.epoch(),
            });
        }
        if block.round() == 0 {
            return Err(ConsensusError::UnexpectedGenesisBlock);
        }
        if !committee.is_valid_index(block.author()) {
            return Err(ConsensusError::InvalidAuthorityIndex {
                index: block.author(),
                max: committee.size() - 1,
            });
        }

        // Verify the block's signature.
        block.verify_signature(&self.context)?;

        // Verify the block's ancestor refs are consistent with the block's round,
        // and total parent stakes reach quorum.
        if block.ancestors().len() > committee.size() {
            return Err(ConsensusError::TooManyAncestors(
                block.ancestors().len(),
                committee.size(),
            ));
        }
        if block.ancestors().is_empty() {
            return Err(ConsensusError::InsufficientParentStakes {
                parent_stakes: 0,
                quorum: committee.quorum_threshold(),
            });
        }
        let mut seen_ancestors = vec![false; committee.size()];
        let mut parent_stakes = 0;
        for (i, ancestor) in block.ancestors().iter().enumerate() {
            if !committee.is_valid_index(ancestor.author) {
                return Err(ConsensusError::InvalidAuthorityIndex {
                    index: ancestor.author,
                    max: committee.size() - 1,
                });
            }
            if (i == 0 && ancestor.author != block.author())
                || (i > 0 && ancestor.author == block.author())
            {
                return Err(ConsensusError::InvalidAncestorPosition {
                    block_authority: block.author(),
                    ancestor_authority: ancestor.author,
                    position: i,
                });
            }
            if ancestor.round >= block.round() {
                return Err(ConsensusError::InvalidAncestorRound {
                    ancestor: ancestor.round,
                    block: block.round(),
                });
            }
            if ancestor.round == GENESIS_ROUND && !self.genesis.contains(ancestor) {
                return Err(ConsensusError::InvalidGenesisAncestor(*ancestor));
            }
            if seen_ancestors[ancestor.author] {
                return Err(ConsensusError::DuplicatedAncestorsAuthority(
                    ancestor.author,
                ));
            }
            seen_ancestors[ancestor.author] = true;
            // Block must have round >= 1 so checked_sub(1) should be safe.
            if ancestor.round == block.round().checked_sub(1).unwrap() {
                parent_stakes += committee.stake(ancestor.author);
            }
        }
        if !committee.reached_quorum(parent_stakes) {
            return Err(ConsensusError::InsufficientParentStakes {
                parent_stakes,
                quorum: committee.quorum_threshold(),
            });
        }

        let batch: Vec<_> = block.transactions().iter().map(|t| t.data()).collect();

        self.check_transactions(&batch)
    }

    pub(crate) fn check_transactions(&self, batch: &[&[u8]]) -> ConsensusResult<()> {
        let max_transaction_size_limit =
            self.context.protocol_config.max_transaction_size_bytes() as usize;
        for t in batch {
            if t.len() > max_transaction_size_limit && max_transaction_size_limit > 0 {
                return Err(ConsensusError::TransactionTooLarge {
                    size: t.len(),
                    limit: max_transaction_size_limit,
                });
            }
        }

        let max_num_transactions_limit =
            self.context.protocol_config.max_num_transactions_in_block() as usize;
        if batch.len() > max_num_transactions_limit && max_num_transactions_limit > 0 {
            return Err(ConsensusError::TooManyTransactions {
                count: batch.len(),
                limit: max_num_transactions_limit,
            });
        }

        let total_transactions_size_limit = self
            .context
            .protocol_config
            .max_transactions_in_block_bytes() as usize;
        if batch.iter().map(|t| t.len()).sum::<usize>() > total_transactions_size_limit
            && total_transactions_size_limit > 0
        {
            return Err(ConsensusError::TooManyTransactionBytes {
                size: batch.len(),
                limit: total_transactions_size_limit,
            });
        }
        Ok(())
    }
}

// All block verification logic are implemented below.
impl BlockVerifier for SignedBlockVerifier {
    fn verify_and_vote(&self, block: &SignedBlock) -> ConsensusResult<Vec<TransactionIndex>> {
        self.verify_block(block)?;
        if self.context.protocol_config.mysticeti_fastpath() {
            self.transaction_verifier
                .verify_and_vote_batch(&block.transactions_data())
                .map_err(|e| ConsensusError::InvalidTransaction(e.to_string()))
        } else {
            self.transaction_verifier
                .verify_batch(&block.transactions_data())
                .map_err(|e| ConsensusError::InvalidTransaction(e.to_string()))?;
            Ok(vec![])
        }
    }

    fn check_ancestors(
        &self,
        block: &VerifiedBlock,
        ancestors: &[Option<VerifiedBlock>],
        gc_enabled: bool,
        gc_round: Round,
    ) -> ConsensusResult<()> {
        if gc_enabled {
            // TODO: will be removed with new timestamp calculation is in place as all these will be irrelevant.
            // When gc is enabled we don't have guarantees that all ancestors will be available. We'll take into account only the passed gc_round ones
            // for the timestamp check.
            let mut max_timestamp_ms = BlockTimestampMs::MIN;
            for ancestor in ancestors.iter().flatten() {
                if ancestor.round() <= gc_round {
                    continue;
                }
                max_timestamp_ms = max_timestamp_ms.max(ancestor.timestamp_ms());
                if max_timestamp_ms > block.timestamp_ms() {
                    return Err(ConsensusError::InvalidBlockTimestamp {
                        max_timestamp_ms,
                        block_timestamp_ms: block.timestamp_ms(),
                    });
                }
            }
        } else {
            assert_eq!(block.ancestors().len(), ancestors.len());
            // This checks the invariant that block timestamp >= max ancestor timestamp.
            let mut max_timestamp_ms = BlockTimestampMs::MIN;
            for (ancestor_ref, ancestor_block) in block.ancestors().iter().zip(ancestors.iter()) {
                let ancestor_block = ancestor_block
                    .as_ref()
                    .expect("There should never be an empty slot");
                assert_eq!(ancestor_ref, &ancestor_block.reference());
                max_timestamp_ms = max_timestamp_ms.max(ancestor_block.timestamp_ms());
            }
            if max_timestamp_ms > block.timestamp_ms() {
                return Err(ConsensusError::InvalidBlockTimestamp {
                    max_timestamp_ms,
                    block_timestamp_ms: block.timestamp_ms(),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) struct NoopBlockVerifier;

#[cfg(test)]
impl BlockVerifier for NoopBlockVerifier {
    fn verify_and_vote(&self, _block: &SignedBlock) -> ConsensusResult<Vec<TransactionIndex>> {
        Ok(vec![])
    }

    fn check_ancestors(
        &self,
        _block: &VerifiedBlock,
        _ancestors: &[Option<VerifiedBlock>],
        _gc_enabled: bool,
        _gc_round: Round,
    ) -> ConsensusResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use consensus_config::AuthorityIndex;
    use rstest::rstest;
    use sui_protocol_config::ProtocolConfig;

    use super::*;
    use crate::{
        block::{BlockDigest, BlockRef, TestBlock, Transaction, TransactionIndex},
        context::Context,
        transaction::{TransactionVerifier, ValidationError},
    };

    struct TxnSizeVerifier {}

    impl TransactionVerifier for TxnSizeVerifier {
        // Fails verification if any transaction is < 4 bytes.
        fn verify_batch(&self, transactions: &[&[u8]]) -> Result<(), ValidationError> {
            for txn in transactions {
                if txn.len() < 4 {
                    return Err(ValidationError::InvalidTransaction(format!(
                        "Length {} is too short!",
                        txn.len()
                    )));
                }
            }
            Ok(())
        }

        // Fails verification if any transaction is < 4 bytes.
        // Rejects transactions with length [4, 16) bytes.
        fn verify_and_vote_batch(
            &self,
            batch: &[&[u8]],
        ) -> Result<Vec<TransactionIndex>, ValidationError> {
            let mut rejected_indices = vec![];
            for (i, txn) in batch.iter().enumerate() {
                if txn.len() < 4 {
                    return Err(ValidationError::InvalidTransaction(format!(
                        "Length {} is too short!",
                        txn.len()
                    )));
                }
                if txn.len() < 16 {
                    rejected_indices.push(i as TransactionIndex);
                }
            }
            Ok(rejected_indices)
        }
    }

    #[tokio::test]
    async fn test_verify_block() {
        let (context, keypairs) = Context::new_for_test(4);
        let context = Arc::new(context);
        const AUTHOR: u32 = 2;
        let author_protocol_keypair = &keypairs[AUTHOR as usize].1;
        let verifier = SignedBlockVerifier::new(context.clone(), Arc::new(TxnSizeVerifier {}));

        let test_block = TestBlock::new(10, AUTHOR)
            .set_ancestors(vec![
                BlockRef::new(9, AuthorityIndex::new_for_test(2), BlockDigest::MIN),
                BlockRef::new(9, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
                BlockRef::new(9, AuthorityIndex::new_for_test(1), BlockDigest::MIN),
                BlockRef::new(7, AuthorityIndex::new_for_test(3), BlockDigest::MIN),
            ])
            .set_transactions(vec![Transaction::new(vec![4; 8])]);

        // Valid SignedBlock.
        {
            let block = test_block.clone().build();
            let signed_block = SignedBlock::new(block, author_protocol_keypair).unwrap();
            verifier.verify_block(&signed_block).unwrap();
        }

        // Block with wrong epoch.
        {
            let block = test_block.clone().set_epoch(1).build();
            let signed_block = SignedBlock::new(block, author_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify_block(&signed_block),
                Err(ConsensusError::WrongEpoch {
                    expected: _,
                    actual: _
                })
            ));
        }

        // Block at genesis round.
        {
            let block = test_block.clone().set_round(0).build();
            let signed_block = SignedBlock::new(block, author_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify_block(&signed_block),
                Err(ConsensusError::UnexpectedGenesisBlock)
            ));
        }

        // Block with invalid authority index.
        {
            let block = test_block
                .clone()
                .set_author(AuthorityIndex::new_for_test(4))
                .build();
            let signed_block = SignedBlock::new(block, author_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify_block(&signed_block),
                Err(ConsensusError::InvalidAuthorityIndex { index: _, max: _ })
            ));
        }

        // Block with mismatched authority index and signature.
        {
            let block = test_block
                .clone()
                .set_author(AuthorityIndex::new_for_test(1))
                .build();
            let signed_block = SignedBlock::new(block, author_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify_block(&signed_block),
                Err(ConsensusError::SignatureVerificationFailure(_))
            ));
        }

        // Block with wrong key.
        {
            let block = test_block.clone().build();
            let signed_block = SignedBlock::new(block, &keypairs[3].1).unwrap();
            assert!(matches!(
                verifier.verify_block(&signed_block),
                Err(ConsensusError::SignatureVerificationFailure(_))
            ));
        }

        // Block without signature.
        {
            let block = test_block.clone().build();
            let mut signed_block = SignedBlock::new(block, author_protocol_keypair).unwrap();
            signed_block.clear_signature();
            assert!(matches!(
                verifier.verify_block(&signed_block),
                Err(ConsensusError::MalformedSignature(_))
            ));
        }

        // Block with invalid ancestor round.
        {
            let block = test_block
                .clone()
                .set_ancestors(vec![
                    BlockRef::new(9, AuthorityIndex::new_for_test(2), BlockDigest::MIN),
                    BlockRef::new(9, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
                    BlockRef::new(9, AuthorityIndex::new_for_test(1), BlockDigest::MIN),
                    BlockRef::new(10, AuthorityIndex::new_for_test(3), BlockDigest::MIN),
                ])
                .build();
            let signed_block = SignedBlock::new(block, author_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify_block(&signed_block),
                Err(ConsensusError::InvalidAncestorRound {
                    ancestor: _,
                    block: _
                })
            ));
        }

        // Block with parents not reaching quorum.
        {
            let block = test_block
                .clone()
                .set_ancestors(vec![
                    BlockRef::new(9, AuthorityIndex::new_for_test(2), BlockDigest::MIN),
                    BlockRef::new(9, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
                    BlockRef::new(8, AuthorityIndex::new_for_test(1), BlockDigest::MIN),
                    BlockRef::new(8, AuthorityIndex::new_for_test(3), BlockDigest::MIN),
                ])
                .build();
            let signed_block = SignedBlock::new(block, author_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify_block(&signed_block),
                Err(ConsensusError::InsufficientParentStakes {
                    parent_stakes: _,
                    quorum: _
                })
            ));
        }

        // Block with too many ancestors.
        {
            let block = test_block
                .clone()
                .set_ancestors(vec![
                    BlockRef::new(9, AuthorityIndex::new_for_test(2), BlockDigest::MIN),
                    BlockRef::new(9, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
                    BlockRef::new(8, AuthorityIndex::new_for_test(1), BlockDigest::MIN),
                    BlockRef::new(8, AuthorityIndex::new_for_test(3), BlockDigest::MIN),
                    BlockRef::new(9, AuthorityIndex::new_for_test(3), BlockDigest::MIN),
                ])
                .build();
            let signed_block = SignedBlock::new(block, author_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify_block(&signed_block),
                Err(ConsensusError::TooManyAncestors(_, _))
            ));
        }

        // Block without own ancestor.
        {
            let block = test_block
                .clone()
                .set_ancestors(vec![
                    BlockRef::new(9, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
                    BlockRef::new(8, AuthorityIndex::new_for_test(1), BlockDigest::MIN),
                    BlockRef::new(8, AuthorityIndex::new_for_test(3), BlockDigest::MIN),
                ])
                .build();
            let signed_block = SignedBlock::new(block, author_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify_block(&signed_block),
                Err(ConsensusError::InvalidAncestorPosition {
                    block_authority: _,
                    ancestor_authority: _,
                    position: _
                })
            ));
        }

        // Block with own ancestor at wrong position.
        {
            let block = test_block
                .clone()
                .set_ancestors(vec![
                    BlockRef::new(9, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
                    BlockRef::new(8, AuthorityIndex::new_for_test(1), BlockDigest::MIN),
                    BlockRef::new(8, AuthorityIndex::new_for_test(2), BlockDigest::MIN),
                    BlockRef::new(8, AuthorityIndex::new_for_test(3), BlockDigest::MIN),
                ])
                .build();
            let signed_block = SignedBlock::new(block, author_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify_block(&signed_block),
                Err(ConsensusError::InvalidAncestorPosition {
                    block_authority: _,
                    ancestor_authority: _,
                    position: _
                })
            ));
        }

        // Block with ancestors from the same authority.
        {
            let block = test_block
                .clone()
                .set_ancestors(vec![
                    BlockRef::new(8, AuthorityIndex::new_for_test(2), BlockDigest::MIN),
                    BlockRef::new(8, AuthorityIndex::new_for_test(1), BlockDigest::MIN),
                    BlockRef::new(8, AuthorityIndex::new_for_test(1), BlockDigest::MIN),
                ])
                .build();
            let signed_block = SignedBlock::new(block, author_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify_block(&signed_block),
                Err(ConsensusError::DuplicatedAncestorsAuthority(_))
            ));
        }

        // Block with transaction too large.
        {
            let block = test_block
                .clone()
                .set_transactions(vec![Transaction::new(vec![4; 257 * 1024])])
                .build();
            let signed_block = SignedBlock::new(block, author_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify_block(&signed_block),
                Err(ConsensusError::TransactionTooLarge { size: _, limit: _ })
            ));
        }

        // Block with too many transactions.
        {
            let block = test_block
                .clone()
                .set_transactions((0..1000).map(|_| Transaction::new(vec![4; 8])).collect())
                .build();
            let signed_block = SignedBlock::new(block, author_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify_block(&signed_block),
                Err(ConsensusError::TooManyTransactions { count: _, limit: _ })
            ));
        }

        // Block with too many transaction bytes.
        {
            let block = test_block
                .clone()
                .set_transactions(
                    (0..100)
                        .map(|_| Transaction::new(vec![4; 8 * 1024]))
                        .collect(),
                )
                .build();
            let signed_block = SignedBlock::new(block, author_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify_block(&signed_block),
                Err(ConsensusError::TooManyTransactionBytes { size: _, limit: _ })
            ));
        }

        // Block with an invalid transaction.
        {
            let block = test_block
                .clone()
                .set_transactions(vec![
                    Transaction::new(vec![1; 4]),
                    Transaction::new(vec![1; 2]),
                ])
                .build();
            let signed_block = SignedBlock::new(block, author_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify_and_vote(&signed_block),
                Err(ConsensusError::InvalidTransaction(_))
            ));
        }
    }

    #[tokio::test]
    async fn test_verify_and_vote_transactions() {
        let mut protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
        protocol_config.set_mysticeti_fastpath_for_testing(true);

        let (context, keypairs) = Context::new_for_test(4);
        let context = Arc::new(context.with_protocol_config(protocol_config));

        const AUTHOR: u32 = 2;
        let author_protocol_keypair = &keypairs[AUTHOR as usize].1;
        let verifier = SignedBlockVerifier::new(context.clone(), Arc::new(TxnSizeVerifier {}));

        let base_block = TestBlock::new(10, AUTHOR).set_ancestors(vec![
            BlockRef::new(9, AuthorityIndex::new_for_test(2), BlockDigest::MIN),
            BlockRef::new(9, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
            BlockRef::new(9, AuthorityIndex::new_for_test(1), BlockDigest::MIN),
            BlockRef::new(7, AuthorityIndex::new_for_test(3), BlockDigest::MIN),
        ]);

        // Block with all transactions valid and accepted.
        {
            let block = base_block
                .clone()
                .set_transactions(vec![
                    Transaction::new(vec![1; 16]),
                    Transaction::new(vec![2; 16]),
                    Transaction::new(vec![3; 16]),
                    Transaction::new(vec![4; 16]),
                ])
                .build();
            let signed_block = SignedBlock::new(block, author_protocol_keypair).unwrap();
            assert_eq!(
                verifier.verify_and_vote(&signed_block).unwrap(),
                Vec::<TransactionIndex>::new()
            );
        }

        // Block with 2 transactions rejected.
        {
            let block = base_block
                .clone()
                .set_transactions(vec![
                    Transaction::new(vec![1; 16]),
                    Transaction::new(vec![2; 8]),
                    Transaction::new(vec![3; 16]),
                    Transaction::new(vec![4; 9]),
                ])
                .build();
            let signed_block = SignedBlock::new(block, author_protocol_keypair).unwrap();
            assert_eq!(
                verifier.verify_and_vote(&signed_block).unwrap(),
                vec![1 as TransactionIndex, 3 as TransactionIndex],
            );
        }

        // Block with an invalid transaction returns an error.
        {
            let block = base_block
                .clone()
                .set_transactions(vec![
                    Transaction::new(vec![1; 16]),
                    Transaction::new(vec![2; 8]),
                    Transaction::new(vec![3; 1]), // Invalid transaction size
                    Transaction::new(vec![4; 9]),
                ])
                .build();
            let signed_block = SignedBlock::new(block, author_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify_and_vote(&signed_block),
                Err(ConsensusError::InvalidTransaction(_))
            ));
        }
    }

    /// Tests the block's ancestors for timestamp monotonicity. Test will run for both when gc is enabled and disabled, but
    /// with none of the ancestors being below the gc_round.
    #[rstest]
    #[tokio::test]
    async fn test_check_ancestors(#[values(false, true)] gc_enabled: bool) {
        let num_authorities = 4;
        let (context, _keypairs) = Context::new_for_test(num_authorities);
        let context = Arc::new(context);
        let verifier = SignedBlockVerifier::new(context.clone(), Arc::new(TxnSizeVerifier {}));
        let gc_round = 0;

        let mut ancestor_blocks = vec![];
        for i in 0..num_authorities {
            let test_block = TestBlock::new(10, i as u32)
                .set_timestamp_ms(1000 + 100 * i as BlockTimestampMs)
                .build();
            ancestor_blocks.push(Some(VerifiedBlock::new_for_test(test_block)));
        }
        let ancestor_refs = ancestor_blocks
            .iter()
            .flatten()
            .map(|block| block.reference())
            .collect::<Vec<_>>();

        // Block respecting timestamp invariant.
        {
            let block = TestBlock::new(11, 0)
                .set_ancestors(ancestor_refs.clone())
                .set_timestamp_ms(1500)
                .build();
            let verified_block = VerifiedBlock::new_for_test(block);
            assert!(verifier
                .check_ancestors(&verified_block, &ancestor_blocks, gc_enabled, gc_round)
                .is_ok());
        }

        // Block not respecting timestamp invariant.
        {
            let block = TestBlock::new(11, 0)
                .set_ancestors(ancestor_refs.clone())
                .set_timestamp_ms(1000)
                .build();
            let verified_block = VerifiedBlock::new_for_test(block);
            assert!(matches!(
                verifier.check_ancestors(&verified_block, &ancestor_blocks, gc_enabled, gc_round),
                Err(ConsensusError::InvalidBlockTimestamp {
                    max_timestamp_ms: _,
                    block_timestamp_ms: _
                })
            ));
        }
    }

    #[tokio::test]
    async fn test_check_ancestors_passed_gc_round() {
        let num_authorities = 4;
        let (context, _keypairs) = Context::new_for_test(num_authorities);
        let context = Arc::new(context);
        let verifier = SignedBlockVerifier::new(context.clone(), Arc::new(TxnSizeVerifier {}));
        let gc_enabled = true;
        let gc_round = 3;

        let mut ancestor_blocks = vec![];

        // Create one block just on the `gc_round` (so it should be considered garbage collected). This has higher
        // timestamp that the block we are testing.
        let test_block = TestBlock::new(gc_round, 0_u32)
            .set_timestamp_ms(1500 as BlockTimestampMs)
            .build();
        ancestor_blocks.push(Some(VerifiedBlock::new_for_test(test_block)));

        // Rest of the blocks
        for i in 1..=3 {
            let test_block = TestBlock::new(gc_round + 1, i as u32)
                .set_timestamp_ms(1000 + 100 * i as BlockTimestampMs)
                .build();
            ancestor_blocks.push(Some(VerifiedBlock::new_for_test(test_block)));
        }

        let ancestor_refs = ancestor_blocks
            .iter()
            .flatten()
            .map(|block| block.reference())
            .collect::<Vec<_>>();

        // Block respecting timestamp invariant.
        {
            let block = TestBlock::new(gc_round + 2, 0)
                .set_ancestors(ancestor_refs.clone())
                .set_timestamp_ms(1600)
                .build();
            let verified_block = VerifiedBlock::new_for_test(block);
            assert!(verifier
                .check_ancestors(&verified_block, &ancestor_blocks, gc_enabled, gc_round)
                .is_ok());
        }

        // Block not respecting timestamp invariant for the block that is garbage collected
        // Validation should pass.
        {
            let block = TestBlock::new(11, 0)
                .set_ancestors(ancestor_refs.clone())
                .set_timestamp_ms(1400)
                .build();
            let verified_block = VerifiedBlock::new_for_test(block);
            assert!(verifier
                .check_ancestors(&verified_block, &ancestor_blocks, gc_enabled, gc_round)
                .is_ok());
        }

        // Block not respecting timestamp invariant for the blocks that are not garbage collected
        {
            let block = TestBlock::new(11, 0)
                .set_ancestors(ancestor_refs.clone())
                .set_timestamp_ms(1100)
                .build();
            let verified_block = VerifiedBlock::new_for_test(block);
            assert!(matches!(
                verifier.check_ancestors(&verified_block, &ancestor_blocks, gc_enabled, gc_round),
                Err(ConsensusError::InvalidBlockTimestamp {
                    max_timestamp_ms: _,
                    block_timestamp_ms: _
                })
            ));
        }
    }
}
