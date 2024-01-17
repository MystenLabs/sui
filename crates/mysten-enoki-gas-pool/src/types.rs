// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sui_json_rpc_types::SuiObjectRef;
use sui_types::base_types::ObjectRef;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GasCoin {
    pub object_ref: ObjectRef,
    pub balance: u64,
}

#[derive(Debug, JsonSchema, Serialize, Deserialize)]
pub struct SuiGasCoin {
    pub object_ref: SuiObjectRef,
    pub balance: u64,
}

impl From<GasCoin> for SuiGasCoin {
    fn from(gas_coin: GasCoin) -> Self {
        Self {
            object_ref: gas_coin.object_ref.into(),
            balance: gas_coin.balance,
        }
    }
}

impl From<SuiGasCoin> for GasCoin {
    fn from(gas_coin: SuiGasCoin) -> Self {
        Self {
            object_ref: gas_coin.object_ref.to_object_ref(),
            balance: gas_coin.balance,
        }
    }
}
