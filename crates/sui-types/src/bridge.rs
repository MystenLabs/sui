// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::ObjectID;
use crate::base_types::SequenceNumber;
use crate::collection_types::LinkedTableNode;
use crate::dynamic_field::{get_dynamic_field_from_store, Field};
use crate::error::SuiResult;
use crate::object::Owner;
use crate::storage::ObjectStore;
use crate::sui_serde::BigInt;
use crate::sui_serde::Readable;
use crate::versioned::Versioned;
use crate::SUI_BRIDGE_OBJECT_ID;
use crate::{
    base_types::SuiAddress,
    collection_types::{Bag, LinkedTable, VecMap},
    error::SuiError,
    id::UID,
};
use enum_dispatch::enum_dispatch;
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use num_enum::TryFromPrimitive;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use strum_macros::Display;

pub type BridgeInnerDynamicField = Field<u64, BridgeInnerV1>;
pub type BridgeRecordDyanmicField = Field<
    MoveTypeBridgeMessageKey,
    LinkedTableNode<MoveTypeBridgeMessageKey, MoveTypeBridgeRecord>,
>;

pub const BRIDGE_MODULE_NAME: &IdentStr = ident_str!("bridge");
pub const BRIDGE_TREASURY_MODULE_NAME: &IdentStr = ident_str!("treasury");
pub const BRIDGE_LIMITER_MODULE_NAME: &IdentStr = ident_str!("limiter");
pub const BRIDGE_COMMITTEE_MODULE_NAME: &IdentStr = ident_str!("committee");
pub const BRIDGE_MESSAGE_MODULE_NAME: &IdentStr = ident_str!("message");
pub const BRIDGE_CREATE_FUNCTION_NAME: &IdentStr = ident_str!("create");
pub const BRIDGE_INIT_COMMITTEE_FUNCTION_NAME: &IdentStr = ident_str!("init_bridge_committee");
pub const BRIDGE_REGISTER_FOREIGN_TOKEN_FUNCTION_NAME: &IdentStr =
    ident_str!("register_foreign_token");
pub const BRIDGE_CREATE_ADD_TOKEN_ON_SUI_MESSAGE_FUNCTION_NAME: &IdentStr =
    ident_str!("create_add_tokens_on_sui_message");
pub const BRIDGE_EXECUTE_SYSTEM_MESSAGE_FUNCTION_NAME: &IdentStr =
    ident_str!("execute_system_message");

pub const BRIDGE_SUPPORTED_ASSET: &[&str] = &["btc", "eth", "usdc", "usdt"];

pub const BRIDGE_COMMITTEE_MINIMAL_VOTING_POWER: u64 = 7500; // out of 10000 (75%)
pub const BRIDGE_COMMITTEE_MAXIMAL_VOTING_POWER: u64 = 10000; // (100%)

// Threshold for action to be approved by the committee (our of 10000)
pub const APPROVAL_THRESHOLD_TOKEN_TRANSFER: u64 = 3334;
pub const APPROVAL_THRESHOLD_EMERGENCY_PAUSE: u64 = 450;
pub const APPROVAL_THRESHOLD_EMERGENCY_UNPAUSE: u64 = 5001;
pub const APPROVAL_THRESHOLD_COMMITTEE_BLOCKLIST: u64 = 5001;
pub const APPROVAL_THRESHOLD_LIMIT_UPDATE: u64 = 5001;
pub const APPROVAL_THRESHOLD_ASSET_PRICE_UPDATE: u64 = 5001;
pub const APPROVAL_THRESHOLD_EVM_CONTRACT_UPGRADE: u64 = 5001;
pub const APPROVAL_THRESHOLD_ADD_TOKENS_ON_SUI: u64 = 5001;
pub const APPROVAL_THRESHOLD_ADD_TOKENS_ON_EVM: u64 = 5001;

// const for initial token ids for convenience
pub const TOKEN_ID_SUI: u8 = 0;
pub const TOKEN_ID_BTC: u8 = 1;
pub const TOKEN_ID_ETH: u8 = 2;
pub const TOKEN_ID_USDC: u8 = 3;
pub const TOKEN_ID_USDT: u8 = 4;

#[derive(
    Debug,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    Clone,
    Copy,
    TryFromPrimitive,
    JsonSchema,
    Hash,
    Display,
)]
#[repr(u8)]
pub enum BridgeChainId {
    SuiMainnet = 0,
    SuiTestnet = 1,
    SuiCustom = 2,

    EthMainnet = 10,
    EthSepolia = 11,
    EthCustom = 12,
}

