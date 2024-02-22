// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::{
    block::{BlockAPI, SignedBlock, VerifiedBlock},
    context::Context,
    error::{ConsensusError, ConsensusResult},
    transaction::TransactionVerifier,
};

pub(crate) trait BlockVerifier: Send + Sync + 'static {
    /// Verifies a block's metadata and transactions.
    /// This is called before examining a block's causal history.
    fn verify(&self, block: &SignedBlock) -> ConsensusResult<()>;

    /// Verifies a block w.r.t. ancestor blocks.
    /// This is called after a block has complete causal history locally,
    /// and is ready to be accepted into the DAG.
    ///
    /// Caller must make sure ancestors corresponse to block.ancestors() 1-to-1, in the same order.
    fn check_ancestors(
        &self,
        block: &VerifiedBlock,
        ancestors: &[VerifiedBlock],
    ) -> ConsensusResult<()>;
}

/// `SignedBlockVerifier` checks the validity of a block.
///
/// Blocks that fail verification at one honest authority will be rejected by all other honest
/// authorities as well. The means invalid blocks, and blocks with an invalid ancestor, will never
/// be accepted into the DAG.
pub(crate) struct SignedBlockVerifier {
    context: Arc<Context>,
    transaction_verifier: Arc<dyn TransactionVerifier>,
}

impl SignedBlockVerifier {
    pub(crate) fn new(
        context: Arc<Context>,
        transaction_verifier: Arc<dyn TransactionVerifier>,
    ) -> Self {
        Self {
            context,
            transaction_verifier,
        }
    }
}

// All block verification logic are implemented below.
impl BlockVerifier for SignedBlockVerifier {
    fn verify(&self, block: &SignedBlock) -> ConsensusResult<()> {
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
        // Verifiy the block's signature.
        block.verify_signature(&self.context)?;
        // Verify the block's ancestor refs are consistent with the block's round,
        // and total parent stakes reach quorum.
        if block.ancestors().len() > committee.size() {
            return Err(ConsensusError::TooManyAncestors(
                block.ancestors().len(),
                committee.size(),
            ));
        }
        let mut own_ancestor = false;
        let mut seen_parents = vec![false; committee.size()];
        let mut parent_stakes = 0;
        for ancestor in block.ancestors() {
            if !committee.is_valid_index(ancestor.author) {
                return Err(ConsensusError::InvalidAuthorityIndex {
                    index: ancestor.author,
                    max: committee.size() - 1,
                });
            }
            if ancestor.author == block.author() {
                own_ancestor = true;
            }
            if ancestor.round >= block.round() {
                return Err(ConsensusError::InvalidAncestorRound {
                    ancestor: ancestor.round,
                    block: block.round(),
                });
            }
            // Block must have round >= 1 so checked_sub(1) should be safe.
            if ancestor.round == block.round().checked_sub(1).unwrap()
                && !seen_parents[ancestor.author]
            {
                seen_parents[ancestor.author] = true;
                parent_stakes += committee.stake(ancestor.author);
            }
            // TODO: reject blocks with multiple ancestors from the same authority.
        }
        if !own_ancestor {
            return Err(ConsensusError::MissingOwnAncestor);
        }
        if !committee.reached_quorum(parent_stakes) {
            return Err(ConsensusError::InsufficientParentStakes {
                parent_stakes,
                quorum: committee.quorum_threshold(),
            });
        }

        // TODO: reject when there are too many or large transactions.
        let batch: Vec<_> = block.transactions().iter().map(|t| t.data()).collect();
        self.transaction_verifier
            .verify_batch(&self.context.protocol_config, &batch)
            .map_err(|e| ConsensusError::InvalidTransaction(format!("{e:?}")))
    }

    fn check_ancestors(
        &self,
        block: &VerifiedBlock,
        ancestors: &[VerifiedBlock],
    ) -> ConsensusResult<()> {
        assert_eq!(block.ancestors().len(), ancestors.len());
        for (ancestor, block) in block.ancestors().iter().zip(ancestors.iter()) {
            assert_eq!(ancestor, &block.reference());
        }
        // TODO: verify block timestamp w.r.t. its ancestors.
        Ok(())
    }
}

pub(crate) struct NoopBlockVerifier;

impl BlockVerifier for NoopBlockVerifier {
    fn verify(&self, _block: &SignedBlock) -> ConsensusResult<()> {
        Ok(())
    }

    fn check_ancestors(
        &self,
        _block: &VerifiedBlock,
        _ancestors: &[VerifiedBlock],
    ) -> ConsensusResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use consensus_config::AuthorityIndex;

    use super::*;
    use crate::{
        block::{BlockDigest, BlockRef, TestBlock, Transaction},
        context::Context,
        transaction::{TransactionVerifier, ValidationError},
    };

    struct TxnSizeVerifier {}

    impl TransactionVerifier for TxnSizeVerifier {
        // Fails verification if any transaction is < 4 bytes.
        fn verify_batch(
            &self,
            _protocol_config: &sui_protocol_config::ProtocolConfig,
            transactions: &[&[u8]],
        ) -> Result<(), ValidationError> {
            for txn in transactions {
                if txn.len() < 4 {
                    return Err(ValidationError::InvalidTransaction(format!(
                        "Lenght {} too short!",
                        txn.len()
                    )));
                }
            }
            Ok(())
        }
    }

