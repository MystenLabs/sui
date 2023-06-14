// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use sui_types::base_types::SuiAddress;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum FaucetRequest {
    FixedAmountRequest(FixedAmountRequest),
    GetBatchSendStatusRequest(GetBatchSendStatusRequest),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FixedAmountRequest {
    pub recipient: SuiAddress,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GetBatchSendStatusRequest {
    pub task_id: String,
}

impl FaucetRequest {
    pub fn new_fixed_amount_request(recipient: impl Into<SuiAddress>) -> Self {
        Self::FixedAmountRequest(FixedAmountRequest {
            recipient: recipient.into(),
        })
    }

    pub fn new_get_batch_send_status_request(task_id: impl Into<String>) -> Self {
        Self::GetBatchSendStatusRequest(GetBatchSendStatusRequest {
            task_id: task_id.into(),
        })
    }
}
