// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::*;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub(crate) const DEFAULT_AMOUNT: u64 = 2000;
pub(crate) const DEFAULT_NUM_COINS: usize = 5;

/* -------------------------------------------------------------------------- */
/*                                   Request                                  */
/* -------------------------------------------------------------------------- */

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum FaucetRequest {
    FixedAmountRequest(FixedAmountRequest),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FixedAmountRequest {
    recipient: String,
}

impl FaucetRequest {
    pub fn new_fixed_amount_request(recipient: impl Into<String>) -> Self {
        Self::FixedAmountRequest(FixedAmountRequest {
            recipient: recipient.into(),
        })
    }
}

/* -------------------------------------------------------------------------- */
/*                                  Response                                  */
/* -------------------------------------------------------------------------- */

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FaucetResponse {
    pub gas_object_ids: Vec<String>,
    pub error: Option<String>,
}

#[async_trait]
impl FaucetService for FixedAmountRequest {
    async fn execute(self, faucet: &(impl Faucet + Send + Sync)) -> FaucetResponse {
        match faucet
            .send(&self.recipient, &[DEFAULT_AMOUNT; DEFAULT_NUM_COINS])
            .await
        {
            Ok(v) => v.into(),
            Err(e) => e.into(),
        }
    }
}

impl From<FaucetError> for FaucetResponse {
    fn from(e: FaucetError) -> Self {
        Self {
            error: Some(e.to_string()),
            gas_object_ids: vec![],
        }
    }
}

impl From<Vec<String>> for FaucetResponse {
    fn from(v: Vec<String>) -> Self {
        Self {
            gas_object_ids: v,
            error: None,
        }
    }
}