impl BridgeChainId {
    pub fn is_sui_chain(&self) -> bool {
        matches!(
            self,
            BridgeChainId::SuiMainnet | BridgeChainId::SuiTestnet | BridgeChainId::SuiCustom
        )
    }
}

pub fn get_bridge_obj_initial_shared_version(
    object_store: &dyn ObjectStore,
) -> SuiResult<Option<SequenceNumber>> {
    Ok(object_store
        .get_object(&SUI_BRIDGE_OBJECT_ID)
        .map(|obj| match obj.owner {
            Owner::Shared {
                initial_shared_version,
            } => initial_shared_version,
            _ => unreachable!("Bridge object must be shared"),
        }))
}

/// Bridge provides an abstraction over multiple versions of the inner BridgeInner object.
/// This should be the primary interface to the bridge object in Rust.
/// We use enum dispatch to dispatch all methods defined in BridgeTrait to the actual
/// implementation in the inner types.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[enum_dispatch(BridgeTrait)]
pub enum Bridge {
    V1(BridgeInnerV1),
}

/// Rust version of the Move sui::bridge::Bridge type
/// This repreents the object with 0x9 ID.
/// In Rust, this type should be rarely used since it's just a thin
/// wrapper used to access the inner object.
/// Within this module, we use it to determine the current version of the bridge inner object type,
/// so that we could deserialize the inner object correctly.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BridgeWrapper {
    pub id: UID,
    pub version: Versioned,
}

/// This is the standard API that all bridge inner object type should implement.
#[enum_dispatch]
pub trait BridgeTrait {
    fn bridge_version(&self) -> u64;
    fn message_version(&self) -> u8;
    fn chain_id(&self) -> u8;
    fn sequence_nums(&self) -> &VecMap<u8, u64>;
    fn committee(&self) -> &MoveTypeBridgeCommittee;
    fn treasury(&self) -> &MoveTypeBridgeTreasury;
    fn bridge_records(&self) -> &LinkedTable<MoveTypeBridgeMessageKey>;
    fn frozen(&self) -> bool;
    fn try_into_bridge_summary(self) -> SuiResult<BridgeSummary>;
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BridgeSummary {
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub bridge_version: u64,
    // Message version
    pub message_version: u8,
    /// Self Chain ID
    pub chain_id: u8,
    /// Sequence numbers of all message types
    #[schemars(with = "Vec<(u8, BigInt<u64>)>")]
    #[serde_as(as = "Vec<(_, Readable<BigInt<u64>, _>)>")]
    pub sequence_nums: Vec<(u8, u64)>,
    pub committee: BridgeCommitteeSummary,
    /// Summary of the treasury
    pub treasury: BridgeTreasurySummary,
    /// Object ID of bridge Records (dynamic field)
    pub bridge_records_id: ObjectID,
    /// Summary of the limiter
    pub limiter: BridgeLimiterSummary,
    /// Whether the bridge is currently frozen or not
    pub is_frozen: bool,
    // TODO: add treasury
}

impl Default for BridgeSummary {
    fn default() -> Self {
        BridgeSummary {
            bridge_version: 1,
            message_version: 1,
            chain_id: 1,
            sequence_nums: vec![],
            committee: BridgeCommitteeSummary::default(),
            treasury: BridgeTreasurySummary::default(),
            bridge_records_id: ObjectID::random(),
            limiter: BridgeLimiterSummary::default(),
            is_frozen: false,
        }
    }
}

pub fn get_bridge_wrapper(object_store: &dyn ObjectStore) -> Result<BridgeWrapper, SuiError> {
    let wrapper = object_store
        .get_object(&SUI_BRIDGE_OBJECT_ID)
        // Don't panic here on None because object_store is a generic store.
        .ok_or_else(|| SuiError::SuiBridgeReadError("BridgeWrapper object not found".to_owned()))?;
    let move_object = wrapper.data.try_as_move().ok_or_else(|| {
        SuiError::SuiBridgeReadError("BridgeWrapper object must be a Move object".to_owned())
    })?;
    let result = bcs::from_bytes::<BridgeWrapper>(move_object.contents())
        .map_err(|err| SuiError::SuiBridgeReadError(err.to_string()))?;
    Ok(result)
}

pub fn get_bridge(object_store: &dyn ObjectStore) -> Result<Bridge, SuiError> {
    let wrapper = get_bridge_wrapper(object_store)?;
    let id = wrapper.version.id.id.bytes;
    let version = wrapper.version.version;
    match version {
        1 => {
            let result: BridgeInnerV1 = get_dynamic_field_from_store(object_store, id, &version)
                .map_err(|err| {
                    SuiError::SuiBridgeReadError(format!(
                        "Failed to load bridge inner object with ID {:?} and version {:?}: {:?}",
                        id, version, err
                    ))
                })?;
            Ok(Bridge::V1(result))
        }
        _ => Err(SuiError::SuiBridgeReadError(format!(
            "Unsupported SuiBridge version: {}",
            version
        ))),
    }
}

/// Rust version of the Move bridge::BridgeInner type.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BridgeInnerV1 {
    pub bridge_version: u64,
    pub message_version: u8,
    pub chain_id: u8,
    pub sequence_nums: VecMap<u8, u64>,
    pub committee: MoveTypeBridgeCommittee,
    pub treasury: MoveTypeBridgeTreasury,
    pub bridge_records: LinkedTable<MoveTypeBridgeMessageKey>,
    pub limiter: MoveTypeBridgeTransferLimiter,
    pub frozen: bool,
}

