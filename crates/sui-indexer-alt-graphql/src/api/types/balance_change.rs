// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use anyhow::Context as _;
use async_graphql::SimpleObject;
use sui_indexer_alt_schema::transactions::BalanceChange as StoredBalanceChange;
use sui_rpc::proto::sui::rpc::v2::BalanceChange as GrpcBalanceChange;
use sui_types::{TypeTag, object::Owner};

use crate::{api::scalars::big_int::BigInt, error::RpcError, scope::Scope};

use super::{address::Address, move_type::MoveType};

/// Effects to the balance (sum of coin values per coin type) of addresses and objects.
#[derive(Clone, SimpleObject)]
pub(crate) struct BalanceChange {
    /// The address or object whose balance has changed.
    pub(crate) owner: Option<Address>,
    /// The inner type of the coin whose balance has changed (e.g. `0x2::sui::SUI`).
    pub(crate) coin_type: Option<MoveType>,
    /// The signed balance change.
    pub(crate) amount: Option<BigInt>,
}

impl BalanceChange {
    /// Create a BalanceChange from a gRPC BalanceChange.
    pub(crate) fn from_grpc(scope: Scope, grpc: &GrpcBalanceChange) -> Result<Self, RpcError> {
        let address = grpc.address().parse().context("Failed to parse address")?;
        let coin_type: TypeTag = grpc
            .coin_type()
            .parse()
            .context("Failed to parse coin type")?;
        let amount: i128 = grpc.amount().parse().context("Failed to parse amount")?;

        Ok(Self {
            owner: Some(Address::with_address(scope.clone(), address)),
            coin_type: Some(MoveType::from_native(coin_type, scope)),
            amount: Some(BigInt::from(amount)),
        })
    }

    /// Create a BalanceChange from a stored BalanceChange (database).
    pub(crate) fn from_stored(scope: Scope, stored: StoredBalanceChange) -> Result<Self, RpcError> {
        let StoredBalanceChange::V1 {
            owner,
            coin_type,
            amount,
        } = stored;

        // Extract address from owner
        let address = match owner {
            Owner::AddressOwner(addr)
            | Owner::ObjectOwner(addr)
            | Owner::ConsensusAddressOwner { owner: addr, .. } => Some(addr),
            Owner::Shared { .. } | Owner::Immutable => None,
        };

        let coin_type = TypeTag::from_str(&coin_type).context("Failed to parse coin type")?;

        Ok(Self {
            owner: address.map(|addr| Address::with_address(scope.clone(), addr)),
            coin_type: Some(MoveType::from_native(coin_type, scope)),
            amount: Some(BigInt::from(amount)),
        })
    }
}
