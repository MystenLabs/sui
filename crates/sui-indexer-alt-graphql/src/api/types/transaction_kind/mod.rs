// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Union;
use sui_types::transaction::TransactionKind as NativeTransactionKind;

use crate::scope::Scope;

use self::{
    authenticator_state_update::AuthenticatorStateUpdateTransaction,
    change_epoch::ChangeEpochTransaction,
    consensus_commit_prologue::ConsensusCommitPrologueTransaction,
    end_of_epoch::EndOfEpochTransaction, genesis::GenesisTransaction,
    randomness_state_update::RandomnessStateUpdateTransaction,
};

pub(crate) mod authenticator_state_update;
pub(crate) mod change_epoch;
pub(crate) mod consensus_commit_prologue;
pub(crate) mod end_of_epoch;
pub(crate) mod genesis;
pub(crate) mod randomness_state_update;

/// Different types of transactions that can be executed on the Sui network.
#[derive(Union, Clone)]
pub enum TransactionKind {
    Genesis(GenesisTransaction),
    ConsensusCommitPrologue(ConsensusCommitPrologueTransaction),
    ChangeEpoch(ChangeEpochTransaction),
    RandomnessStateUpdate(RandomnessStateUpdateTransaction),
    AuthenticatorStateUpdate(AuthenticatorStateUpdateTransaction),
    EndOfEpoch(EndOfEpochTransaction),
}

impl TransactionKind {
    pub fn from(kind: NativeTransactionKind, scope: Scope) -> Option<Self> {
        use NativeTransactionKind as K;
        use TransactionKind as T;

        match kind {
            K::Genesis(g) => Some(T::Genesis(GenesisTransaction { native: g, scope })),
            K::ConsensusCommitPrologue(ccp) => Some(T::ConsensusCommitPrologue(
                ConsensusCommitPrologueTransaction::from_v1(ccp, scope),
            )),
            K::ConsensusCommitPrologueV2(ccp) => Some(T::ConsensusCommitPrologue(
                ConsensusCommitPrologueTransaction::from_v2(ccp, scope),
            )),
            K::ConsensusCommitPrologueV3(ccp) => Some(T::ConsensusCommitPrologue(
                ConsensusCommitPrologueTransaction::from_v3(ccp, scope),
            )),
            K::ConsensusCommitPrologueV4(ccp) => Some(T::ConsensusCommitPrologue(
                ConsensusCommitPrologueTransaction::from_v4(ccp, scope),
            )),
            K::ChangeEpoch(ce) => {
                Some(T::ChangeEpoch(ChangeEpochTransaction { native: ce, scope }))
            }
            K::RandomnessStateUpdate(rsu) => {
                Some(T::RandomnessStateUpdate(RandomnessStateUpdateTransaction {
                    native: rsu,
                }))
            }
            K::AuthenticatorStateUpdate(asu) => Some(T::AuthenticatorStateUpdate(
                AuthenticatorStateUpdateTransaction { native: asu, scope },
            )),
            K::EndOfEpochTransaction(eoe) => {
                Some(T::EndOfEpoch(EndOfEpochTransaction { native: eoe, scope }))
            }
            // Other types will return None for now
            _ => None,
        }
    }
}
