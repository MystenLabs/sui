// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{context_data::db_data_provider::PgManager, error::Error};

use super::big_int::BigInt;
use super::move_object::MoveObject;
use async_graphql::*;

use sui_json_rpc::coin_api::parse_to_struct_tag;
use sui_types::balance::Supply;
use sui_types::coin::CoinMetadata as SuiCoinMetadata;
use sui_types::gas_coin::{GAS, TOTAL_SUPPLY_SUI};

pub(crate) struct CoinMetadata {
    pub super_: MoveObject,
    pub native: SuiCoinMetadata,
}

#[Object]
impl CoinMetadata {
    /// Convert the coin object into a Move object
    async fn as_move_object(&self) -> &MoveObject {
        &self.super_
    }

    async fn decimals(&self) -> Option<u8> {
        Some(self.native.decimals)
    }

    async fn name(&self) -> Option<&String> {
        Some(&self.native.name)
    }

    async fn symbol(&self) -> Option<&String> {
        Some(&self.native.symbol)
    }

    async fn description(&self) -> Option<&String> {
        Some(&self.native.description)
    }

    async fn icon_url(&self) -> Option<&String> {
        self.native.icon_url.as_ref()
    }

    async fn supply(&self, ctx: &Context<'_>) -> Result<Option<BigInt>> {
        let coin_type = &self.super_.native.type_().type_params()[0];
        let coin_struct = parse_to_struct_tag(&coin_type.to_canonical_string(true))?;

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

pub(crate) enum CoinMetadataDowncastError {
    NotACoinMetadata,
    Bcs(bcs::Error),
}

impl TryFrom<&MoveObject> for CoinMetadata {
    type Error = CoinMetadataDowncastError;

    fn try_from(move_object: &MoveObject) -> Result<Self, Self::Error> {
        if !move_object.native.type_().is_coin_metadata() {
            return Err(CoinMetadataDowncastError::NotACoinMetadata);
        }

        Ok(Self {
            super_: move_object.clone(),
            native: bcs::from_bytes(move_object.native.contents())
                .map_err(CoinMetadataDowncastError::Bcs)?,
        })
    }
}
