// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde::Serialize;
use serde_json::{json, Value};
use serde_with::serde_as;
use serde_with::DisplayFromStr;

use sui_types::base_types::{SuiAddress, TransactionDigest};
use sui_types::crypto::SignatureScheme;
use sui_types::sui_serde::Hex;

use crate::errors::Error;
use crate::{ErrorType, SUI};

#[derive(Serialize, Deserialize)]
pub struct NetworkIdentifier {
    pub blockchain: String,
    pub network: SuiEnv,
}

#[derive(Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq, Debug, Clone)]
#[serde(rename_all = "lowercase")]
#[allow(clippy::enum_variant_names)]
pub enum SuiEnv {
    MainNet,
    DevNet,
    TestNet,
}

impl SuiEnv {
    pub fn url(&self) -> &str {
        match self {
            SuiEnv::MainNet => "http://127.0.0.1:9000",
            SuiEnv::DevNet => "https://fullnode.devnet.sui.io:443",
            SuiEnv::TestNet => "https://fullnode.testnet.sui.io:443",
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct AccountIdentifier {
    pub address: String,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct Currency {
    pub symbol: String,
    pub decimals: u64,
}
#[derive(Deserialize)]
pub struct AccountBalanceRequest {
    pub network_identifier: NetworkIdentifier,
    pub account_identifier: AccountIdentifier,
}
#[derive(Serialize)]
pub struct AccountBalanceResponse {
    pub block_identifier: BlockIdentifier,
    pub balances: Vec<Amount>,
}

impl IntoResponse for AccountBalanceResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

#[derive(Serialize)]
pub struct BlockIdentifier {
    pub index: u64,
    pub hash: String,
}
#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct Amount {
    #[serde_as(as = "DisplayFromStr")]
    pub value: i64,
    pub currency: Currency,
}

impl Amount {
    pub fn new(value: i64) -> Self {
        Self {
            value,
            currency: SUI.clone(),
        }
    }
}

#[derive(Deserialize)]
pub struct AccountCoinsRequest {
    pub network_identifier: NetworkIdentifier,
    pub account_identifier: AccountIdentifier,
    pub include_mempool: bool,
}
#[derive(Serialize)]
pub struct AccountCoinsResponse {
    pub block_identifier: BlockIdentifier,
    pub coins: Vec<Coin>,
}
impl IntoResponse for AccountCoinsResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}
#[derive(Serialize)]
pub struct Coin {
    pub coin_identifier: CoinIdentifier,
    pub amount: Amount,
}

#[derive(Serialize, Deserialize)]
pub struct CoinIdentifier {
    pub identifier: String,
}

#[derive(Deserialize)]
pub struct MetadataRequest {
    #[serde(default)]
    pub metadata: Option<Value>,
}

#[derive(Serialize)]
pub struct NetworkListResponse {
    pub network_identifiers: Vec<NetworkIdentifier>,
}

impl IntoResponse for NetworkListResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

#[derive(Deserialize)]
pub struct ConstructionDeriveRequest {
    pub network_identifier: NetworkIdentifier,
    pub public_key: PublicKey,
}

#[derive(Deserialize)]
pub struct PublicKey {
    pub hex_bytes: Hex,
    pub curve_type: CurveType,
}

impl TryInto<SuiAddress> for PublicKey {
    type Error = Error;

    fn try_into(self) -> Result<SuiAddress, Self::Error> {
        let key_bytes = self.hex_bytes.to_vec()?;
        let curve = match self.curve_type {
            CurveType::Secp256k1 => SignatureScheme::Secp256k1,
            CurveType::Edwards25519 => SignatureScheme::ED25519,
        };
        let pub_key =
            sui_types::crypto::PublicKey::try_from_bytes(curve, &key_bytes).map_err(|e| {
                Error::new_with_detail(ErrorType::ParsingError, json!({"cause":e.to_string()}))
            })?;
        Ok((&pub_key).into())
    }
}

#[derive(Deserialize, Copy, Clone)]
#[serde(rename_all = "lowercase")]
pub enum CurveType {
    Secp256k1,
    Edwards25519,
}

#[derive(Serialize)]
pub struct ConstructionDeriveResponse {
    pub account_identifier: AccountIdentifier,
}

impl IntoResponse for ConstructionDeriveResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

#[derive(Deserialize)]
pub struct ConstructionPayloadsRequest {
    pub network_identifier: NetworkIdentifier,
    pub operations: Vec<Operation>,
    #[serde(default)]
    pub metadata: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub public_keys: Vec<PublicKey>,
}

#[derive(Deserialize)]
pub struct Operation {
    pub operation_identifier: OperationIdentifier,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_operations: Vec<OperationIdentifier>,
    #[serde(rename = "type")]
    pub type_: OperationType,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub account: Option<AccountIdentifier>,
    #[serde(default)]
    pub amount: Option<Amount>,
    #[serde(default)]
    pub coin_change: Option<CoinChange>,
    #[serde(default)]
    pub metadata: Option<Value>,
}

#[derive(Deserialize, Serialize, Copy, Clone)]
pub enum OperationType {
    Gas,
    TransferSUI,
    TransferCoin,
    MergeCoins,
    SplitCoin,
}

#[derive(Deserialize)]
pub struct OperationIdentifier {
    pub index: u64,
    #[serde(default)]
    pub network_index: Option<u64>,
}

#[derive(Deserialize)]
pub struct CoinChange {
    pub coin_identifier: CoinIdentifier,
    pub coin_action: CoinAction,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoinAction {
    CoinCreated,
    CoinSpent,
}

#[derive(Serialize)]
pub struct ConstructionPayloadsResponse {
    pub unsigned_transaction: Hex,
    pub payloads: Vec<SigningPayload>,
}

impl IntoResponse for ConstructionPayloadsResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

#[derive(Serialize, Deserialize)]
pub struct SigningPayload {
    pub account_identifier: AccountIdentifier,
    pub hex_bytes: Hex,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_type: Option<SignatureType>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SignatureType {
    Ed25519,
    Ecdsa,
}

#[derive(Deserialize)]
pub struct ConstructionCombineRequest {
    pub network_identifier: NetworkIdentifier,
    pub unsigned_transaction: Hex,
    pub signatures: Vec<Signature>,
}

#[derive(Deserialize)]
pub struct Signature {
    pub signing_payload: SigningPayload,
    pub public_key: PublicKey,
    pub signature_type: SignatureType,
    pub hex_bytes: Hex,
}

#[derive(Serialize)]
pub struct ConstructionCombineResponse {
    pub signed_transaction: Hex,
}

impl IntoResponse for ConstructionCombineResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

#[derive(Deserialize)]
pub struct ConstructionSubmitRequest {
    pub network_identifier: NetworkIdentifier,
    pub signed_transaction: Hex,
}
#[derive(Serialize)]
pub struct TransactionIdentifierResponse {
    pub transaction_identifier: TransactionIdentifier,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

impl IntoResponse for TransactionIdentifierResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

#[derive(Serialize)]
pub struct TransactionIdentifier {
    pub hash: TransactionDigest,
}

#[derive(Deserialize)]
pub struct ConstructionPreprocessRequest {
    pub network_identifier: NetworkIdentifier,
    pub operations: Vec<Operation>,
    pub metadata: Option<Value>,
    pub max_fee: Vec<Amount>,
    pub suggested_fee_multiplier: Option<u64>,
}

#[derive(Serialize)]
pub struct ConstructionPreprocessResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_public_keys: Vec<AccountIdentifier>,
}
impl IntoResponse for ConstructionPreprocessResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}
