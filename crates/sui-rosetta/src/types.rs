// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::{Display, Formatter};
use std::str::FromStr;

use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::de::Error as DeError;
use serde::{Deserialize, Serializer};
use serde::{Deserializer, Serialize};
use serde_json::{json, Value};
use serde_with::serde_as;
use strum_macros::EnumIter;

use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest, TRANSACTION_DIGEST_LENGTH};
use sui_types::crypto::SignatureScheme;
use sui_types::sui_serde::Base64;
use sui_types::sui_serde::Hex;
use sui_types::sui_serde::Readable;

use crate::actions::SuiAction;
use crate::errors::Error;
use crate::{ErrorType, SUI};

#[derive(Serialize, Deserialize, Clone)]
pub struct NetworkIdentifier {
    pub blockchain: String,
    pub network: SuiEnv,
}

#[derive(Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq, Debug, Clone, Copy)]
#[serde(rename_all = "lowercase")]
#[allow(clippy::enum_variant_names)]
pub enum SuiEnv {
    MainNet,
    DevNet,
    TestNet,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AccountIdentifier {
    pub address: SuiAddress,
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

#[derive(Serialize, Deserialize, Clone)]
pub struct BlockIdentifier {
    pub index: u64,
    pub hash: BlockHash,
}
#[serde_as]
#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct BlockHash(
    #[serde_as(as = "Readable<Base64, _>")] pub(crate) [u8; TRANSACTION_DIGEST_LENGTH],
);

#[derive(Serialize, Deserialize, Clone)]
pub struct Amount {
    pub value: SignedValue,
    pub currency: Currency,
}

#[derive(Clone)]
pub struct SignedValue {
    negative: bool,
    value: u64,
}

impl From<u64> for SignedValue {
    fn from(value: u64) -> Self {
        Self {
            negative: false,
            value,
        }
    }
}

impl SignedValue {
    pub fn neg(v: u64) -> Self {
        Self {
            negative: true,
            value: v,
        }
    }
    pub fn is_negative(&self) -> bool {
        self.negative
    }

    pub fn abs(&self) -> u64 {
        self.value
    }
}

impl Display for SignedValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.negative {
            write!(f, "-")?;
        }
        write!(f, "{}", self.value)
    }
}

impl Serialize for SignedValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SignedValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(if let Some(value) = s.strip_prefix('-') {
            SignedValue {
                negative: true,
                value: u64::from_str(value).map_err(D::Error::custom)?,
            }
        } else {
            SignedValue {
                negative: false,
                value: u64::from_str(&s).map_err(D::Error::custom)?,
            }
        })
    }
}

