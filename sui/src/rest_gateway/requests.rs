// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

/**
Request containing the information needed to execute a split coin transaction.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SplitCoinRequest {
    pub signer: String,
    pub coin_object_id: String,
    pub split_amounts: Vec<u64>,
    pub gas_payment: String,
    pub gas_budget: u64,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct SignedTransaction {
    pub unsigned_tx_base64: String,
    pub signature: String,
    pub pub_key: String,
}
