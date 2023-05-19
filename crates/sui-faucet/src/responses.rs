// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FaucetResponse {
    // This string is the Uuid for the req
    pub task: Option<String>,
    pub error: Option<String>,
}

impl From<FaucetError> for FaucetResponse {
    fn from(e: FaucetError) -> Self {
        Self {
            error: Some(e.to_string()),
            task: None,
        }
    }
}

impl From<FaucetReceipt> for FaucetResponse {
    fn from(v: FaucetReceipt) -> Self {
        Self {
            task: Some(v.task),
            error: None,
        }
    }
}
