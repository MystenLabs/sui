// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use anyhow::Context as _;
use async_graphql::Object;
use sui_indexer_alt_schema::transactions::BalanceChange as StoredBalanceChange;
use sui_types::{TypeTag, object::Owner as NativeOwner};

use crate::{api::scalars::big_int::BigInt, error::RpcError, scope::Scope};

use super::{address::Address, move_type::MoveType};

#[derive(Clone)]
pub(crate) struct BalanceChange {
    pub(crate) scope: Scope,
    pub(crate) stored: StoredBalanceChange,
}

/// Effects to the balance (sum of coin values per coin type) of addresses and objects.
#[Object]
impl BalanceChange {
    /// The address or object whose balance has changed.
    async fn owner(&self) -> Option<Address> {
        use NativeOwner as O;
        let StoredBalanceChange::V1 { owner, .. } = &self.stored;

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
    async fn coin_type(&self) -> Result<Option<MoveType>, RpcError> {
        let StoredBalanceChange::V1 { coin_type, .. } = &self.stored;
        let type_ = TypeTag::from_str(coin_type).context("Failed to parse coin type")?;
        Ok(Some(MoveType::from_native(type_, self.scope.clone())))
    }

    /// The signed balance change.
    async fn amount(&self) -> Option<BigInt> {
        let StoredBalanceChange::V1 { amount, .. } = &self.stored;
        Some(BigInt::from(*amount))
    }
}
