// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Object;
use sui_indexer_alt_schema::transactions::BalanceChange as NativeBalanceChange;
use sui_types::object::Owner as NativeOwner;

use crate::{api::scalars::big_int::BigInt, scope::Scope};

use super::address::Address;

#[derive(Clone)]
pub(crate) struct BalanceChange {
    pub(crate) scope: Scope,
    pub(crate) native: NativeBalanceChange,
}

/// Effects to the balance (sum of coin values per coin type) of addresses and objects.
#[Object]
impl BalanceChange {
    /// The address or object whose balance has changed.
    async fn owner(&self) -> Option<Address> {
        use NativeOwner as O;
        let NativeBalanceChange::V1 { owner, .. } = &self.native;

        match owner {
            O::AddressOwner(addr)
            | O::ObjectOwner(addr)
            | O::ConsensusAddressOwner { owner: addr, .. } => {
                Some(Address::with_address(self.scope.clone(), *addr))
            }
            O::Shared { .. } | O::Immutable => None,
        }
    }

    /// The inner type of the coin whose balance has changed (e.g. `0x2::sui::SUI`).
    async fn coin_type(&self) -> Option<String> {
        let NativeBalanceChange::V1 { coin_type, .. } = &self.native;
        Some(coin_type.clone())
    }

    /// The signed balance change.
    async fn amount(&self) -> Option<BigInt> {
        let NativeBalanceChange::V1 { amount, .. } = &self.native;
        Some(BigInt::from(*amount))
    }
}

impl BalanceChange {
    /// Create a BalanceChange from the native schema type.
    pub(crate) fn from_native(scope: Scope, native: NativeBalanceChange) -> Self {
        Self { scope, native }
    }
}
