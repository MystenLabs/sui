// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

use crate::sui_json::SuiJsonValue;

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

/**
Request containing the information needed to execute a split coin transaction.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MergeCoinRequest {
    pub signer: String,
    pub primary_coin: String,
    pub coin_to_merge: String,
    pub gas_payment: String,
    pub gas_budget: u64,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct SignedTransaction {
    pub unsigned_tx_base64: String,
    pub signature: String,
    pub pub_key: String,
}

/**
Request containing the information required to execute a move module.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CallRequest {
    /** Required; Hex code as string representing the sender's address */
    pub sender: String,
    /** Required; Hex code as string representing Move module location */
    pub package_object_id: String,
    /** Required; Name of the move module */
    pub module: String,
    /** Required; Name of the function to be called in the move module */
    pub function: String,
    /** Optional; The argument types to be parsed */
    pub type_args: Option<Vec<String>>,
    /** Required; JSON representation of the arguments */
    pub args: Vec<SuiJsonValue>,
    /** Required; Hex code as string representing the gas object id */
    pub gas_object_id: String,
    /** Required; Gas budget required as a cap for gas usage */
    pub gas_budget: u64,
}
