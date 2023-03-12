// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use move_core_types::language_storage::TypeTag;
use serde_with::DisplayFromStr;
use sui_types::object::Owner;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BalanceChange {
    pub owner: Owner,
    pub change_type: BalanceChangeType,
    #[schemars(with = "String")]
    #[serde_as(as = "DisplayFromStr")]
    pub coin_type: TypeTag,
    /// The amount indicate the balance value changes,
    /// negative amount means spending coin value and positive means receiving coin value.
    pub amount: i128,
}

#[derive(Eq, Debug, Copy, Clone, PartialEq, Deserialize, Serialize, Hash, JsonSchema)]
pub enum BalanceChangeType {
    Gas,
    Pay,
    Receive,
}
