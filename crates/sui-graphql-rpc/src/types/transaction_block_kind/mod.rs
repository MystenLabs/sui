// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::transaction_block_kind::authenticator_state_update::AuthenticatorStateUpdateTransaction;

use self::{
    change_epoch::ChangeEpochTransaction,
    consensus_commit_prologue::ConsensusCommitPrologueTransaction, genesis::GenesisTransaction,
    randomness_state_update::RandomnessStateUpdateTransaction,
};
use async_graphql::*;
use sui_types::transaction::TransactionKind as NativeTransactionKind;

pub(crate) mod authenticator_state_update;
pub(crate) mod change_epoch;
pub(crate) mod consensus_commit_prologue;
pub(crate) mod genesis;
pub(crate) mod randomness_state_update;

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

// TODO: flesh out the programmable transaction block type
#[derive(SimpleObject, Clone, Eq, PartialEq)]
pub(crate) struct ProgrammableTransactionBlock {
    pub value: String,
}

// TODO: flesh out the end of epoch transaction type
#[derive(SimpleObject, Clone, Eq, PartialEq)]
pub(crate) struct EndOfEpochTransaction {
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct TxBlockKindNotImplementedYet {
    pub(crate) text: String,
}

impl From<NativeTransactionKind> for TransactionBlockKind {
    fn from(kind: NativeTransactionKind) -> Self {
        use NativeTransactionKind as K;
        use TransactionBlockKind as T;

        match kind {
            // TODO: flesh out type
            K::ProgrammableTransaction(pt) => T::Programmable(ProgrammableTransactionBlock {
                value: format!("{pt:?}"),
            }),

            K::ChangeEpoch(ce) => T::ChangeEpoch(ChangeEpochTransaction(ce)),

            K::Genesis(g) => T::Genesis(GenesisTransaction(g)),

            K::ConsensusCommitPrologue(ccp) => T::ConsensusCommitPrologue(ccp.into()),

            K::ConsensusCommitPrologueV2(ccp) => T::ConsensusCommitPrologue(ccp.into()),

            K::AuthenticatorStateUpdate(asu) => {
                T::AuthenticatorState(AuthenticatorStateUpdateTransaction(asu))
            }

            // TODO: flesh out type
            K::EndOfEpochTransaction(eoe) => T::EndOfEpoch(EndOfEpochTransaction {
                value: format!("{eoe:?}"),
            }),

            K::RandomnessStateUpdate(rsu) => T::Randomness(RandomnessStateUpdateTransaction(rsu)),
        }
    }
}