impl BridgeTrait for BridgeInnerV1 {
    fn bridge_version(&self) -> u64 {
        self.bridge_version
    }

    fn message_version(&self) -> u8 {
        self.message_version
    }

    fn chain_id(&self) -> u8 {
        self.chain_id
    }

    fn sequence_nums(&self) -> &VecMap<u8, u64> {
        &self.sequence_nums
    }

    fn committee(&self) -> &MoveTypeBridgeCommittee {
        &self.committee
    }

    fn treasury(&self) -> &MoveTypeBridgeTreasury {
        &self.treasury
    }

    fn bridge_records(&self) -> &LinkedTable<MoveTypeBridgeMessageKey> {
        &self.bridge_records
    }

    fn frozen(&self) -> bool {
        self.frozen
    }

    fn try_into_bridge_summary(self) -> SuiResult<BridgeSummary> {
        let transfer_limit = self
            .limiter
            .transfer_limit
            .contents
            .into_iter()
            .map(|e| {
                let source = BridgeChainId::try_from(e.key.source).map_err(|_e| {
                    SuiError::GenericBridgeError {
                        error: format!("Unrecognized chain id: {}", e.key.source),
                    }
                })?;
                let destination = BridgeChainId::try_from(e.key.destination).map_err(|_e| {
                    SuiError::GenericBridgeError {
                        error: format!("Unrecognized chain id: {}", e.key.destination),
                    }
                })?;
                Ok((source, destination, e.value))
            })
            .collect::<SuiResult<Vec<_>>>()?;
        let supported_tokens = self
            .treasury
            .supported_tokens
            .contents
            .into_iter()
            .map(|e| (e.key, e.value))
            .collect::<Vec<_>>();
        let id_token_type_map = self
            .treasury
            .id_token_type_map
            .contents
            .into_iter()
            .map(|e| (e.key, e.value))
            .collect::<Vec<_>>();
        let transfer_records = self
            .limiter
            .transfer_records
            .contents
            .into_iter()
            .map(|e| {
                let source = BridgeChainId::try_from(e.key.source).map_err(|_e| {
                    SuiError::GenericBridgeError {
                        error: format!("Unrecognized chain id: {}", e.key.source),
                    }
                })?;
                let destination = BridgeChainId::try_from(e.key.destination).map_err(|_e| {
                    SuiError::GenericBridgeError {
                        error: format!("Unrecognized chain id: {}", e.key.destination),
                    }
                })?;
                Ok((source, destination, e.value))
            })
            .collect::<SuiResult<Vec<_>>>()?;
        let limiter = BridgeLimiterSummary {
            transfer_limit,
            transfer_records,
        };
        Ok(BridgeSummary {
            bridge_version: self.bridge_version,
            message_version: self.message_version,
            chain_id: self.chain_id,
            sequence_nums: self
                .sequence_nums
                .contents
                .into_iter()
                .map(|e| (e.key, e.value))
                .collect(),
            committee: BridgeCommitteeSummary {
                members: self
                    .committee
                    .members
                    .contents
                    .into_iter()
                    .map(|e| (e.key, e.value))
                    .collect(),
                member_registration: self
                    .committee
                    .member_registrations
                    .contents
                    .into_iter()
                    .map(|e| (e.key, e.value))
                    .collect(),
                last_committee_update_epoch: self.committee.last_committee_update_epoch,
            },
            bridge_records_id: self.bridge_records.id,
            limiter,
            treasury: BridgeTreasurySummary {
                supported_tokens,
                id_token_type_map,
            },
            is_frozen: self.frozen,
        })
    }
}

