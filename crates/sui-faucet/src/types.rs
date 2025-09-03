// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::FaucetError;
use serde::{Deserialize, Serialize};
use sui_sdk::types::{
    base_types::{ObjectID, SuiAddress},
    digests::TransactionDigest,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum FaucetRequest {
    FixedAmountRequest(FixedAmountRequest),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FixedAmountRequest {
    pub recipient: SuiAddress,
}

impl FaucetRequest {
    pub fn new_fixed_amount_request(recipient: impl Into<SuiAddress>) -> Self {
        Self::FixedAmountRequest(FixedAmountRequest {
            recipient: recipient.into(),
        })
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum RequestStatus {
    Success,
    Failure(FaucetError),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FaucetResponse {
    pub status: RequestStatus,
    pub coins_sent: Option<Vec<CoinInfo>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CoinInfo {
    pub amount: u64,
    pub id: ObjectID,
    pub transfer_tx_digest: TransactionDigest,
}

impl From<FaucetError> for FaucetResponse {
    fn from(value: FaucetError) -> Self {
        FaucetResponse {
            status: RequestStatus::Failure(value),
            coins_sent: None,
        }
    }
}

impl From<reqwest::Error> for FaucetResponse {
    fn from(value: reqwest::Error) -> Self {
        FaucetResponse {
            status: RequestStatus::Failure(FaucetError::internal(value)),
            coins_sent: None,
        }
    }
}
