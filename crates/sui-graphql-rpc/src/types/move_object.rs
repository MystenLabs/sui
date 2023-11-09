// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::big_int::BigInt;
use super::coin::CoinDowncastError;
use super::coin::CoinMetadata;
use super::move_value::MoveValue;
use super::stake::StakedSuiDowncastError;
use super::{coin::Coin, object::Object};
use crate::error::Error;
use crate::types::stake::StakedSui;
use async_graphql::*;
use move_core_types::language_storage::{StructTag, TypeTag};
use sui_types::coin::CoinMetadata as SuiCoinMetadata;
use sui_types::governance::StakedSui;
use sui_types::object::{Data, MoveObject as NativeMoveObject, Object as NativeSuiObject};

#[derive(Clone)]
pub(crate) struct MoveObject {
    /// Representation of this Move Object as a generic Object.
    pub super_: Object,

    /// Move-object-specific data, extracted from the native representation at
    /// `graphql_object.native_object.data`.
    pub native: NativeMoveObject,
}

pub(crate) struct MoveObjectDowncastError;

#[Object]
impl MoveObject {
    /// Displays the contents of the MoveObject in a JSON string and through graphql types.  Also
    /// provides the flat representation of the type signature, and the bcs of the corresponding
    /// data
    async fn contents(&self) -> Option<MoveValue> {
        let type_ = StructTag::from(self.native.type_().clone());
        Some(MoveValue::new(
            type_.to_canonical_string(/* with_prefix */ true),
            self.native.contents().into(),
        ))
    }

    /// Determines whether a tx can transfer this object
    async fn has_public_transfer(&self) -> Option<bool> {
        Some(self.native.has_public_transfer())
    }

    /// Attempts to convert the Move object into an Object
    /// This provides additional information such as version and digest on the top-level
    async fn as_object(&self) -> &Object {
        &self.super_
    }

    /// Attempts to convert the Move object into a `0x2::coin::Coin`.
    async fn as_coin(&self) -> Result<Option<Coin>, Error> {
        match Coin::try_from(self) {
            Ok(coin) => Ok(Some(coin)),
            Err(CoinDowncastError::NotACoin) => Ok(None),
            Err(CoinDowncastError::Bcs(e)) => {
                Err(Error::Internal(format!("Failed to deserialize coin: {e}")))
            }
        }
    }

    /// Attempts to convert the Move object into a `0x3::staking_pool::StakedSui`.
    async fn as_staked_sui(&self) -> Result<Option<StakedSui>, Error> {
        match StakedSui::try_from(self) {
            Ok(coin) => Ok(Some(coin)),
            Err(StakedSuiDowncastError::NotAStakedSui) => Ok(None),
            Err(StakedSuiDowncastError::Bcs(e)) => Err(Error::Internal(format!(
                "Failed to deserialize staked sui: {e}"
            ))),
        }
    }
}

impl TryFrom<&Object> for MoveObject {
    type Error = MoveObjectDowncastError;

    fn try_from(object: &Object) -> Result<Self, Self::Error> {
        if let Data::Move(move_object) = &object.native.data {
            Ok(Self {
                super_: object.clone(),
                native: move_object.clone(),
            })
        } else {
            Err(MoveObjectDowncastError)
        }
    }

    async fn as_coin_metadata(&self) -> Result<Option<CoinMetadata>> {
        let coin_metadata = SuiCoinMetadata::try_from(&self.native_object)
            .map_err(|e| Error::Internal(e.to_string()))?;

        let coin_struct = self.native_object.data.struct_tag();

        let Some(coin_type) = coin_struct else {
            return Ok(None);
        };

        Ok(Some(CoinMetadata {
            decimals: Some(coin_metadata.decimals),
            name: Some(coin_metadata.name.clone()),
            symbol: Some(coin_metadata.symbol.clone()),
            description: Some(coin_metadata.description.clone()),
            icon_url: coin_metadata.icon_url.clone(),
            coin_type: coin_type.to_canonical_string(true),
        }))
    }
}
