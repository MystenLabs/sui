// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::big_int::BigInt;
use super::move_object::MoveObject;
use async_graphql::*;

use sui_types::coin::Coin as NativeCoin;

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
