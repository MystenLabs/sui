// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FaucetResponse {
    pub transferred_gas_objects: Vec<CoinInfo>,
    pub error: Option<String>,
}

impl From<FaucetError> for FaucetResponse {
    fn from(e: FaucetError) -> Self {
        Self {
            error: Some(e.to_string()),
            transferred_gas_objects: vec![],
        }
    }
}

impl From<FaucetReceipt> for FaucetResponse {
    fn from(v: FaucetReceipt) -> Self {
        Self {
            transferred_gas_objects: v.sent,
            error: None,
        }
    }
}
