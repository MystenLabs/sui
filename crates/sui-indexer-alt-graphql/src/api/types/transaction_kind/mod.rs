// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Union;
use sui_types::transaction::TransactionKind as NativeTransactionKind;

use crate::scope::Scope;

use self::genesis::GenesisTransaction;

pub(crate) mod genesis;

/// Different types of transactions that can be executed on the Sui network.
#[derive(Union, Clone)]
pub enum TransactionKind {
    Genesis(GenesisTransaction),
}

impl TransactionKind {
    pub fn from(kind: NativeTransactionKind, scope: Scope) -> Option<Self> {
        use NativeTransactionKind as K;
        use TransactionKind as T;

        match kind {
            K::Genesis(g) => Some(T::Genesis(GenesisTransaction { native: g, scope })),
            // For now, only support Genesis - other types will return None
            _ => None,
        }
    }
}
