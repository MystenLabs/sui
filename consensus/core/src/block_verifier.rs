// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bytes::Bytes;
use consensus_types::block::{BlockRef, NUM_RESERVED_TRANSACTION_INDICES, TransactionIndex};
use std::{collections::BTreeSet, sync::Arc};

use crate::{
    VerifiedBlock,
    block::{
        Block, BlockAPI, GENESIS_ROUND, SignedBlock, genesis_blocks, max_transaction_vote_targets,
    },
    context::Context,
    error::{ConsensusError, ConsensusResult},
    transaction::TransactionVerifier,
};

pub trait BlockVerifier: Send + Sync + 'static {
    /// Verifies a block and its transactions, checking signatures, size limits,
    /// and transaction validity. All honest validators should produce the same verification
    /// outcome for the same block, so any verification error should be due to equivocation.
    /// Returns the verified block.
    ///
    /// When Mysticeti fastpath is enabled, it also votes on the transactions in verified blocks,
    /// and can return a non-empty list of rejected transaction indices. Different honest
    /// validators may vote differently on transactions.
    ///
    /// The method takes both the SignedBlock and its serialized bytes, to avoid re-serializing the block.
    #[allow(private_interfaces)]
    fn verify_and_vote(
        &self,
        block: SignedBlock,
        serialized_block: Bytes,
    ) -> ConsensusResult<(VerifiedBlock, Vec<TransactionIndex>)>;

    /// Votes on the transactions in a verified block.
    /// This is used to vote on transactions in a verified block, without having to verify the block again. The method
    /// will verify the transactions and vote on them.
    fn vote(&self, block: &VerifiedBlock) -> ConsensusResult<Vec<TransactionIndex>>;
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
        let genesis = genesis_blocks(&context)
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
        // The block must belong to the current epoch, have the block version of the epoch and
        // have a valid authority index, before its signature is verified.
        if block.epoch() != committee.epoch() {
            return Err(ConsensusError::WrongEpoch {
                expected: committee.epoch(),
                actual: block.epoch(),
            });
        }
        if block.round() == 0 {
            return Err(ConsensusError::UnexpectedGenesisBlock);
        }
        // All authorities of an epoch must run with the same protocol config. So a block of
        // another version is invalid, even if it is otherwise well formed. The match keeps this
        // check complete when a new block version is added.
        let enable_v3 = self.context.protocol_config.enable_v3();
        let version_matches = match **block {
            Block::V1(_) | Block::V2(_) => !enable_v3,
            Block::V3(_) => enable_v3,
        };
        if !version_matches {
            return Err(ConsensusError::UnexpectedBlockVersion {
                version: if enable_v3 {
                    format!("block {} is not V3 but v3 is enabled", block.slot())
                } else {
                    format!("block {} is V3 but v3 is disabled", block.slot())
                },
            });
        }
        if !committee.is_valid_index(block.author()) {
            return Err(ConsensusError::InvalidAuthorityIndex {
                loc: format!("verifying block {}", block.slot()),
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
                    loc: format!("ancestor {}", ancestor),
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

        if enable_v3 {
            let cutoff = block.transaction_votes_cutoff_round();
            if cutoff >= block.round() {
                return Err(ConsensusError::InvalidTransactionVotesCutoff {
                    cutoff,
                    block: block.round(),
                });
            }
            self.check_transaction_votes(block)?;
        }

        let batch: Vec<_> = block.transactions().iter().map(|t| t.data()).collect();
        self.check_transactions(&batch)
    }

    fn check_transaction_votes(&self, block: &SignedBlock) -> ConsensusResult<()> {
        let transaction_votes = block.transaction_votes();
        if transaction_votes.is_empty() {
            return Ok(());
        }
        // This check runs before the allocation and the scans below, to keep them bounded.
        let vote_target_limit = max_transaction_vote_targets(&self.context);
        if transaction_votes.len() > vote_target_limit {
            return Err(ConsensusError::InvalidTransactionVotes(format!(
                "block at round {} from {} has {} vote targets but the limit is {}",
                block.round(),
                block.author(),
                transaction_votes.len(),
                vote_target_limit,
            )));
        }
        let cutoff_round = block.transaction_votes_cutoff_round();
        let transaction_limit = self
            .context
            .protocol_config
            .max_num_transactions_in_block()
            .min(
                TransactionIndex::MAX
                    .saturating_sub(NUM_RESERVED_TRANSACTION_INDICES)
                    .into(),
            ) as u16;

        let mut vote_targets = BTreeSet::new();
        for votes in transaction_votes {
            if !vote_targets.insert(votes.block_ref) {
                return Err(ConsensusError::InvalidTransactionVotes(format!(
                    "vote target {} appears more than once",
                    votes.block_ref,
                )));
            }
            if votes.block_ref.round >= block.round() {
                return Err(ConsensusError::InvalidTransactionVotes(format!(
                    "vote target {} must have a round less than block round {} from {}",
                    votes.block_ref,
                    block.round(),
                    block.author(),
                )));
            }
            if votes.block_ref.round <= cutoff_round {
                return Err(ConsensusError::InvalidTransactionVotes(format!(
                    "vote target {} is at or below cutoff round {}",
                    votes.block_ref, cutoff_round,
                )));
            }
            if !self
                .context
                .committee
                .is_valid_index(votes.block_ref.author)
            {
                return Err(ConsensusError::InvalidAuthorityIndex {
                    loc: format!("transaction vote block {}", votes.block_ref),
                    index: votes.block_ref.author,
                    max: self.context.committee.size() - 1,
                });
            }
            if votes.rejects.is_empty() {
                return Err(ConsensusError::InvalidTransactionVotes(format!(
                    "vote target {} has no reject indices",
                    votes.block_ref,
                )));
            }
            if votes.rejects.len() > transaction_limit as usize {
                return Err(ConsensusError::InvalidTransactionVotes(format!(
                    "vote target {} has {} reject indices but the limit is {}",
                    votes.block_ref,
                    votes.rejects.len(),
                    transaction_limit,
                )));
            }
            if votes.rejects.windows(2).any(|pair| pair[0] >= pair[1]) {
                return Err(ConsensusError::InvalidTransactionVotes(format!(
                    "reject indices for vote target {} are not strictly increasing: {:?}",
                    votes.block_ref, votes.rejects,
                )));
            }
            if let Some(reject) = votes.rejects.last()
                && *reject >= transaction_limit
            {
                return Err(ConsensusError::InvalidTransactionVotes(format!(
                    "reject index {} for vote target {} is not below limit {}",
                    reject, votes.block_ref, transaction_limit,
                )));
            }
        }

        Ok(())
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
    fn verify_and_vote(
        &self,
        block: SignedBlock,
        serialized_block: Bytes,
    ) -> ConsensusResult<(VerifiedBlock, Vec<TransactionIndex>)> {
        self.verify_block(&block)?;

        // If the block verification passed then we can produce the verified block, but we should only return it if the transaction verification passed as well.
        let verified_block = VerifiedBlock::new_verified(block, serialized_block);

        let rejected_transactions = if self.context.protocol_config.transaction_voting_enabled() {
            self.vote(&verified_block)?
        } else {
            self.transaction_verifier
                .verify_batch(
                    &verified_block.reference(),
                    &verified_block.transactions_data(),
                )
                .map_err(|e| ConsensusError::InvalidTransaction(e.to_string()))?;
            vec![]
        };
        Ok((verified_block, rejected_transactions))
    }

    fn vote(&self, block: &VerifiedBlock) -> ConsensusResult<Vec<TransactionIndex>> {
        self.transaction_verifier
            .verify_and_vote_batch(&block.reference(), &block.transactions_data())
            .map_err(|e| ConsensusError::InvalidTransaction(e.to_string()))
    }
}

/// Allows all transactions to pass verification, for testing.
pub struct NoopBlockVerifier;

impl BlockVerifier for NoopBlockVerifier {
    #[allow(private_interfaces)]
    fn verify_and_vote(
        &self,
        _block: SignedBlock,
        _serialized_block: Bytes,
    ) -> ConsensusResult<(VerifiedBlock, Vec<TransactionIndex>)> {
        Ok((
            VerifiedBlock::new_verified(_block, _serialized_block),
            vec![],
        ))
    }

    fn vote(&self, _block: &VerifiedBlock) -> ConsensusResult<Vec<TransactionIndex>> {
        Ok(vec![])
    }
}

#[cfg(test)]
mod test {
    use consensus_config::{AuthorityIndex, ConsensusProtocolConfig};
    use consensus_types::block::{BlockDigest, BlockRef, TransactionIndex};

    use super::*;
    use crate::{
        block::{Block, BlockTransactionVotes, BlockV1, GENESIS_ROUND, TestBlock, Transaction},
        context::Context,
        transaction::{TransactionVerifier, ValidationError},
    };

    struct TxnSizeVerifier {}

    impl TransactionVerifier for TxnSizeVerifier {
        // Fails verification if any transaction is < 4 bytes.
        fn verify_batch(
            &self,
            _block_ref: &BlockRef,
            transactions: &[&[u8]],
        ) -> Result<(), ValidationError> {
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
            _block_ref: &BlockRef,
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
            .set_ancestors_raw(vec![
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
                Err(ConsensusError::InvalidAuthorityIndex {
                    loc: _,
                    index: _,
                    max: _
                })
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
                .set_ancestors_raw(vec![
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
                .set_ancestors_raw(vec![
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
                .set_ancestors_raw(vec![
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
                .set_ancestors_raw(vec![
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
                .set_ancestors_raw(vec![
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
                .set_ancestors_raw(vec![
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
            let serialized_block = signed_block
                .serialize()
                .expect("Block serialization failed.");
            assert!(matches!(
                verifier.verify_and_vote(signed_block, serialized_block),
                Err(ConsensusError::InvalidTransaction(_))
            ));
        }
    }

    #[tokio::test]
    async fn test_block_version_matches_protocol_config() {
        let (context, keypairs) = Context::new_for_test(4);
        let epoch = context.committee.epoch();
        const AUTHOR: u32 = 2;
        let author_key = &keypairs[AUTHOR as usize].1;
        let mut v3_context = context.clone();
        v3_context.protocol_config.set_enable_v3_for_testing(true);
        // Genesis blocks differ between block versions, so a round 1 block of the wrong version
        // also has unknown genesis ancestors.
        let genesis_ancestors = |context: &Context| {
            let mut refs = genesis_blocks(context)
                .iter()
                .map(|block| block.reference())
                .collect::<Vec<_>>();
            refs.swap(0, AUTHOR as usize);
            refs
        };
        let v2_genesis = genesis_ancestors(&context);
        let v3_genesis = genesis_ancestors(&v3_context);
        let verifier = SignedBlockVerifier::new(Arc::new(context), Arc::new(TxnSizeVerifier {}));
        let v3_verifier =
            SignedBlockVerifier::new(Arc::new(v3_context), Arc::new(TxnSizeVerifier {}));
        let ancestors = vec![
            BlockRef::new(9, AuthorityIndex::new_for_test(2), BlockDigest::MIN),
            BlockRef::new(9, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
            BlockRef::new(9, AuthorityIndex::new_for_test(1), BlockDigest::MIN),
        ];
        let test_block = TestBlock::new(10, AUTHOR).set_ancestors_raw(ancestors.clone());
        let v1 = SignedBlock::new(
            Block::V1(BlockV1::new(
                epoch,
                10,
                AuthorityIndex::new_for_test(AUTHOR),
                0,
                ancestors,
                vec![],
                vec![],
                vec![],
            )),
            author_key,
        )
        .unwrap();
        let v2 = SignedBlock::new(test_block.clone().build(), author_key).unwrap();
        let v3 = SignedBlock::new(test_block.clone().build_v3(0), author_key).unwrap();
        assert_eq!(v1.transaction_votes_cutoff_round(), GENESIS_ROUND);
        assert_eq!(v2.transaction_votes_cutoff_round(), GENESIS_ROUND);

        // Without v3, only the earlier block versions are valid.
        verifier.verify_block(&v1).unwrap();
        verifier.verify_block(&v2).unwrap();
        assert!(matches!(
            verifier.verify_block(&v3),
            Err(ConsensusError::UnexpectedBlockVersion { version })
                if version.contains("is V3 but v3 is disabled")
        ));

        // With v3, only V3 blocks are valid.
        v3_verifier.verify_block(&v3).unwrap();
        for block in [&v1, &v2] {
            assert!(matches!(
                v3_verifier.verify_block(block),
                Err(ConsensusError::UnexpectedBlockVersion { version })
                    if version.contains("is not V3 but v3 is enabled")
            ));
        }

        // The version check must run before the genesis ancestor check, which would otherwise
        // report the unknown genesis ancestors of a round 1 block of the wrong version.
        let round_1_v3 = SignedBlock::new(
            TestBlock::new(1, AUTHOR)
                .set_ancestors_raw(v3_genesis)
                .build_v3(0),
            author_key,
        )
        .unwrap();
        v3_verifier.verify_block(&round_1_v3).unwrap();
        assert!(matches!(
            verifier.verify_block(&round_1_v3),
            Err(ConsensusError::UnexpectedBlockVersion { version })
                if version.contains("is V3 but v3 is disabled")
        ));
        let round_1_v2 = SignedBlock::new(
            TestBlock::new(1, AUTHOR)
                .set_ancestors_raw(v2_genesis)
                .build(),
            author_key,
        )
        .unwrap();
        verifier.verify_block(&round_1_v2).unwrap();
        assert!(matches!(
            v3_verifier.verify_block(&round_1_v2),
            Err(ConsensusError::UnexpectedBlockVersion { version })
                if version.contains("is not V3 but v3 is enabled")
        ));

        let invalid_cutoff = SignedBlock::new(test_block.build_v3(10), author_key).unwrap();
        assert!(matches!(
            v3_verifier.verify_block(&invalid_cutoff),
            Err(ConsensusError::InvalidTransactionVotesCutoff {
                cutoff: 10,
                block: 10,
            })
        ));
    }

    #[tokio::test]
    async fn test_v3_transaction_votes_are_bounded_and_valid() {
        let (mut context, keypairs) = Context::new_for_test(4);
        context.protocol_config.set_enable_v3_for_testing(true);
        context
            .protocol_config
            .set_max_num_transactions_in_block_for_testing(2);
        let context = Arc::new(context);
        const AUTHOR: u32 = 2;
        let verifier = SignedBlockVerifier::new(context, Arc::new(TxnSizeVerifier {}));
        let direct_target = BlockRef::new(9, AuthorityIndex::new_for_test(0), BlockDigest::MIN);
        let old_ancestor = BlockRef::new(7, AuthorityIndex::new_for_test(3), BlockDigest::MIN);
        let test_block = TestBlock::new(10, AUTHOR).set_ancestors_raw(vec![
            BlockRef::new(9, AuthorityIndex::new_for_test(2), BlockDigest::MIN),
            direct_target,
            BlockRef::new(9, AuthorityIndex::new_for_test(1), BlockDigest::MIN),
        ]);
        let verify = |votes, cutoff_round| {
            let block = test_block
                .clone()
                .set_transaction_votes(votes)
                .build_v3(cutoff_round);
            let signed_block = SignedBlock::new(block, &keypairs[AUTHOR as usize].1).unwrap();
            verifier.verify_block(&signed_block)
        };
        let assert_invalid = |result: ConsensusResult<()>, reason: &str| match result {
            Err(ConsensusError::InvalidTransactionVotes(message)) => {
                assert!(message.contains(reason), "unexpected error: {message}");
            }
            other => panic!("expected invalid transaction votes, got {other:?}"),
        };

        verify(
            vec![BlockTransactionVotes {
                block_ref: direct_target,
                rejects: vec![0, 1],
            }],
            8,
        )
        .unwrap();

        assert_invalid(
            verify(
                vec![
                    BlockTransactionVotes {
                        block_ref: direct_target,
                        rejects: vec![0],
                    },
                    BlockTransactionVotes {
                        block_ref: direct_target,
                        rejects: vec![1],
                    },
                ],
                8,
            ),
            "appears more than once",
        );
        verify(
            vec![BlockTransactionVotes {
                block_ref: old_ancestor,
                rejects: vec![0],
            }],
            6,
        )
        .unwrap();
        assert_invalid(
            verify(
                vec![BlockTransactionVotes {
                    block_ref: BlockRef::new(10, AuthorityIndex::new_for_test(3), BlockDigest::MIN),
                    rejects: vec![0],
                }],
                8,
            ),
            "must have a round less than block round",
        );
        assert_invalid(
            verify(
                vec![BlockTransactionVotes {
                    block_ref: direct_target,
                    rejects: vec![0],
                }],
                9,
            ),
            "at or below cutoff",
        );
        assert_invalid(
            verify(
                vec![BlockTransactionVotes {
                    block_ref: direct_target,
                    rejects: vec![],
                }],
                8,
            ),
            "has no reject indices",
        );
        assert_invalid(
            verify(
                vec![BlockTransactionVotes {
                    block_ref: direct_target,
                    rejects: vec![0, 0],
                }],
                8,
            ),
            "not strictly increasing",
        );
        assert_invalid(
            verify(
                vec![BlockTransactionVotes {
                    block_ref: direct_target,
                    rejects: vec![0, 1, 2],
                }],
                8,
            ),
            "3 reject indices but the limit is 2",
        );
        assert_invalid(
            verify(
                vec![BlockTransactionVotes {
                    block_ref: direct_target,
                    rejects: vec![1, 0],
                }],
                8,
            ),
            "not strictly increasing",
        );
        assert_invalid(
            verify(
                vec![BlockTransactionVotes {
                    block_ref: direct_target,
                    rejects: vec![2],
                }],
                8,
            ),
            "is not below limit 2",
        );
    }

    #[tokio::test]
    async fn test_v3_transaction_count_limit() {
        let (mut context, keypairs) = Context::new_for_test(4);
        context.protocol_config.set_enable_v3_for_testing(true);
        context
            .protocol_config
            .set_max_num_transactions_in_block_for_testing(2);
        let context = Arc::new(context);
        const AUTHOR: u32 = 2;
        let verifier = SignedBlockVerifier::new(context, Arc::new(TxnSizeVerifier {}));
        let test_block = TestBlock::new(10, AUTHOR).set_ancestors_raw(vec![
            BlockRef::new(9, AuthorityIndex::new_for_test(2), BlockDigest::MIN),
            BlockRef::new(9, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
            BlockRef::new(9, AuthorityIndex::new_for_test(1), BlockDigest::MIN),
        ]);
        let verify = |num_transactions: usize| {
            let block = test_block
                .clone()
                .set_transactions(vec![Transaction::new(vec![1]); num_transactions])
                .build_v3(8);
            let signed_block = SignedBlock::new(block, &keypairs[AUTHOR as usize].1).unwrap();
            verifier.verify_block(&signed_block)
        };

        verify(2).unwrap();
        assert!(matches!(
            verify(3),
            Err(ConsensusError::TooManyTransactions { count: 3, limit: 2 })
        ));
    }

    #[tokio::test]
    async fn test_v3_transaction_vote_target_limit() {
        let (mut context, keypairs) = Context::new_for_test(4);
        context.protocol_config.set_enable_v3_for_testing(true);
        context.protocol_config.set_gc_depth_for_testing(2);
        let vote_target_limit = max_transaction_vote_targets(&context);
        assert_eq!(vote_target_limit, 8);
        let context = Arc::new(context);
        const AUTHOR: u32 = 2;
        let verifier = SignedBlockVerifier::new(context, Arc::new(TxnSizeVerifier {}));
        let test_block = TestBlock::new(10, AUTHOR).set_ancestors_raw(vec![
            BlockRef::new(9, AuthorityIndex::new_for_test(2), BlockDigest::MIN),
            BlockRef::new(9, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
            BlockRef::new(9, AuthorityIndex::new_for_test(1), BlockDigest::MIN),
        ]);
        // Rounds 8 and 9 hold one target per authority, which is exactly the limit.
        let mut transaction_votes = (8..=9)
            .flat_map(|round| {
                (0..4).map(move |author| BlockTransactionVotes {
                    block_ref: BlockRef::new(
                        round,
                        AuthorityIndex::new_for_test(author),
                        BlockDigest::MIN,
                    ),
                    rejects: vec![0],
                })
            })
            .collect::<Vec<_>>();
        let verify = |transaction_votes: Vec<BlockTransactionVotes>| {
            let block = test_block
                .clone()
                .set_transaction_votes(transaction_votes)
                .build_v3(7);
            let signed_block = SignedBlock::new(block, &keypairs[AUTHOR as usize].1).unwrap();
            verifier.verify_block(&signed_block)
        };

        assert_eq!(transaction_votes.len(), vote_target_limit);
        verify(transaction_votes.clone()).unwrap();

        // An equivocating block in round 9 adds the target above the limit.
        transaction_votes.push(BlockTransactionVotes {
            block_ref: BlockRef::new(9, AuthorityIndex::new_for_test(0), BlockDigest::MAX),
            rejects: vec![0],
        });
        assert!(matches!(
            verify(transaction_votes),
            Err(ConsensusError::InvalidTransactionVotes(message))
                if message.contains("has 9 vote targets but the limit is 8")
        ));
    }

    // `TransactionConsumer::new()` asserts max_num_transactions_in_block is not more than the
    // first reserved index. Votes must not use reserved indices, even at that highest limit.
    #[tokio::test]
    async fn test_v3_transaction_votes_exclude_reserved_indices() {
        let first_reserved_index =
            TransactionIndex::MAX.saturating_sub(NUM_RESERVED_TRANSACTION_INDICES);
        let (mut context, keypairs) = Context::new_for_test(4);
        context.protocol_config.set_enable_v3_for_testing(true);
        context
            .protocol_config
            .set_max_num_transactions_in_block_for_testing(first_reserved_index.into());
        let context = Arc::new(context);
        const AUTHOR: u32 = 2;
        let verifier = SignedBlockVerifier::new(context, Arc::new(TxnSizeVerifier {}));
        let direct_target = BlockRef::new(9, AuthorityIndex::new_for_test(0), BlockDigest::MIN);
        let test_block = TestBlock::new(10, AUTHOR).set_ancestors_raw(vec![
            BlockRef::new(9, AuthorityIndex::new_for_test(2), BlockDigest::MIN),
            direct_target,
            BlockRef::new(9, AuthorityIndex::new_for_test(1), BlockDigest::MIN),
        ]);
        let verify = |rejects: Vec<TransactionIndex>| {
            let block = test_block
                .clone()
                .set_transaction_votes(vec![BlockTransactionVotes {
                    block_ref: direct_target,
                    rejects,
                }])
                .build_v3(8);
            let signed_block = SignedBlock::new(block, &keypairs[AUTHOR as usize].1).unwrap();
            verifier.verify_block(&signed_block)
        };

        verify(vec![first_reserved_index - 1]).unwrap();
        assert!(matches!(
            verify(vec![first_reserved_index]),
            Err(ConsensusError::InvalidTransactionVotes(_))
        ));
    }

    #[tokio::test]
    async fn test_verify_and_vote_transactions() {
        let mut protocol_config = ConsensusProtocolConfig::for_testing();
        protocol_config.set_transaction_voting_enabled_for_testing(true);

        let (context, keypairs) = Context::new_for_test(4);
        let context = Arc::new(context.with_protocol_config(protocol_config));

        const AUTHOR: u32 = 2;
        let author_protocol_keypair = &keypairs[AUTHOR as usize].1;
        let verifier = SignedBlockVerifier::new(context.clone(), Arc::new(TxnSizeVerifier {}));

        let base_block = TestBlock::new(10, AUTHOR).set_ancestors_raw(vec![
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
            let serialized_block = signed_block
                .serialize()
                .expect("Block serialization failed.");
            let (verified_block, rejected_transactions) = verifier
                .verify_and_vote(signed_block, serialized_block.clone())
                .unwrap();
            assert_eq!(rejected_transactions, Vec::<TransactionIndex>::new());
            assert_eq!(verified_block.serialized().clone(), serialized_block);
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
            let serialized_block = signed_block
                .serialize()
                .expect("Block serialization failed.");
            let (_verified_block, rejected_transactions) = verifier
                .verify_and_vote(signed_block, serialized_block)
                .unwrap();
            assert_eq!(
                rejected_transactions,
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
            let serialized_block = signed_block
                .serialize()
                .expect("Block serialization failed.");
            assert!(matches!(
                verifier.verify_and_vote(signed_block, serialized_block),
                Err(ConsensusError::InvalidTransaction(_))
            ));
        }
    }
}
