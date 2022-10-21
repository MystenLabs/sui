// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt::{Debug, Display, Formatter};
use std::str::FromStr;

use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::de::Error as DeError;
use serde::{Deserialize, Serializer};
use serde::{Deserializer, Serialize};
use serde_json::Value;
use serde_with::serde_as;
use strum_macros::EnumIter;
use strum_macros::EnumString;

use sui_types::base_types::{
    ObjectID, ObjectInfo, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest,
    TRANSACTION_DIGEST_LENGTH,
};
use sui_types::crypto::SignatureScheme;
use sui_types::messages::ExecutionStatus;
use sui_types::sui_serde::Base64;
use sui_types::sui_serde::Hex;
use sui_types::sui_serde::Readable;

use crate::errors::Error;
use crate::operations::Operation;
use crate::{ErrorType, UnsupportedBlockchain, UnsupportedNetwork, SUI};

#[derive(Serialize, Deserialize, Clone)]
pub struct NetworkIdentifier {
    pub blockchain: String,
    pub network: SuiEnv,
}

#[derive(
    Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq, Debug, Clone, Copy, EnumString,
)]
#[strum(serialize_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum SuiEnv {
    MainNet,
    DevNet,
    TestNet,
    LocalNet,
}

impl SuiEnv {
    pub fn check_network_identifier(
        &self,
        network_identifier: &NetworkIdentifier,
    ) -> Result<(), Error> {
        if &network_identifier.blockchain != "sui" {
            return Err(Error::new(UnsupportedBlockchain));
        }
        if &network_identifier.network != self {
            return Err(Error::new(UnsupportedNetwork));
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AccountIdentifier {
    pub address: SuiAddress,
}

impl From<SuiAddress> for AccountIdentifier {
    fn from(address: SuiAddress) -> Self {
        AccountIdentifier { address }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Currency {
    pub symbol: String,
    pub decimals: u64,
}
#[derive(Deserialize)]
pub struct AccountBalanceRequest {
    pub network_identifier: NetworkIdentifier,
    pub account_identifier: AccountIdentifier,
    #[serde(default)]
    pub block_identifier: PartialBlockIdentifier,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub currencies: Vec<Currency>,
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
#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Debug)]
pub struct BlockHash(
    #[serde_as(as = "Readable<Base64, _>")] pub(crate) [u8; TRANSACTION_DIGEST_LENGTH],
);

impl TryFrom<&[u8]> for BlockHash {
    type Error = Error;

    fn try_from(bytes: &[u8]) -> Result<Self, Error> {
        let arr: [u8; TRANSACTION_DIGEST_LENGTH] = bytes
            .try_into()
            .map_err(|e| Error::new_with_cause(ErrorType::InvalidInput, e))?;
        Ok(Self(arr))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Amount {
    pub value: SignedValue,
    pub currency: Currency,
}

#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct SignedValue {
    negative: bool,
    value: u128,
}

impl From<u64> for SignedValue {
    fn from(value: u64) -> Self {
        Self {
            negative: false,
            value: value as u128,
        }
    }
}

impl From<u128> for SignedValue {
    fn from(value: u128) -> Self {
        Self {
            negative: false,
            value,
        }
    }
}

impl From<i128> for SignedValue {
    fn from(value: i128) -> Self {
        Self {
            negative: value.is_negative(),
            value: value.abs() as u128,
        }
    }
}

impl From<i64> for SignedValue {
    fn from(value: i64) -> Self {
        Self {
            negative: value.is_negative(),
            value: value.abs() as u128,
        }
    }
}

impl SignedValue {
    pub fn neg(v: u128) -> Self {
        Self {
            negative: true,
            value: v,
        }
    }
    pub fn is_negative(&self) -> bool {
        self.negative
    }

    pub fn abs(&self) -> u128 {
        self.value
    }

    pub fn add(&mut self, other: &Self) {
        if self.negative ^ other.negative {
            if self.value > other.value {
                self.value -= other.value;
            } else {
                self.value = other.value - self.value;
                self.negative = !self.negative;
            }
        } else {
            self.value += other.value
        }
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
                value: u128::from_str(value).map_err(D::Error::custom)?,
            }
        } else {
            SignedValue {
                negative: false,
                value: u128::from_str(&s).map_err(D::Error::custom)?,
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CoinIdentifier {
    pub identifier: CoinID,
}

#[derive(Clone, Debug)]
pub struct CoinID {
    pub id: ObjectID,
    pub version: SequenceNumber,
}

impl Serialize for CoinID {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        format!("{}:{}", self.id, self.version.value()).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CoinID {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;

        let (id, version) = s.split_at(
            s.find(':')
                .ok_or_else(|| D::Error::custom(format!("Malformed Coin id [{s}].")))?,
        );
        let version = version.trim_start_matches(':');
        let id = ObjectID::from_hex_literal(id).map_err(D::Error::custom)?;
        let version = SequenceNumber::from_u64(u64::from_str(version).map_err(D::Error::custom)?);

        Ok(Self { id, version })
    }
}

#[test]
fn test_coin_id_serde() {
    let id = ObjectID::random();
    let coin_id = CoinID {
        id,
        version: SequenceNumber::from_u64(10),
    };
    let s = serde_json::to_string(&coin_id).unwrap();
    assert_eq!(format!("\"{}:{}\"", id, 10), s);

    let deserialized: CoinID = serde_json::from_str(&s).unwrap();

    assert_eq!(id, deserialized.id);
    assert_eq!(SequenceNumber::from_u64(10), deserialized.version)
}

impl From<ObjectRef> for CoinID {
    fn from((id, version, _): ObjectRef) -> Self {
        Self { id, version }
    }
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
        let pub_key =
            sui_types::crypto::PublicKey::try_from_bytes(self.curve_type.into(), &key_bytes)
                .map_err(|e| Error::new_with_cause(ErrorType::ParsingError, e))?;
        Ok((&pub_key).into())
    }
}

#[derive(Deserialize, Serialize, Copy, Clone)]
#[serde(rename_all = "lowercase")]
pub enum CurveType {
    Secp256k1,
    Edwards25519,
}

impl From<CurveType> for SignatureScheme {
    fn from(type_: CurveType) -> Self {
        match type_ {
            CurveType::Secp256k1 => SignatureScheme::Secp256k1,
            CurveType::Edwards25519 => SignatureScheme::ED25519,
        }
    }
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<Operation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ConstructionMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub public_keys: Vec<PublicKey>,
}

#[derive(Deserialize, Serialize, Copy, Clone, Debug, EnumIter)]
pub enum OperationType {
    // Balance changing operations from TransactionEffect
    GasSpent,
    SuiBalanceChange,
    // Sui transaction types, readonly
    GasBudget,
    TransferSUI,
    Pay,
    PaySui,
    PayAllSui,
    TransferObject,
    Publish,
    MoveCall,
    EpochChange,
    Genesis,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct OperationIdentifier {
    index: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    network_index: Option<u64>,
}

impl From<u64> for OperationIdentifier {
    fn from(index: u64) -> Self {
        OperationIdentifier {
            index,
            network_index: None,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct CoinChange {
    pub coin_identifier: CoinIdentifier,
    pub coin_action: CoinAction,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
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
    pub hex_bytes: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub max_fee: Vec<Amount>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_fee_multiplier: Option<u64>,
}

#[derive(Serialize)]
pub struct ConstructionPreprocessResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<MetadataOptions>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_public_keys: Vec<AccountIdentifier>,
}

#[derive(Serialize, Deserialize)]
pub struct MetadataOptions {
    pub input_objects: Vec<ObjectID>,
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
    pub options: Option<MetadataOptions>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub public_keys: Vec<PublicKey>,
}

#[derive(Serialize)]
pub struct ConstructionMetadataResponse {
    pub metadata: ConstructionMetadata,
    #[serde(default)]
    pub suggested_fee: Vec<Amount>,
}

#[derive(Serialize, Deserialize)]
pub struct ConstructionMetadata {
    pub input_objects: BTreeMap<ObjectID, ObjectInfo>,
}

impl ConstructionMetadata {
    pub fn try_get_info(&self, id: &ObjectID) -> Result<&ObjectInfo, Error> {
        self.input_objects
            .get(id)
            .ok_or_else(|| Error::missing_metadata(id))
    }
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
    pub operation_statuses: Vec<Value>,
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

#[derive(Copy, Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "UPPERCASE")]
pub enum OperationStatus {
    Success,
    Failure,
}

impl From<&ExecutionStatus> for OperationStatus {
    fn from(es: &ExecutionStatus) -> Self {
        match es {
            ExecutionStatus::Success => OperationStatus::Success,
            ExecutionStatus::Failure { .. } => OperationStatus::Failure,
        }
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
#[derive(Deserialize, Default, Debug)]
pub struct PartialBlockIdentifier {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<BlockHash>,
}
#[derive(Deserialize)]
pub struct BlockRequest {
    pub network_identifier: NetworkIdentifier,
    #[serde(default)]
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

#[derive(Default)]
pub struct IndexCounter {
    index: u64,
}

impl IndexCounter {
    pub fn next_idx(&mut self) -> u64 {
        let next = self.index;
        self.index += 1;
        next
    }
}

#[derive(Serialize, Clone)]
pub struct PrefundedAccount {
    pub privkey: String,
    pub account_identifier: AccountIdentifier,
    pub curve_type: CurveType,
    pub currency: Currency,
}
