// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context_data::db_data_provider::PgManager;

use super::big_int::BigInt;
use super::move_object::MoveObject;
use async_graphql::*;

use sui_types::coin::CoinMetadata as NativeCoinMetadata;

pub(crate) struct CoinMetadata {
    pub super_: MoveObject,
    pub native: NativeCoinMetadata,
}

pub(crate) enum CoinMetadataDowncastError {
    NotCoinMetadata,
    Bcs(bcs::Error),
}

#[Object]
impl CoinMetadata {
    /// The number of decimal places used to represent the token.
    async fn decimals(&self) -> Option<u8> {
        Some(self.native.decimals)
    }

    /// Full, official name of the token.
    async fn name(&self) -> Option<&str> {
        Some(&self.native.name)
    }

    /// The token's identifying abbreviation.
    async fn symbol(&self) -> Option<&str> {
        Some(&self.native.symbol)
    }

    /// Optional description of the token, provided by the creator of the token.
    async fn description(&self) -> Option<&str> {
        Some(&self.native.description)
    }

    async fn icon_url(&self) -> Option<&str> {
        self.native.icon_url.as_deref()
    }

    /// The overall quantity of tokens that will be issued.
    async fn supply(&self, ctx: &Context<'_>) -> Result<Option<BigInt>> {
        let type_params = self.super_.native.type_().type_params();
        let Some(coin_type) = type_params.first() else {
            return Ok(None);
        };

        let supply = ctx
            .data_unchecked::<PgManager>()
            .fetch_total_supply(coin_type.to_canonical_string(/* with_prefix */ true))
            .await
            .extend()?;

        Ok(supply.map(BigInt::from))
    }

    /// Convert the coin metadata object into a Move object.
    async fn as_move_object(&self) -> &MoveObject {
        &self.super_
    }
}

impl TryFrom<&MoveObject> for CoinMetadata {
    type Error = CoinMetadataDowncastError;

    fn try_from(move_object: &MoveObject) -> Result<Self, Self::Error> {
        if !move_object.native.type_().is_coin_metadata() {
            return Err(CoinMetadataDowncastError::NotCoinMetadata);
        }

        Ok(Self {
            super_: move_object.clone(),
            native: bcs::from_bytes(move_object.native.contents())
                .map_err(CoinMetadataDowncastError::Bcs)?,
        })
    }
}
