// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Union;
use sui_types::transaction::TransactionKind as NativeTransactionKind;

use crate::scope::Scope;

use self::{
    consensus_commit_prologue::ConsensusCommitPrologueTransaction, genesis::GenesisTransaction,
};

pub(crate) mod consensus_commit_prologue;
pub(crate) mod genesis;

/// Different types of transactions that can be executed on the Sui network.
#[derive(Union, Clone)]
pub enum TransactionKind {
    Genesis(GenesisTransaction),
    ConsensusCommitPrologue(ConsensusCommitPrologueTransaction),
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
            // Other types will return None for now
            _ => None,
        }
    }
}