    #[test]
    fn test_verify_block() {
        let (context, keypairs) = Context::new_for_test(4);
        let context = Arc::new(context);
        let authority_2_protocol_keypair = &keypairs[2].1;
        let verifier = SignedBlockVerifier::new(context.clone(), Arc::new(TxnSizeVerifier {}));

        let test_block = TestBlock::new(10, 2)
            .set_ancestors(vec![
                BlockRef::new(9, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
                BlockRef::new(9, AuthorityIndex::new_for_test(1), BlockDigest::MIN),
                BlockRef::new(9, AuthorityIndex::new_for_test(2), BlockDigest::MIN),
                BlockRef::new(7, AuthorityIndex::new_for_test(3), BlockDigest::MIN),
            ])
            .set_transactions(vec![Transaction::new(vec![4; 8])]);

        // Valid SignedBlock.
        {
            let block = test_block.clone().build();
            let signed_block = SignedBlock::new(block, authority_2_protocol_keypair).unwrap();
            verifier.verify(&signed_block).unwrap();
        }

        // SignedBlock with wrong epoch.
        {
            let block = test_block.clone().set_epoch(1).build();
            let signed_block = SignedBlock::new(block, authority_2_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify(&signed_block),
                Err(ConsensusError::WrongEpoch {
                    expected: _,
                    actual: _
                })
            ));
        }

        // SignedBlock at genesis round.
        {
            let block = test_block.clone().set_round(0).build();
            let signed_block = SignedBlock::new(block, authority_2_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify(&signed_block),
                Err(ConsensusError::UnexpectedGenesisBlock)
            ));
        }

        // SignedBlock with invalid authority index.
        {
            let block = test_block
                .clone()
                .set_author(AuthorityIndex::new_for_test(4))
                .build();
            let signed_block = SignedBlock::new(block, authority_2_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify(&signed_block),
                Err(ConsensusError::InvalidAuthorityIndex { index: _, max: _ })
            ));
        }

        // SignedBlock with mismatched authority index and signature.
        {
            let block = test_block
                .clone()
                .set_author(AuthorityIndex::new_for_test(1))
                .build();
            let signed_block = SignedBlock::new(block, authority_2_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify(&signed_block),
                Err(ConsensusError::SignatureVerificationFailure(_))
            ));
        }

        // SignedBlock with wrong key.
        {
            let block = test_block.clone().build();
            let signed_block = SignedBlock::new(block, &keypairs[3].1).unwrap();
            assert!(matches!(
                verifier.verify(&signed_block),
                Err(ConsensusError::SignatureVerificationFailure(_))
            ));
        }

        // SignedBlock without signature.
        {
            let block = test_block.clone().build();
            let mut signed_block = SignedBlock::new(block, authority_2_protocol_keypair).unwrap();
            signed_block.clear_signature();
            assert!(matches!(
                verifier.verify(&signed_block),
                Err(ConsensusError::MalformedSignature(_))
            ));
        }

        // SignedBlock with invalid ancestor round.
        {
            let block = test_block
                .clone()
                .set_ancestors(vec![
                    BlockRef::new(9, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
                    BlockRef::new(9, AuthorityIndex::new_for_test(1), BlockDigest::MIN),
                    BlockRef::new(9, AuthorityIndex::new_for_test(2), BlockDigest::MIN),
                    BlockRef::new(10, AuthorityIndex::new_for_test(3), BlockDigest::MIN),
                ])
                .build();
            let signed_block = SignedBlock::new(block, authority_2_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify(&signed_block),
                Err(ConsensusError::InvalidAncestorRound {
                    ancestor: _,
                    block: _
                })
            ));
        }

        // SignedBlock with parents not reaching quorum.
        {
            let block = test_block
                .clone()
                .set_ancestors(vec![
                    BlockRef::new(9, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
                    BlockRef::new(8, AuthorityIndex::new_for_test(1), BlockDigest::MIN),
                    BlockRef::new(9, AuthorityIndex::new_for_test(2), BlockDigest::MIN),
                    BlockRef::new(8, AuthorityIndex::new_for_test(3), BlockDigest::MIN),
                ])
                .build();
            let signed_block = SignedBlock::new(block, authority_2_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify(&signed_block),
                Err(ConsensusError::InsufficientParentStakes {
                    parent_stakes: _,
                    quorum: _
                })
            ));
        }

        // SignedBlock with too many ancestors.
        {
            let block = test_block
                .clone()
                .set_ancestors(vec![
                    BlockRef::new(9, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
                    BlockRef::new(8, AuthorityIndex::new_for_test(1), BlockDigest::MIN),
                    BlockRef::new(9, AuthorityIndex::new_for_test(2), BlockDigest::MIN),
                    BlockRef::new(8, AuthorityIndex::new_for_test(3), BlockDigest::MIN),
                    BlockRef::new(9, AuthorityIndex::new_for_test(3), BlockDigest::MIN),
                ])
                .build();
            let signed_block = SignedBlock::new(block, authority_2_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify(&signed_block),
                Err(ConsensusError::TooManyAncestors(_, _))
            ));
        }

        // SignedBlock without own ancestor.
        {
            let block = test_block
                .clone()
                .set_ancestors(vec![
                    BlockRef::new(9, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
                    BlockRef::new(8, AuthorityIndex::new_for_test(1), BlockDigest::MIN),
                    BlockRef::new(8, AuthorityIndex::new_for_test(3), BlockDigest::MIN),
                ])
                .build();
            let signed_block = SignedBlock::new(block, authority_2_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify(&signed_block),
                Err(ConsensusError::MissingOwnAncestor)
            ));
        }

        // SignedBlock with invalid transaction.
        {
            let block = test_block
                .clone()
                .set_transactions(vec![Transaction::new(vec![1; 2])])
                .build();
            let signed_block = SignedBlock::new(block, authority_2_protocol_keypair).unwrap();
            assert!(matches!(
                verifier.verify(&signed_block),
                Err(ConsensusError::InvalidTransaction(_))
            ));
        }
    }
}
