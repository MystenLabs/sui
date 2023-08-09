// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BatchFaucetResponse {
    // This string is the Uuid for the req
    pub task: Option<String>,
    pub error: Option<String>,
}

impl From<FaucetError> for BatchFaucetResponse {
    fn from(e: FaucetError) -> Self {
        Self {
            error: Some(e.to_string()),
            task: None,
        }
    }
}

impl From<BatchFaucetReceipt> for BatchFaucetResponse {
    fn from(v: BatchFaucetReceipt) -> Self {
        Self {
            task: Some(v.task),
            error: None,
        }
    }
}

impl From<Uuid> for BatchFaucetResponse {
    fn from(v: Uuid) -> Self {
        Self {
            task: Some(v.to_string()),
            error: None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BatchStatusFaucetResponse {
    // This string is the Uuid for the req
    pub status: Option<BatchSendStatus>,
    pub error: Option<String>,
}

impl From<FaucetError> for BatchStatusFaucetResponse {
    fn from(e: FaucetError) -> Self {
        Self {
            error: Some(e.to_string()),
            status: None,
        }
    }
}

impl From<BatchSendStatus> for BatchStatusFaucetResponse {
    fn from(v: BatchSendStatus) -> Self {
        Self {
            status: Some(v),
            error: None,
        }
    }
}
