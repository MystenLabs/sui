// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use anyhow::Context as _;
use async_graphql::Object;
use sui_indexer_alt_reader::kv_loader::BalanceChangeContents as KvBalanceChangeContents;
use sui_indexer_alt_schema::transactions::BalanceChange as StoredBalanceChange;
use sui_rpc::proto::sui::rpc::v2::BalanceChange as GrpcBalanceChange;
use sui_types::TypeTag;
use sui_types::balance_change::BalanceChange as NativeBalanceChange;
use sui_types::object::Owner;

use crate::api::scalars::big_int::BigInt;
use crate::api::types::address::Address;
use crate::api::types::move_type::MoveType;
use crate::error::RpcError;
use crate::scope::Scope;

/// Content variants for balance changes from different sources
#[derive(Clone)]
pub(crate) enum BalanceChangeContents {
    Grpc(GrpcBalanceChange),
    Native(NativeBalanceChange),
    Stored(StoredBalanceChange),
}

/// Effects to the balance (sum of coin values per coin type) of addresses and objects.
#[derive(Clone)]
pub(crate) struct BalanceChange {
    pub(crate) scope: Scope,
    pub(crate) content: BalanceChangeContents,
}

impl BalanceChange {
    /// Create a BalanceChange from a gRPC BalanceChange.
    pub(crate) fn from_grpc(scope: Scope, grpc: GrpcBalanceChange) -> Self {
        Self {
            scope,
            content: BalanceChangeContents::Grpc(grpc),
        }
    }

    /// Create a BalanceChange from a stored BalanceChange (database).
    pub(crate) fn from_stored(scope: Scope, stored: StoredBalanceChange) -> Self {
        Self {
            scope,
            content: BalanceChangeContents::Stored(stored),
        }
    }

    /// Create a BalanceChange from a native BalanceChange.
    pub(crate) fn from_native(scope: Scope, native: NativeBalanceChange) -> Self {
        Self {
            scope,
            content: BalanceChangeContents::Native(native),
        }
    }

    /// Create a BalanceChange from KvBalanceChangeContents (from kv_loader).
    pub(crate) fn from_kv_content(scope: Scope, kv_content: KvBalanceChangeContents) -> Self {
        match kv_content {
            KvBalanceChangeContents::Grpc(grpc) => Self::from_grpc(scope, grpc),
            KvBalanceChangeContents::Native(native) => Self::from_native(scope, native),
        }
    }
}

#[Object]
impl BalanceChange {
    /// The address or object whose balance has changed.
    async fn owner(&self) -> Result<Option<Address>, RpcError> {
        let address = match &self.content {
            BalanceChangeContents::Grpc(grpc) => {
                grpc.address().parse().context("Failed to parse address")?
            }
            BalanceChangeContents::Native(native) => native.address,
            BalanceChangeContents::Stored(stored) => {
                let StoredBalanceChange::V1 { owner, .. } = stored;
                match owner {
                    Owner::AddressOwner(addr)
                    | Owner::ObjectOwner(addr)
                    | Owner::ConsensusAddressOwner { owner: addr, .. } => *addr,
                    Owner::Shared { .. } | Owner::Immutable => return Ok(None),
                }
            }
        };

        Ok(Some(Address::with_address(self.scope.clone(), address)))
    }

    /// The inner type of the coin whose balance has changed (e.g. `0x2::sui::SUI`).
    async fn coin_type(&self) -> Result<Option<MoveType>, RpcError> {
        let coin_type = match &self.content {
            BalanceChangeContents::Grpc(grpc) => grpc
                .coin_type()
                .parse()
                .context("Failed to parse coin type")?,
            BalanceChangeContents::Native(native) => native.coin_type.clone(),
            BalanceChangeContents::Stored(stored) => {
                let StoredBalanceChange::V1 { coin_type, .. } = stored;
                TypeTag::from_str(coin_type).context("Failed to parse coin type")?
            }
        };

        Ok(Some(MoveType::from_native(coin_type, self.scope.clone())))
    }

    /// The signed balance change.
    async fn amount(&self) -> Result<Option<BigInt>, RpcError> {
        let amount = match &self.content {
            BalanceChangeContents::Grpc(grpc) => grpc
                .amount()
                .parse::<i128>()
                .context("Failed to parse amount")?,
            BalanceChangeContents::Native(native) => native.amount,
            BalanceChangeContents::Stored(stored) => {
                let StoredBalanceChange::V1 { amount, .. } = stored;
                *amount
            }
        };

        Ok(Some(BigInt::from(amount)))
    }
}

impl From<BalanceChangeContents> for GrpcBalanceChange {
    fn from(content: BalanceChangeContents) -> Self {
        match content {
            BalanceChangeContents::Grpc(grpc) => grpc,
            BalanceChangeContents::Native(native) => {
                let mut grpc = GrpcBalanceChange::default();
                grpc.set_address(native.address.to_string());
                grpc.set_coin_type(native.coin_type.to_canonical_string(/* with_prefix */ true));
                grpc.set_amount(native.amount.to_string());
                grpc
            }
            BalanceChangeContents::Stored(stored) => {
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

                let mut grpc = GrpcBalanceChange::default();
                if let Some(addr) = address {
                    grpc.set_address(addr.to_string());
                }
                grpc.set_coin_type(coin_type);
                grpc.set_amount(amount.to_string());
                grpc
            }
        }
    }
}
