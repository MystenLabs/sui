// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::{GasCoin, SuiGasCoin};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sui_types::base_types::{ObjectID, SuiAddress};

#[derive(Debug, JsonSchema, Serialize, Deserialize)]
pub struct ReserveGasStorageRequest {
    pub gas_budget: u64,
    pub request_sponsor: SuiAddress,
}

#[derive(Debug, JsonSchema, Serialize, Deserialize)]
pub struct UpdateGasStorageRequest {
    pub sponsor_address: SuiAddress,
    pub released_gas_coins: Vec<SuiGasCoin>,
    pub deleted_gas_coins: Vec<ObjectID>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReserveGasStorageResponse {
    pub gas_coins: Option<Vec<SuiGasCoin>>,
    pub error: Option<String>,
}

impl ReserveGasStorageResponse {
    pub fn new_ok(gas_coins: Vec<GasCoin>) -> Self {
        Self {
            gas_coins: Some(gas_coins.into_iter().map(|c| c.into()).collect()),
            error: None,
        }
    }

    pub fn new_err(error: anyhow::Error) -> Self {
        Self {
            gas_coins: None,
            error: Some(error.to_string()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateGasStorageResponse {
    pub error: Option<String>,
}

impl UpdateGasStorageResponse {
    pub fn new_ok() -> Self {
        Self { error: None }
    }

    pub fn new_err(error: anyhow::Error) -> Self {
        Self {
            error: Some(error.to_string()),
        }
    }
}
