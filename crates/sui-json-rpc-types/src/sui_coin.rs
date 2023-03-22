// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use serde_with::{serde_as, DisplayFromStr};
use sui_types::base_types::{
    EpochId, ObjectDigest, ObjectID, ObjectRef, SequenceNumber, TransactionDigest,
};
use sui_types::coin::CoinMetadata;

use sui_types::error::SuiError;
use sui_types::object::Object;

use crate::{BigInt, Page};

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, PartialEq, Eq, Copy)]
/// Type for de/serializing u128 to string
pub struct BigBigInt(
    #[serde_as(as = "DisplayFromStr")]
    #[schemars(with = "String")]
    u128,
);

impl From<BigBigInt> for u128 {
    fn from(x: BigBigInt) -> u128 {
        x.0
    }
}

impl From<u128> for BigBigInt {
    fn from(v: u128) -> BigBigInt {
        BigBigInt(v)
    }
}

impl Display for BigBigInt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub type CoinPage = Page<Coin, ObjectID>;

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Balance {
    pub coin_type: String,
    pub coin_object_count: usize,
    pub total_balance: BigBigInt,
    pub locked_balance: HashMap<EpochId, u128>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Coin {
    pub coin_type: String,
    pub coin_object_id: ObjectID,
    pub version: SequenceNumber,
    pub digest: ObjectDigest,
    pub balance: BigInt,
    pub locked_until_epoch: Option<EpochId>,
    pub previous_transaction: TransactionDigest,
}

impl Coin {
    pub fn object_ref(&self) -> ObjectRef {
        (self.coin_object_id, self.version, self.digest)
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SuiCoinMetadata {
    /// Number of decimal places the coin uses.
    pub decimals: u8,
    /// Name for the token
    pub name: String,
    /// Symbol for the token
    pub symbol: String,
    /// Description of the token
    pub description: String,
    /// URL for the token logo
    pub icon_url: Option<String>,
    /// Object id for the CoinMetadata object
    pub id: Option<ObjectID>,
}

impl TryFrom<Object> for SuiCoinMetadata {
    type Error = SuiError;
    fn try_from(object: Object) -> Result<Self, Self::Error> {
        let metadata: CoinMetadata = object.try_into()?;
        let CoinMetadata {
            decimals,
            name,
            symbol,
            description,
            icon_url,
            id,
        } = metadata;
        Ok(Self {
            id: Some(*id.object_id()),
            decimals,
            name,
            symbol,
            description,
            icon_url,
        })
    }
}