impl Amount {
    pub fn new(value: SignedValue) -> Self {
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

#[derive(Serialize, Deserialize, Clone)]
pub struct CoinIdentifier {
    pub identifier: ObjectID,
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
        let pub_key = sui_types::crypto::PublicKey::try_from_bytes(curve, &key_bytes)
            .map_err(|e| Error::new_with_cause(ErrorType::ParsingError, e))?;
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

#[derive(Deserialize, Serialize, Clone)]
pub struct Operation {
    pub operation_identifier: OperationIdentifier,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_operations: Vec<OperationIdentifier>,
    #[serde(rename = "type")]
    pub type_: OperationType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<AccountIdentifier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<Amount>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coin_change: Option<CoinChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

impl Operation {
    pub fn from_actions(actions: Vec<SuiAction>) -> Vec<Operation> {
        actions
            .into_iter()
            .flat_map(Vec::<Operation>::from)
            .collect::<Vec<_>>()
    }
    pub fn budget(index: u64, budget: u64, gas: Option<ObjectID>) -> Self {
        Self {
            operation_identifier: OperationIdentifier {
                index,
                network_index: None,
            },
            related_operations: vec![],
            type_: OperationType::GasBudget,
            status: None,
            account: None,
            amount: Some(Amount {
                value: budget.into(),
                currency: SUI.clone(),
            }),
            coin_change: gas.map(|gas| CoinChange {
                coin_identifier: CoinIdentifier { identifier: gas },
                coin_action: CoinAction::CoinSpent,
            }),
            metadata: None,
        }
    }
}

#[derive(Deserialize, Serialize, Copy, Clone, Debug)]
pub enum OperationType {
    GasBudget,
    TransferSUI,
    TransferCoin,
    MergeCoins,
    SplitCoin,
    // Readonly
    Publish,
    MoveCall,
    EpochChange
}

#[derive(Deserialize, Serialize, Clone)]
pub struct OperationIdentifier {
    pub index: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_index: Option<u64>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct CoinChange {
    pub coin_identifier: CoinIdentifier,
    pub coin_action: CoinAction,
}

#[derive(Deserialize, Serialize, Clone)]
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

#[derive(Serialize, Deserialize, Clone)]
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
#[derive(Deserialize)]
pub struct ConstructionHashRequest {
    pub network_identifier: NetworkIdentifier,
    pub signed_transaction: Hex,
}

#[derive(Deserialize)]
pub struct ConstructionMetadataRequest {
    pub network_identifier: NetworkIdentifier,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub public_keys: Vec<PublicKey>,
}

#[derive(Serialize)]
pub struct ConstructionMetadataResponse {
    pub metadata: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_fee: Vec<Amount>,
}

impl IntoResponse for ConstructionMetadataResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

#[derive(Deserialize)]
pub struct ConstructionParseRequest {
    pub network_identifier: NetworkIdentifier,
    pub signed: bool,
    pub transaction: Hex,
}

#[derive(Serialize)]
pub struct ConstructionParseResponse {
    pub operations: Vec<Operation>,
    pub account_identifier_signers: Vec<AccountIdentifier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

impl IntoResponse for ConstructionParseResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

#[derive(Deserialize)]
pub struct NetworkRequest {
    pub network_identifier: NetworkIdentifier,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Serialize)]
pub struct NetworkStatusResponse {
    pub current_block_identifier: BlockIdentifier,
    pub current_block_timestamp: u64,
    pub genesis_block_identifier: BlockIdentifier,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oldest_block_identifier: Option<BlockIdentifier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_status: Option<SyncStatus>,
    pub peers: Vec<Peer>,
}

impl IntoResponse for NetworkStatusResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

#[derive(Serialize)]
pub struct SyncStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_index: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_index: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synced: Option<bool>,
}
#[derive(Serialize)]
pub struct Peer {
    pub peer_id: SuiAddress,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Serialize)]
pub struct NetworkOptionsResponse {
    pub version: Version,
    pub allow: Allow,
}

impl IntoResponse for NetworkOptionsResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

#[derive(Serialize)]
pub struct Version {
    pub rosetta_version: String,
    pub node_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub middleware_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Serialize)]
pub struct Allow {
    pub operation_statuses: Vec<OperationStatus>,
    pub operation_types: Vec<OperationType>,
    pub errors: Vec<Error>,
    pub historical_balance_lookup: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp_start_index: Option<u64>,
    pub call_methods: Vec<String>,
    pub balance_exemptions: Vec<BalanceExemption>,
    pub mempool_coins: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_hash_case: Option<Case>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transaction_hash_case: Option<Case>,
}

#[derive(Debug, EnumIter)]
pub enum OperationStatus {
    Success,
}

impl OperationStatus {
    fn is_successful(&self) -> bool {
        match self {
            OperationStatus::Success => true,
        }
    }
}

impl Serialize for OperationStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        json!({ "status" : format!("{self:?}").to_uppercase(), "successful" : self.is_successful() })
            .serialize(serializer)
    }
}

#[derive(Serialize)]
pub struct BalanceExemption {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_account_address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<Currency>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exemption_type: Option<ExemptionType>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum ExemptionType {
    GreaterOrEqual,
    LessOrEqual,
    Dynamic,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(clippy::enum_variant_names, dead_code)]
pub enum Case {
    UpperCase,
    LowerCase,
    CaseSensitive,
    Null,
}

#[derive(Serialize, Clone)]
pub struct Block {
    pub block_identifier: BlockIdentifier,
    pub parent_block_identifier: BlockIdentifier,
    pub timestamp: u64,
    pub transactions: Vec<Transaction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Serialize, Clone)]
pub struct Transaction {
    pub transaction_identifier: TransactionIdentifier,
    pub operations: Vec<Operation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_transactions: Vec<RelatedTransaction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Serialize, Clone)]
pub struct RelatedTransaction {
    network_identifier: NetworkIdentifier,
    transaction_identifier: TransactionIdentifier,
    direction: Direction,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
pub enum Direction {
    Forward,
    Backward,
}

#[derive(Serialize, Clone)]
pub struct BlockResponse {
    pub block: Block,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub other_transactions: Vec<TransactionIdentifier>,
}

impl IntoResponse for BlockResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}
#[derive(Deserialize)]
pub struct PartialBlockIdentifier {
    pub index: Option<u64>,
    pub hash: Option<BlockHash>,
}
#[derive(Deserialize)]
pub struct BlockRequest {
    pub network_identifier: NetworkIdentifier,
    pub block_identifier: PartialBlockIdentifier,
}

#[derive(Deserialize)]
pub struct BlockTransactionRequest {
    pub network_identifier: NetworkIdentifier,
    pub block_identifier: BlockIdentifier,
    pub transaction_identifier: TransactionIdentifier,
}

#[derive(Serialize)]
pub struct BlockTransactionResponse {
    pub transaction: Transaction,
}

impl IntoResponse for BlockTransactionResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}
