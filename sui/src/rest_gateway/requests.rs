// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

/// Request containing the information needed to create a split coin transaction.
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SplitCoinRequest {
    /// Required; Hex code as string representing the sender's address
    pub signer: String,
    /// Required; Hex code as string representing the coin object id
    pub coin_object_id: String,
    /// Required; Amount of each new coins
    pub split_amounts: Vec<u64>,
    /// Required; Hex code as string representing the gas object id
    pub gas_payment: String,
    /// Required; Gas budget required as a cap for gas usage
    pub gas_budget: u64,
}

/// Request containing the information needed to create a split coin transaction.
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MergeCoinRequest {
    /// Required; Hex code as string representing the sender's address
    pub signer: String,
    /// Required; Hex code as string representing the coin object id
    pub primary_coin: String,
    /// Required; Hex code as string representing the coin object id
    pub coin_to_merge: String,
    /// Required; Hex code as string representing the gas object id
    pub gas_payment: String,
    /// Required; Gas budget required as a cap for gas usage
    pub gas_budget: u64,
}

/// Request containing the information needed to execute a transaction.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct SignedTransaction {
    /// Required; Base64 encoded string representing the BCS serialised TransactionData object
    pub tx_bytes: String,
    /// Required; Base64 encoded string representing the ed25519 signature bytes
    pub signature: String,
    /// Required; Base64 encoded string representing the ed25519 public key
    pub pub_key: String,
}

/// Request containing the information required to create a move module call transaction.
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CallRequest {
    /// Required; Hex code as string representing the sender's address
    pub signer: String,
    /// Required; Hex code as string representing Move module location
    pub package_object_id: String,
    /// Required; Name of the move module
    pub module: String,
    /// Required; Name of the function to be called in the move module
    pub function: String,
    /// Optional; The argument types to be parsed
    pub type_arguments: Option<Vec<String>>,
    /// Required; Byte representation of the arguments, Base64 encoded
    pub pure_arguments: Vec<String>,
    /// Required; Hex code as string representing the gas object id
    pub gas_object_id: String,
    /// Required; Gas budget required as a cap for gas usage
    pub gas_budget: u64,
    /// Required; Object arguments
    pub object_arguments: Vec<String>,
    /// Required; Share object arguments
    pub shared_object_arguments: Vec<String>,
}

/// Request containing the address of which object are to be retrieved.
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetObjectsRequest {
    /// Required; Hex code as string representing the address
    pub address: String,
}

/// Request containing the object schema for which info is to be retrieved.
///
/// If owner is specified we look for this object in that address's account store,
/// otherwise we look for it in the shared object store.

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetObjectSchemaRequest {
    /// Required; Hex code as string representing the object id
    pub object_id: String,
}

/// Request containing the object for which info is to be retrieved.
///
/// If owner is specified we look for this object in that address's account store,
/// otherwise we look for it in the shared object store.
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetObjectInfoRequest {
    /// Required; Hex code as string representing the object id
    pub object_id: String,
}

/// Request representing the contents of the Move module to be published.
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PublishRequest {
    /// Required; Hex code as string representing the sender's address
    pub sender: String,
    /// Required; Move modules serialized as Base64
    pub compiled_modules: Vec<String>,
    /// Required; Hex code as string representing the gas object id
    pub gas_object_id: String,
    /// Required; Gas budget required because of the need to execute module initializers
    pub gas_budget: u64,
}

/// Request containing the information needed to create a transfer transaction.
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TransferTransactionRequest {
    /// Required; Hex code as string representing the address to be sent from
    pub from_address: String,
    /// Required; Hex code as string representing the object id
    pub object_id: String,
    /// Required; Hex code as string representing the address to be sent to
    pub to_address: String,
    /// Required; Hex code as string representing the gas object id to be used as payment
    pub gas_object_id: String,
    /** Required; Gas budget required as a cap for gas usage */
    pub gas_budget: u64,
}

/// Request containing the address that requires a sync.
/// TODO: This call may not be required. Sync should not need to be triggered by user.
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SyncRequest {
    /// Required; Hex code as string representing the address
    pub address: String,
}

/// Query string containing the digest of the transaction to be retrieved.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct GetTransactionDetailsRequest {
    /// Required; hex string encoding a 32 byte transaction digest
    pub digest: String,
}
