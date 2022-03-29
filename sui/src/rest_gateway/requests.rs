// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_with::base64::Base64;
use serde_with::serde_as;

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
#[serde_as]
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallRequest {
    /** Required; Hex code as string representing the sender's address */
    pub signer: String,
    /** Required; Hex code as string representing Move module location */
    pub package_object_id: String,
    /** Required; Name of the move module */
    pub module: String,
    /** Required; Name of the function to be called in the move module */
    pub function: String,
    /** Optional; The argument types to be parsed */
    pub type_arguments: Option<Vec<String>>,
    /** Required; Byte representation of the arguments */
    #[serde_as(as = "Vec<Base64>")]
    pub pure_arguments: Vec<Vec<u8>>,
    /** Required; Hex code as string representing the gas object id */
    pub gas_object_id: String,
    /** Required; Gas budget required as a cap for gas usage */
    pub gas_budget: u64,
    /** Required; Object arguments */
    pub object_arguments: Vec<String>,
    /** Required; Share object arguments */
    pub shared_object_arguments: Vec<String>,
}

/**
Request containing the address of which objecst are to be retrieved.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetObjectsRequest {
    /** Required; Hex code as string representing the address */
    pub address: String,
}

/**
    Request containing the object schema for which info is to be retrieved.

If owner is specified we look for this object in that address's account store,
otherwise we look for it in the shared object store.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetObjectSchemaRequest {
    /** Required; Hex code as string representing the object id */
    pub object_id: String,
}

/**
Request containing the object for which info is to be retrieved.

If owner is specified we look for this object in that address's account store,
otherwise we look for it in the shared object store.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetObjectInfoRequest {
    /** Required; Hex code as string representing the object id */
    pub object_id: String,
}

/**
Request representing the contents of the Move module to be published.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PublishRequest {
    /** Required; Hex code as string representing the sender's address */
    pub sender: String,
    /** Required; Move modules serialized as hex */
    pub compiled_modules: Vec<String>,
    /** Required; Hex code as string representing the gas object id */
    pub gas_object_id: String,
    /** Required; Gas budget required because of the need to execute module initializers */
    pub gas_budget: u64,
}