/// Rust version of the Move treasury::BridgeTreasury type.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MoveTypeBridgeTreasury {
    pub treasuries: Bag,
    pub supported_tokens: VecMap<String, BridgeTokenMetadata>,
    // Mapping token id to type name
    pub id_token_type_map: VecMap<u8, String>,
    // Bag for storing potential new token waiting to be approved
    pub waiting_room: Bag,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BridgeTokenMetadata {
    pub id: u8,
    pub decimal_multiplier: u64,
    pub notional_value: u64,
    pub native_token: bool,
}

/// Rust version of the Move committee::BridgeCommittee type.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MoveTypeBridgeCommittee {
    pub members: VecMap<Vec<u8>, MoveTypeCommitteeMember>,
    pub member_registrations: VecMap<SuiAddress, MoveTypeCommitteeMemberRegistration>,
    pub last_committee_update_epoch: u64,
}

/// Rust version of the Move committee::CommitteeMemberRegistration type.
#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MoveTypeCommitteeMemberRegistration {
    pub sui_address: SuiAddress,
    pub bridge_pubkey_bytes: Vec<u8>,
    pub http_rest_url: Vec<u8>,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct BridgeCommitteeSummary {
    pub members: Vec<(Vec<u8>, MoveTypeCommitteeMember)>,
    pub member_registration: Vec<(SuiAddress, MoveTypeCommitteeMemberRegistration)>,
    pub last_committee_update_epoch: u64,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct BridgeLimiterSummary {
    pub transfer_limit: Vec<(BridgeChainId, BridgeChainId, u64)>,
    pub transfer_records: Vec<(BridgeChainId, BridgeChainId, MoveTypeBridgeTransferRecord)>,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct BridgeTreasurySummary {
    pub supported_tokens: Vec<(String, BridgeTokenMetadata)>,
    pub id_token_type_map: Vec<(u8, String)>,
}

/// Rust version of the Move committee::CommitteeMember type.
#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Default, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MoveTypeCommitteeMember {
    pub sui_address: SuiAddress,
    pub bridge_pubkey_bytes: Vec<u8>,
    pub voting_power: u64,
    pub http_rest_url: Vec<u8>,
    pub blocklisted: bool,
}

/// Rust version of the Move message::BridgeMessageKey type.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct MoveTypeBridgeMessageKey {
    pub source_chain: u8,
    pub message_type: u8,
    pub bridge_seq_num: u64,
}

/// Rust version of the Move limiter::TransferLimiter type.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MoveTypeBridgeTransferLimiter {
    pub transfer_limit: VecMap<MoveTypeBridgeRoute, u64>,
    pub transfer_records: VecMap<MoveTypeBridgeRoute, MoveTypeBridgeTransferRecord>,
}

/// Rust version of the Move chain_ids::BridgeRoute type.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MoveTypeBridgeRoute {
    pub source: u8,
    pub destination: u8,
}

/// Rust version of the Move limiter::TransferRecord type.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct MoveTypeBridgeTransferRecord {
    hour_head: u64,
    hour_tail: u64,
    per_hour_amounts: Vec<u64>,
    total_amount: u64,
}

/// Rust version of the Move message::BridgeMessage type.
#[derive(Debug, Serialize, Deserialize)]
pub struct MoveTypeBridgeMessage {
    pub message_type: u8,
    pub message_version: u8,
    pub seq_num: u64,
    pub source_chain: u8,
    pub payload: Vec<u8>,
}

/// Rust version of the Move message::BridgeMessage type.
#[derive(Debug, Serialize, Deserialize)]
pub struct MoveTypeBridgeRecord {
    pub message: MoveTypeBridgeMessage,
    pub verified_signatures: Option<Vec<Vec<u8>>>,
    pub claimed: bool,
}

pub fn is_bridge_committee_initiated(object_store: &dyn ObjectStore) -> SuiResult<bool> {
    match get_bridge(object_store) {
        Ok(bridge) => Ok(!bridge.committee().members.contents.is_empty()),
        Err(SuiError::SuiBridgeReadError(..)) => Ok(false),
        Err(other) => Err(other),
    }
}

/// Rust version of the Move message::TokenTransferPayload type.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct MoveTypeTokenTransferPayload {
    pub sender_address: Vec<u8>,
    pub target_chain: u8,
    pub target_address: Vec<u8>,
    pub token_type: u8,
    pub amount: u64,
}

/// Rust version of the Move message::ParsedTokenTransferMessage type.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct MoveTypeParsedTokenTransferMessage {
    pub message_version: u8,
    pub seq_num: u64,
    pub source_chain: u8,
    pub payload: Vec<u8>,
    pub parsed_payload: MoveTypeTokenTransferPayload,
}
