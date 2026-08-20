// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use async_graphql::Object;
use sui_rpc::proto::sui::rpc::v2::BalanceChange as GrpcBalanceChange;

use crate::api::scalars::big_int::BigInt;
use crate::api::types::address::Address;
use crate::api::types::move_type::MoveType;
use crate::error::RpcError;
use crate::scope::Scope;

#[derive(Clone)]
pub(crate) struct BalanceChange {
    pub(crate) scope: Scope,
    pub(crate) content: GrpcBalanceChange,
}

/// Effects to the balance (sum of coin values per coin type) of addresses and objects.
#[Object]
impl BalanceChange {
    /// The address or object whose balance has changed.
    async fn owner(&self) -> Result<Option<Address>, RpcError> {
        let address = self
            .content
            .address()
            .parse()
            .context("Failed to parse address")?;

        Ok(Some(Address::with_address(self.scope.clone(), address)))
    }

    /// The inner type of the coin whose balance has changed (e.g. `0x2::sui::SUI`).
    async fn coin_type(&self) -> Result<Option<MoveType>, RpcError> {
        let coin_type = self
            .content
            .coin_type()
            .parse()
            .context("Failed to parse coin type")?;

        Ok(Some(MoveType::from_native(coin_type, self.scope.clone())))
    }

    /// The signed balance change.
    async fn amount(&self) -> Result<Option<BigInt>, RpcError> {
        let amount = self
            .content
            .amount()
            .parse::<i128>()
            .context("Failed to parse amount")?;

        Ok(Some(BigInt::from(amount)))
    }
}
