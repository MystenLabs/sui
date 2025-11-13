// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Debug;
use std::str::FromStr;

use axum::Json;
use axum::response::{IntoResponse, Response};
use fastcrypto::encoding::Hex;
use serde::de::Error as DeError;
use serde::{Deserialize, Serializer};
use serde::{Deserializer, Serialize};
use serde_json::Value;
use strum_macros::EnumIter;
use strum_macros::EnumString;

use sui_rpc::proto::sui::rpc::v2::ExecutionStatus;
use sui_rpc::proto::sui::rpc::v2::TransactionKind;
use sui_rpc::proto::sui::rpc::v2::transaction_kind::Kind;
use sui_sdk_types::Address;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest};
use sui_types::crypto::PublicKey as SuiPublicKey;
use sui_types::crypto::SignatureScheme;
use sui_types::messages_checkpoint::CheckpointDigest;

use crate::SUI;
use crate::errors::{Error, ErrorType};
use crate::operations::Operations;
pub use internal_operation::InternalOperation;

pub mod internal_operation;

pub type BlockHeight = u64;

#[derive(Serialize, Deserialize, Clone, Debug)]
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
            return Err(Error::UnsupportedBlockchain(
                network_identifier.blockchain.clone(),
            ));
        }
        if &network_identifier.network != self {
            return Err(Error::UnsupportedNetwork(network_identifier.network));
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct AccountIdentifier {
    pub address: SuiAddress,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_account: Option<SubAccount>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct SubAccount {
    #[serde(rename = "address")]
    pub account_type: SubAccountType,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum SubAccountType {
    Stake,
    PendingStake,
    EstimatedReward,
}

impl From<SuiAddress> for AccountIdentifier {
    fn from(address: SuiAddress) -> Self {
        AccountIdentifier {
            address,
            sub_account: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Currency {
    pub symbol: String,
    pub decimals: u64,
    #[serde(default)]
    pub metadata: CurrencyMetadata,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Hash)]
pub struct CurrencyMetadata {
    pub coin_type: String,
}

impl Default for CurrencyMetadata {
    fn default() -> Self {
        SUI.metadata.clone()
    }
}

impl Default for Currency {
    fn default() -> Self {
        SUI.clone()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
#[serde(transparent)]
pub struct Currencies(pub Vec<Currency>);

impl Default for Currencies {
    fn default() -> Self {
        Currencies(vec![Currency::default()])
    }
}

fn deserialize_or_default_currencies<'de, D>(deserializer: D) -> Result<Currencies, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<Vec<Currency>> = Option::deserialize(deserializer)?;
    match opt {
        Some(vec) if vec.is_empty() => Ok(Currencies::default()),
        Some(vec) => Ok(Currencies(vec)),
        None => Ok(Currencies::default()),
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AccountBalanceRequest {
    pub network_identifier: NetworkIdentifier,
    pub account_identifier: AccountIdentifier,
    #[serde(default)]
    pub block_identifier: PartialBlockIdentifier,
    #[serde(default, deserialize_with = "deserialize_or_default_currencies")]
    pub currencies: Currencies,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AccountBalanceResponse {
    pub block_identifier: BlockIdentifier,
    pub balances: Vec<Amount>,
}

impl IntoResponse for AccountBalanceResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Copy)]
pub struct BlockIdentifier {
    pub index: BlockHeight,
    pub hash: BlockHash,
}

pub type BlockHash = CheckpointDigest;

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct Amount {
    #[serde(with = "str_format")]
    pub value: i128,
    #[serde(default)]
    pub currency: Currency,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<AmountMetadata>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct AmountMetadata {
    pub sub_balances: Vec<SubBalance>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct SubBalance {
    pub stake_id: Address,
    pub validator: Address,
    #[serde(with = "str_format")]
    pub value: i128,
}

impl Amount {
    pub fn new(value: i128, currency: Option<Currency>) -> Self {
        Self {
            value,
            currency: currency.unwrap_or_default(),
            metadata: None,
        }
    }
    pub fn new_from_sub_balances(sub_balances: Vec<SubBalance>) -> Self {
        let value = sub_balances.iter().map(|b| b.value).sum();

        Self {
            value,
            currency: Currency::default(),
            metadata: Some(AmountMetadata { sub_balances }),
        }
    }
}

mod str_format {
    use std::str::FromStr;

    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(value: &i128, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        value.to_string().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<i128, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        i128::from_str(&s).map_err(Error::custom)
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

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct CoinIdentifier {
    pub identifier: CoinID,
}

#[derive(Clone, Debug, Eq, PartialEq)]
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

#[derive(Serialize, Deserialize, Debug)]
pub struct PublicKey {
    pub hex_bytes: Hex,
    pub curve_type: CurveType,
}

impl From<SuiPublicKey> for PublicKey {
    fn from(pk: SuiPublicKey) -> Self {
        match pk {
            SuiPublicKey::Ed25519(k) => PublicKey {
                hex_bytes: Hex::from_bytes(&k.0),
                curve_type: CurveType::Edwards25519,
            },
            SuiPublicKey::Secp256k1(k) => PublicKey {
                hex_bytes: Hex::from_bytes(&k.0),
                curve_type: CurveType::Secp256k1,
            },
            SuiPublicKey::Secp256r1(k) => PublicKey {
                hex_bytes: Hex::from_bytes(&k.0),
                curve_type: CurveType::Secp256r1,
            },
            SuiPublicKey::ZkLogin(k) => PublicKey {
                hex_bytes: Hex::from_bytes(&k.0),
                curve_type: CurveType::ZkLogin, // inaccurate but added for completeness.
            },
            SuiPublicKey::Passkey(k) => PublicKey {
                hex_bytes: Hex::from_bytes(&k.0),
                curve_type: CurveType::Secp256r1,
            },
        }
    }
}

impl TryInto<SuiAddress> for PublicKey {
    type Error = Error;

    fn try_into(self) -> Result<SuiAddress, Self::Error> {
        let key_bytes = self.hex_bytes.to_vec()?;
        let pub_key = SuiPublicKey::try_from_bytes(self.curve_type.into(), &key_bytes)?;
        Ok((&pub_key).into())
    }
}

#[derive(Deserialize, Serialize, Copy, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub enum CurveType {
    Secp256k1,
    Edwards25519,
    Secp256r1,
    ZkLogin,
}

impl From<CurveType> for SignatureScheme {
    fn from(type_: CurveType) -> Self {
        match type_ {
            CurveType::Secp256k1 => SignatureScheme::Secp256k1,
            CurveType::Edwards25519 => SignatureScheme::ED25519,
            CurveType::Secp256r1 => SignatureScheme::Secp256r1,
            CurveType::ZkLogin => SignatureScheme::ZkLoginAuthenticator,
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

#[derive(Serialize, Deserialize)]
pub struct ConstructionPayloadsRequest {
    pub network_identifier: NetworkIdentifier,
    pub operations: Operations,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ConstructionMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub public_keys: Vec<PublicKey>,
}

#[derive(Deserialize, Serialize, Copy, Clone, Debug, EnumIter, Eq, PartialEq)]
pub enum OperationType {
    // Balance changing operations from TransactionEffect
    Gas,
    SuiBalanceChange,
    StakeReward,
    StakePrinciple,
    // sui-rosetta supported operation type
    PaySui,
    PayCoin,
    Stake,
    WithdrawStake,
    // All other Sui transaction types, readonly
    EpochChange,
    Genesis,
    ConsensusCommitPrologue,
    ProgrammableTransaction,
    AuthenticatorStateUpdate,
    RandomnessStateUpdate,
    EndOfEpochTransaction,
    ProgrammableSystemTransaction,
    Unknown,
}

impl From<&TransactionKind> for OperationType {
    fn from(tx: &TransactionKind) -> Self {
        match tx.kind.and_then(|k| Kind::try_from(k).ok()) {
            Some(Kind::ProgrammableTransaction) => OperationType::ProgrammableTransaction,
            Some(Kind::ChangeEpoch) => OperationType::EpochChange,
            Some(Kind::Genesis) => OperationType::Genesis,
            Some(Kind::ConsensusCommitPrologueV1)
            | Some(Kind::ConsensusCommitPrologueV2)
            | Some(Kind::ConsensusCommitPrologueV3)
            | Some(Kind::ConsensusCommitPrologueV4) => OperationType::ConsensusCommitPrologue,
            Some(Kind::AuthenticatorStateUpdate) => OperationType::AuthenticatorStateUpdate,
            Some(Kind::RandomnessStateUpdate) => OperationType::RandomnessStateUpdate,
            Some(Kind::EndOfEpoch) => OperationType::EndOfEpochTransaction,
            Some(Kind::Unknown) | Some(_) | None => OperationType::Unknown,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, Eq, PartialEq)]
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

#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
pub struct CoinChange {
    pub coin_identifier: CoinIdentifier,
    pub coin_action: CoinAction,
}

#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CoinAction {
    CoinCreated,
    CoinSpent,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ConstructionPayloadsResponse {
    pub unsigned_transaction: Hex,
    pub payloads: Vec<SigningPayload>,
}

impl IntoResponse for ConstructionPayloadsResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SigningPayload {
    pub account_identifier: AccountIdentifier,
    // Rosetta need the hex string without `0x`, cannot use fastcrypto's Hex
    pub hex_bytes: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_type: Option<SignatureType>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum SignatureType {
    Ed25519,
    Ecdsa,
}

#[derive(Deserialize, Serialize)]
pub struct ConstructionCombineRequest {
    pub network_identifier: NetworkIdentifier,
    pub unsigned_transaction: Hex,
    pub signatures: Vec<Signature>,
}

#[derive(Deserialize, Serialize)]
pub struct Signature {
    pub signing_payload: SigningPayload,
    pub public_key: PublicKey,
    pub signature_type: SignatureType,
    pub hex_bytes: Hex,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ConstructionCombineResponse {
    pub signed_transaction: Hex,
}

impl IntoResponse for ConstructionCombineResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

#[derive(Serialize, Deserialize)]
pub struct ConstructionSubmitRequest {
    pub network_identifier: NetworkIdentifier,
    pub signed_transaction: Hex,
}
#[derive(Serialize, Deserialize, Debug)]
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TransactionIdentifier {
    pub hash: TransactionDigest,
}

#[derive(Serialize, Deserialize)]
pub struct ConstructionPreprocessRequest {
    pub network_identifier: NetworkIdentifier,
    pub operations: Operations,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<PreprocessMetadata>,
}

#[derive(Serialize, Deserialize)]
pub struct PreprocessMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ConstructionPreprocessResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<MetadataOptions>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_public_keys: Vec<AccountIdentifier>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MetadataOptions {
    pub internal_operation: InternalOperation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<u64>,
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

#[derive(Serialize, Deserialize)]
pub struct ConstructionMetadataRequest {
    pub network_identifier: NetworkIdentifier,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<MetadataOptions>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub public_keys: Vec<PublicKey>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ConstructionMetadataResponse {
    pub metadata: ConstructionMetadata,
    #[serde(default)]
    pub suggested_fee: Vec<Amount>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConstructionMetadata {
    pub sender: SuiAddress,
    /// `Coin<SUI>` objects to be used as gas
    pub gas_coins: Vec<ObjectRef>,
    /// For PaySui/Stake: extra gas coins to merge into gas
    /// For PayCoin: payment coins of the specified type
    /// For WithdrawStake: stake objects to withdraw
    pub objects: Vec<ObjectRef>,
    /// Party-owned (ConsensusAddress) version of objects
    pub party_objects: Vec<(ObjectID, SequenceNumber)>,
    /// Always refers to SUI balance used
    #[serde(with = "str_format")]
    pub total_coin_value: i128,
    pub gas_price: u64,
    pub budget: u64,
    pub currency: Option<Currency>,
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
    pub operations: Operations,
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
    pub errors: Vec<ErrorType>,
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

#[derive(Copy, Clone, Deserialize, Serialize, Debug, Eq, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum OperationStatus {
    Success,
    Failure,
}

impl From<&ExecutionStatus> for OperationStatus {
    fn from(es: &ExecutionStatus) -> Self {
        if es.success() {
            OperationStatus::Success
        } else {
            OperationStatus::Failure
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Block {
    pub block_identifier: BlockIdentifier,
    pub parent_block_identifier: BlockIdentifier,
    pub timestamp: u64,
    pub transactions: Vec<Transaction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Transaction {
    pub transaction_identifier: TransactionIdentifier,
    pub operations: Operations,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_transactions: Vec<RelatedTransaction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RelatedTransaction {
    network_identifier: NetworkIdentifier,
    transaction_identifier: TransactionIdentifier,
    direction: Direction,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
pub enum Direction {
    Forward,
    Backward,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
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
#[derive(Serialize, Deserialize, Default, Debug)]
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

#[derive(Serialize, Clone)]
pub struct PrefundedAccount {
    pub privkey: String,
    pub account_identifier: AccountIdentifier,
    pub curve_type: CurveType,
    pub currency: Currency,
}

#[cfg(test)]
mod tests {
    use super::*;
    use quick_js::Context;
    use serde_json::json;

    #[tokio::test]
    async fn test_currency_defaults() {
        let expected = Currency {
            symbol: "SUI".to_string(),
            decimals: 9,
            metadata: CurrencyMetadata {
                coin_type:
                    "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI"
                        .to_string(),
            },
        };

        let currency: Currency = serde_json::from_value(json!(
            {
                "symbol": "SUI",
                "decimals": 9,
            }
        ))
        .unwrap();
        assert_eq!(expected, currency);

        let amount: Amount = serde_json::from_value(json!(
            {
                "value": "1000000000",
            }
        ))
        .unwrap();
        assert_eq!(expected, amount.currency);

        let account_balance_request: AccountBalanceRequest = serde_json::from_value(json!(
            {
                "network_identifier": {
                    "blockchain": "sui",
                    "network": "mainnet"
                },
                "account_identifier": {
                    "address": "0xadc3a0bb21840f732435f8b649e99df6b29cd27854dfa4b020e3bee07ea09b96"
                }
            }
        ))
        .unwrap();
        assert_eq!(
            expected,
            account_balance_request.currencies.0.clone().pop().unwrap()
        );

        let account_balance_request: AccountBalanceRequest = serde_json::from_value(json!(
            {
                "network_identifier": {
                    "blockchain": "sui",
                    "network": "mainnet"
                },
                "account_identifier": {
                    "address": "0xadc3a0bb21840f732435f8b649e99df6b29cd27854dfa4b020e3bee07ea09b96"
                },
                "currencies": []
            }
        ))
        .unwrap();
        assert_eq!(
            expected,
            account_balance_request.currencies.0.clone().pop().unwrap()
        );
    }

    #[tokio::test]
    async fn test_metadata_total_coin_value_js_conversion_for_large_balance() {
        #[derive(Serialize, Deserialize, Debug)]
        pub struct TestConstructionMetadata {
            pub sender: SuiAddress,
            pub coins: Vec<ObjectRef>,
            pub objects: Vec<ObjectRef>,
            pub total_coin_value: u64,
            pub gas_price: u64,
            pub budget: u64,
            pub currency: Option<Currency>,
        }

        let test_metadata = TestConstructionMetadata {
            sender: Default::default(),
            coins: vec![],
            objects: vec![],
            total_coin_value: 65_000_004_233_578_496,
            gas_price: 0,
            budget: 0,
            currency: None,
        };
        let test_metadata_json = serde_json::to_string(&test_metadata).unwrap();

        let prod_metadata = ConstructionMetadata {
            sender: Default::default(),
            gas_coins: vec![],
            objects: vec![],
            party_objects: vec![],
            total_coin_value: 65_000_004_233_578_496,
            gas_price: 0,
            budget: 0,
            currency: None,
        };
        let prod_metadata_json = serde_json::to_string(&prod_metadata).unwrap();

        let context = Context::new().unwrap();

        let test_total_coin_value = format!(
            "JSON.parse({:?}).total_coin_value.toString()",
            test_metadata_json
        );
        let js_test_total_coin_value = context.eval_as::<String>(&test_total_coin_value).unwrap();

        let prod_total_coin_value = format!(
            "JSON.parse({:?}).total_coin_value.toString()",
            prod_metadata_json
        );
        let js_prod_total_coin_value = context.eval_as::<String>(&prod_total_coin_value).unwrap();

        assert_eq!("65000004233578500", js_test_total_coin_value);
        assert_eq!("65000004233578496", js_prod_total_coin_value);
    }
}
