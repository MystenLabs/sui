// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use axum::Json;
use std::fmt::Debug;
use std::str::FromStr;

use crate::errors::{Error, ErrorType};
use crate::operations::Operations;
use crate::SUI;
use axum::response::{IntoResponse, Response};
use fastcrypto::encoding::Hex;
use serde::de::Error as DeError;
use serde::{Deserialize, Serializer};
use serde::{Deserializer, Serialize};
use serde_json::Value;
use strum_macros::EnumIter;
use strum_macros::EnumString;
use sui_sdk::rpc_types::{SuiExecutionStatus, SuiTransactionKind};
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest};
use sui_types::committee::EpochId;
use sui_types::crypto::PublicKey as SuiPublicKey;
use sui_types::crypto::SignatureScheme;
use sui_types::governance::{
    ADD_DELEGATION_LOCKED_COIN_FUN_NAME, ADD_DELEGATION_MUL_COIN_FUN_NAME,
};
use sui_types::messages::{
    CallArg, MoveCall, ObjectArg, PaySui, SingleTransactionKind, TransactionData, TransactionKind,
};
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::{SUI_SYSTEM_STATE_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION};

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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AccountIdentifier {
    pub address: SuiAddress,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_account: Option<SubAccount>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SubAccount {
    #[serde(rename = "address")]
    pub account_type: SubAccountType,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum SubAccountType {
    DelegatedSui,
    PendingDelegation,
    LockedSui,
}

impl From<SuiAddress> for AccountIdentifier {
    fn from(address: SuiAddress) -> Self {
        AccountIdentifier {
            address,
            sub_account: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Currency {
    pub symbol: String,
    pub decimals: u64,
}
#[derive(Serialize, Deserialize)]
pub struct AccountBalanceRequest {
    pub network_identifier: NetworkIdentifier,
    pub account_identifier: AccountIdentifier,
    #[serde(default)]
    pub block_identifier: PartialBlockIdentifier,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub currencies: Vec<Currency>,
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

pub type BlockHash = TransactionDigest;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Amount {
    #[serde(with = "str_format")]
    pub value: i128,
    pub currency: Currency,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<AmountMetadata>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AmountMetadata {
    pub lock_until_epoch: EpochId,
}

impl Amount {
    pub fn new(value: i128) -> Self {
        Self {
            value,
            currency: SUI.clone(),
            metadata: None,
        }
    }
    pub fn new_locked(epoch: EpochId, value: i128) -> Self {
        Self {
            value,
            currency: SUI.clone(),
            metadata: Some(AmountMetadata {
                lock_until_epoch: epoch,
            }),
        }
    }
}

mod str_format {
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::str::FromStr;

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

impl From<sui_sdk::rpc_types::Coin> for Coin {
    fn from(coin: sui_sdk::rpc_types::Coin) -> Self {
        Self {
            coin_identifier: CoinIdentifier {
                identifier: CoinID {
                    id: coin.coin_object_id,
                    version: coin.version,
                },
            },
            amount: Amount {
                value: coin.balance as i128,
                currency: SUI.clone(),
                metadata: coin.locked_until_epoch.map(|epoch| AmountMetadata {
                    lock_until_epoch: epoch,
                }),
            },
        }
    }
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

#[derive(Deserialize, Debug)]
pub struct PublicKey {
    pub hex_bytes: Hex,
    pub curve_type: CurveType,
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
}

impl From<CurveType> for SignatureScheme {
    fn from(type_: CurveType) -> Self {
        match type_ {
            CurveType::Secp256k1 => SignatureScheme::Secp256k1,
            CurveType::Edwards25519 => SignatureScheme::ED25519,
            CurveType::Secp256r1 => SignatureScheme::Secp256r1,
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
    // sui-rosetta supported operation type
    PaySui,
    Delegation,
    WithdrawDelegation,
    SwitchDelegation,
    // All other Sui transaction types, readonly
    TransferSUI,
    Pay,
    PayAllSui,
    TransferObject,
    Publish,
    MoveCall,
    EpochChange,
    Genesis,
}

impl From<&SuiTransactionKind> for OperationType {
    fn from(tx: &SuiTransactionKind) -> Self {
        match tx {
            SuiTransactionKind::TransferObject(_) => OperationType::TransferObject,
            SuiTransactionKind::Pay(_) => OperationType::Pay,
            SuiTransactionKind::PaySui(_) => OperationType::PaySui,
            SuiTransactionKind::PayAllSui(_) => OperationType::PayAllSui,
            SuiTransactionKind::Publish(_) => OperationType::Publish,
            SuiTransactionKind::Call(_) => OperationType::MoveCall,
            SuiTransactionKind::TransferSui(_) => OperationType::TransferSUI,
            SuiTransactionKind::ChangeEpoch(_) => OperationType::EpochChange,
            SuiTransactionKind::Genesis(_) => OperationType::Genesis,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TransactionIdentifier {
    pub hash: TransactionDigest,
}

#[derive(Deserialize)]
pub struct ConstructionPreprocessRequest {
    pub network_identifier: NetworkIdentifier,
    pub operations: Operations,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<PreprocessMetadata>,
}

#[derive(Serialize, Deserialize)]
pub enum PreprocessMetadata {
    PaySui,
    Delegation { locked_until_epoch: Option<EpochId> },
}

impl From<TransactionMetadata> for PreprocessMetadata {
    fn from(tx_metadata: TransactionMetadata) -> Self {
        match tx_metadata {
            TransactionMetadata::PaySui(_) => Self::PaySui,
            TransactionMetadata::Delegation {
                locked_until_epoch, ..
            } => Self::Delegation { locked_until_epoch },
        }
    }
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
    pub internal_operation: InternalOperation,
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

#[derive(Serialize, Deserialize, Debug)]
pub struct ConstructionMetadata {
    pub tx_metadata: TransactionMetadata,
    pub sender: SuiAddress,
    pub gas: ObjectRef,
    pub budget: u64,
}

impl IntoResponse for ConstructionMetadataResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum TransactionMetadata {
    PaySui(Vec<ObjectRef>),
    Delegation {
        sui_framework: ObjectRef,
        coins: Vec<ObjectRef>,
        locked_until_epoch: Option<EpochId>,
    },
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

#[derive(Copy, Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "UPPERCASE")]
pub enum OperationStatus {
    Success,
    Failure,
}

impl From<SuiExecutionStatus> for OperationStatus {
    fn from(es: SuiExecutionStatus) -> Self {
        match es {
            SuiExecutionStatus::Success => OperationStatus::Success,
            SuiExecutionStatus::Failure { .. } => OperationStatus::Failure,
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

#[derive(Serialize, Deserialize, Debug)]
pub enum InternalOperation {
    PaySui {
        sender: SuiAddress,
        recipients: Vec<SuiAddress>,
        amounts: Vec<u64>,
    },
    Delegation {
        sender: SuiAddress,
        validator: SuiAddress,
        amount: u128,
        locked_until_epoch: Option<EpochId>,
    },
}

impl InternalOperation {
    pub fn sender(&self) -> SuiAddress {
        match self {
            InternalOperation::PaySui { sender, .. }
            | InternalOperation::Delegation { sender, .. } => *sender,
        }
    }
    /// Combine with ConstructionMetadata to form the TransactionData
    pub fn try_into_data(self, metadata: ConstructionMetadata) -> Result<TransactionData, Error> {
        let single_tx = match (self, metadata.tx_metadata) {
            (
                Self::PaySui {
                    recipients,
                    amounts,
                    ..
                },
                TransactionMetadata::PaySui(coins),
            ) => SingleTransactionKind::PaySui(PaySui {
                coins,
                recipients,
                amounts,
            }),
            (
                InternalOperation::Delegation {
                    validator,
                    amount,
                    locked_until_epoch,
                    ..
                },
                TransactionMetadata::Delegation {
                    sui_framework,
                    coins,
                    ..
                },
            ) => {
                let function = if locked_until_epoch.is_some() {
                    ADD_DELEGATION_LOCKED_COIN_FUN_NAME.to_owned()
                } else {
                    ADD_DELEGATION_MUL_COIN_FUN_NAME.to_owned()
                };
                SingleTransactionKind::Call(MoveCall {
                    package: sui_framework,
                    module: SUI_SYSTEM_MODULE_NAME.to_owned(),
                    function,
                    type_arguments: vec![],
                    arguments: vec![
                        CallArg::Object(ObjectArg::SharedObject {
                            id: SUI_SYSTEM_STATE_OBJECT_ID,
                            initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
                        }),
                        CallArg::ObjVec(
                            coins.into_iter().map(ObjectArg::ImmOrOwnedObject).collect(),
                        ),
                        CallArg::Pure(bcs::to_bytes(&amount)?),
                        CallArg::Pure(bcs::to_bytes(&validator)?),
                    ],
                })
            }
            (op, metadata) => {
                return Err(Error::InternalError(anyhow!(
                "Cannot construct TransactionData from provided operation and metadata, {:?}, {:?}",
                op,
                metadata
            )))
            }
        };

        Ok(TransactionData::new_with_dummy_gas_price(
            TransactionKind::Single(single_tx),
            metadata.sender,
            metadata.gas,
            metadata.budget,
        ))
    }
}
