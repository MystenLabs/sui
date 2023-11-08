// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{context_data::db_data_provider::PgManager, error::Error};

use super::big_int::BigInt;
use super::move_object::MoveObject;
use async_graphql::*;

use sui_json_rpc::coin_api::parse_to_struct_tag;
use sui_types::balance::Supply;
use sui_types::coin::Coin as NativeCoin;
use sui_types::gas_coin::{GAS, TOTAL_SUPPLY_SUI};

#[derive(Clone)]
pub(crate) struct Coin {
    /// Representation of this Coin as a generic Move Object.
    pub super_: MoveObject,

    /// The deserialized representation of the Move Object's contents, as a `0x2::coin::Coin`.
    pub native: NativeCoin,
}

pub(crate) enum CoinDowncastError {
    NotACoin,
    Bcs(bcs::Error),
}

#[Object]
impl Coin {
    /// Balance of the coin object
    async fn balance(&self) -> Option<BigInt> {
        Some(BigInt::from(self.native.balance.value()))
    }

    /// Convert the coin object into a Move object
    async fn as_move_object(&self) -> &MoveObject {
        &self.super_
    }
}

impl TryFrom<&MoveObject> for Coin {
    type Error = CoinDowncastError;

    fn try_from(move_object: &MoveObject) -> Result<Self, Self::Error> {
        if !move_object.native.is_coin() {
            return Err(CoinDowncastError::NotACoin);
        }

        Ok(Self {
            super_: move_object.clone(),
            native: bcs::from_bytes(move_object.native.contents())
                .map_err(CoinDowncastError::Bcs)?,
        })
    }
}

#[derive(SimpleObject)]
#[graphql(complex)]
pub(crate) struct CoinMetadata {
    pub decimals: Option<u8>,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    #[graphql(skip)]
    pub coin_type: String,
}

#[ComplexObject]
impl CoinMetadata {
    async fn supply(&self, ctx: &Context<'_>) -> Result<Option<BigInt>> {
        let coin_struct = parse_to_struct_tag(&self.coin_type)?;
        let total_supply = if GAS::is_gas(&coin_struct) {
            Supply {
                value: TOTAL_SUPPLY_SUI,
            }
        } else {
            ctx.data_unchecked::<PgManager>()
                .inner
                .get_total_supply_in_blocking_task(coin_struct)
                .await
                .map_err(|e| Error::Internal(e.to_string()))
                .extend()?
        };

        Ok(Some(BigInt::from(total_supply.value)))
    }
}
