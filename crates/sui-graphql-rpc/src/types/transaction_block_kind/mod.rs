// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use self::{
    consensus_commit_prologue::ConsensusCommitPrologueTransaction,
    end_of_epoch::ChangeEpochTransaction, genesis::GenesisTransaction,
    randomness_state_update::RandomnessStateUpdateTransaction,
};
use crate::types::transaction_block_kind::{
    authenticator_state_update::AuthenticatorStateUpdateTransaction,
    end_of_epoch::EndOfEpochTransaction, programmable::ProgrammableTransactionBlock,
};
use async_graphql::*;
use sui_types::transaction::TransactionKind as NativeTransactionKind;

pub(crate) mod authenticator_state_update;
pub(crate) mod consensus_commit_prologue;
pub(crate) mod end_of_epoch;
pub(crate) mod genesis;
pub(crate) mod programmable;
pub(crate) mod randomness_state_update;

/// The kind of transaction block, either a programmable transaction or a system transaction.
#[derive(Union, PartialEq, Clone, Eq)]
pub(crate) enum TransactionBlockKind {
    ConsensusCommitPrologue(ConsensusCommitPrologueTransaction),
    Genesis(GenesisTransaction),
    ChangeEpoch(ChangeEpochTransaction),
    Programmable(ProgrammableTransactionBlock),
    AuthenticatorState(AuthenticatorStateUpdateTransaction),
    Randomness(RandomnessStateUpdateTransaction),
    EndOfEpoch(EndOfEpochTransaction),
}

impl TransactionBlockKind {
    pub(crate) fn from(kind: NativeTransactionKind, checkpoint_viewed_at: u64) -> Self {
        use NativeTransactionKind as K;
        use TransactionBlockKind as T;

        match kind {
            K::ProgrammableTransaction(pt) => T::Programmable(ProgrammableTransactionBlock {
                native: pt,
                checkpoint_viewed_at,
            }),
            K::ChangeEpoch(ce) => T::ChangeEpoch(ChangeEpochTransaction {
                native: ce,
                checkpoint_viewed_at,
            }),
            K::Genesis(g) => T::Genesis(GenesisTransaction {
                native: g,
                checkpoint_viewed_at,
            }),
            K::ConsensusCommitPrologue(ccp) => T::ConsensusCommitPrologue(
                ConsensusCommitPrologueTransaction::from_v1(ccp, checkpoint_viewed_at),
            ),
            K::ConsensusCommitPrologueV2(ccp) => T::ConsensusCommitPrologue(
                ConsensusCommitPrologueTransaction::from_v2(ccp, checkpoint_viewed_at),
            ),
            K::ConsensusCommitPrologueV3(ccp) => T::ConsensusCommitPrologue(
                ConsensusCommitPrologueTransaction::from_v3(ccp, checkpoint_viewed_at),
            ),
            K::AuthenticatorStateUpdate(asu) => {
                T::AuthenticatorState(AuthenticatorStateUpdateTransaction {
                    native: asu,
                    checkpoint_viewed_at,
                })
            }
            K::EndOfEpochTransaction(eoe) => T::EndOfEpoch(EndOfEpochTransaction {
                native: eoe,
                checkpoint_viewed_at,
            }),
            K::RandomnessStateUpdate(rsu) => T::Randomness(RandomnessStateUpdateTransaction {
                native: rsu,
                checkpoint_viewed_at,
            }),
        }
    }
}
