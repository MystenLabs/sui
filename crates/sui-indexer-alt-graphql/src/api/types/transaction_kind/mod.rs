// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Union;
use sui_types::transaction::TransactionKind as NativeTransactionKind;

use crate::scope::Scope;

pub(crate) mod authenticator_state_update;
pub(crate) mod change_epoch;
pub(crate) mod consensus_commit_prologue;
pub(crate) mod end_of_epoch;
pub(crate) mod genesis;
pub(crate) mod programmable;
pub(crate) mod programmable_system;
pub(crate) mod randomness_state_update;

/// Different types of transactions that can be executed on the Sui network.
#[derive(Union, Clone)]
pub enum TransactionKind {
    Genesis(genesis::GenesisTransaction),
    ConsensusCommitPrologue(consensus_commit_prologue::ConsensusCommitPrologueTransaction),
    ChangeEpoch(change_epoch::ChangeEpochTransaction),
    RandomnessStateUpdate(randomness_state_update::RandomnessStateUpdateTransaction),
    AuthenticatorStateUpdate(authenticator_state_update::AuthenticatorStateUpdateTransaction),
    EndOfEpoch(end_of_epoch::EndOfEpochTransaction),
    Programmable(programmable::ProgrammableTransaction),
    // GraphQL Union does not allow multiple variants with the same type
    ProgrammableSystem(programmable_system::ProgrammableSystemTransaction),
}

impl TransactionKind {
    pub fn from(kind: NativeTransactionKind, scope: Scope) -> Option<Self> {
        use NativeTransactionKind as K;
        use TransactionKind as T;

        match kind {
            K::Genesis(g) => Some(T::Genesis(genesis::GenesisTransaction { native: g, scope })),
            K::ConsensusCommitPrologue(ccp) => Some(T::ConsensusCommitPrologue(
                consensus_commit_prologue::ConsensusCommitPrologueTransaction::from_v1(ccp, scope),
            )),
            K::ConsensusCommitPrologueV2(ccp) => Some(T::ConsensusCommitPrologue(
                consensus_commit_prologue::ConsensusCommitPrologueTransaction::from_v2(ccp, scope),
            )),
            K::ConsensusCommitPrologueV3(ccp) => Some(T::ConsensusCommitPrologue(
                consensus_commit_prologue::ConsensusCommitPrologueTransaction::from_v3(ccp, scope),
            )),
            K::ConsensusCommitPrologueV4(ccp) => Some(T::ConsensusCommitPrologue(
                consensus_commit_prologue::ConsensusCommitPrologueTransaction::from_v4(ccp, scope),
            )),
            K::ChangeEpoch(ce) => Some(T::ChangeEpoch(change_epoch::ChangeEpochTransaction {
                native: ce,
                scope,
            })),
            K::RandomnessStateUpdate(rsu) => Some(T::RandomnessStateUpdate(
                randomness_state_update::RandomnessStateUpdateTransaction { native: rsu },
            )),
            K::AuthenticatorStateUpdate(asu) => Some(T::AuthenticatorStateUpdate(
                authenticator_state_update::AuthenticatorStateUpdateTransaction {
                    native: asu,
                    scope,
                },
            )),
            K::EndOfEpochTransaction(eoe) => {
                Some(T::EndOfEpoch(end_of_epoch::EndOfEpochTransaction {
                    native: eoe,
                    scope,
                }))
            }
            K::ProgrammableTransaction(pt) => {
                Some(T::Programmable(programmable::ProgrammableTransaction {
                    native: pt,
                    scope,
                }))
            }
            K::ProgrammableSystemTransaction(pt) => Some(T::ProgrammableSystem(
                programmable_system::ProgrammableSystemTransaction {
                    inner: programmable::ProgrammableTransaction { native: pt, scope },
                },
            )),
        }
    }
}
