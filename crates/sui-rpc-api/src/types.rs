// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Chain ID of the current chain
pub const X_SUI_CHAIN_ID: &str = "x-sui-chain-id";

/// Chain name of the current chain
pub const X_SUI_CHAIN: &str = "x-sui-chain";

/// Current checkpoint height
pub const X_SUI_CHECKPOINT_HEIGHT: &str = "x-sui-checkpoint-height";

/// Lowest available checkpoint for which transaction and checkpoint data can be requested.
///
/// Specifically this is the lowest checkpoint for which the following data can be requested:
///  - checkpoints
///  - transactions
///  - effects
///  - events
pub const X_SUI_LOWEST_AVAILABLE_CHECKPOINT: &str = "x-sui-lowest-available-checkpoint";

/// Lowest available checkpoint for which object data can be requested.
///
/// Specifically this is the lowest checkpoint for which input/output object data will be
/// available.
pub const X_SUI_LOWEST_AVAILABLE_CHECKPOINT_OBJECTS: &str =
    "x-sui-lowest-available-checkpoint-objects";

/// Current epoch of the chain
pub const X_SUI_EPOCH: &str = "x-sui-epoch";

/// Cursor to be used for endpoints that support cursor-based pagination. Pass this to the start field of the endpoint on the next call to get the next page of results.
pub const X_SUI_CURSOR: &str = "x-sui-cursor";

/// Current timestamp of the chain - represented as number of milliseconds from the Unix epoch
pub const X_SUI_TIMESTAMP_MS: &str = "x-sui-timestamp-ms";

/// Basic information about the state of a Node
#[serde_with::serde_as]
#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct NodeInfo {
    /// The chain identifier of the chain that this Node is on
    pub chain_id: sui_sdk_types::types::CheckpointDigest,

    /// Human readable name of the chain that this Node is on
    pub chain: std::borrow::Cow<'static, str>,

    /// Current epoch of the Node based on its highest executed checkpoint
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    #[schemars(with = "crate::rest::_schemars::U64")]
    pub epoch: u64,

    /// Checkpoint height of the most recently executed checkpoint
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    #[schemars(with = "crate::rest::_schemars::U64")]
    pub checkpoint_height: u64,

    /// Unix timestamp of the most recently executed checkpoint
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    #[schemars(with = "crate::rest::_schemars::U64")]
    pub timestamp_ms: u64,

    /// The lowest checkpoint for which checkpoints and transaction data is available
    #[serde_as(as = "Option<sui_types::sui_serde::BigInt<u64>>")]
    #[schemars(with = "Option<crate::rest::_schemars::U64>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lowest_available_checkpoint: Option<u64>,

    /// The lowest checkpoint for which object data is available
    #[serde_as(as = "Option<sui_types::sui_serde::BigInt<u64>>")]
    #[schemars(with = "Option<crate::rest::_schemars::U64>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lowest_available_checkpoint_objects: Option<u64>,
    pub software_version: std::borrow::Cow<'static, str>,
}

#[serde_with::serde_as]
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ObjectResponse {
    pub object_id: sui_sdk_types::types::ObjectId,
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    #[schemars(with = "crate::rest::_schemars::U64")]
    pub version: sui_sdk_types::types::Version,
    pub digest: sui_sdk_types::types::ObjectDigest,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<sui_sdk_types::types::Object>,

    #[serde_as(as = "Option<fastcrypto::encoding::Base64>")]
    #[schemars(with = "Option<String>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_bcs: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct GetObjectOptions {
    /// Request that `Object` be included in the response
    ///
    /// Defaults to `true` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<bool>,
    /// Request that `Object` formated as BCS be included in the response
    ///
    /// Defaults to `false` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_bcs: Option<bool>,
}

impl GetObjectOptions {
    pub fn include_object(&self) -> bool {
        self.object.unwrap_or(true)
    }

    pub fn include_object_bcs(&self) -> bool {
        self.object_bcs.unwrap_or(false)
    }
}

#[serde_with::serde_as]
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CheckpointResponse {
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    #[schemars(with = "crate::rest::_schemars::U64")]
    pub sequence_number: sui_sdk_types::types::CheckpointSequenceNumber,

    pub digest: sui_sdk_types::types::CheckpointDigest,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<sui_sdk_types::types::CheckpointSummary>,

    #[serde_as(as = "Option<fastcrypto::encoding::Base64>")]
    #[schemars(with = "Option<String>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_bcs: Option<Vec<u8>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<sui_sdk_types::types::ValidatorAggregatedSignature>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub contents: Option<sui_sdk_types::types::CheckpointContents>,

    #[serde_as(as = "Option<fastcrypto::encoding::Base64>")]
    #[schemars(with = "Option<String>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contents_bcs: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct GetCheckpointOptions {
    /// Request `CheckpointSummary` be included in the response
    ///
    /// Defaults to `true` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<bool>,

    /// Request `CheckpointSummary` encoded as BCS be included in the response
    ///
    /// Defaults to `false` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_bcs: Option<bool>,

    /// Request `ValidatorAggregatedSignature` be included in the response
    ///
    /// Defaults to `true` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<bool>,

    /// Request `CheckpointContents` be included in the response
    ///
    /// Defaults to `false` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contents: Option<bool>,

    /// Request `CheckpointContents` encoded as BCS be included in the response
    ///
    /// Defaults to `false` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contents_bcs: Option<bool>,
}

impl GetCheckpointOptions {
    pub fn include_summary(&self) -> bool {
        self.summary.unwrap_or(true)
    }

    pub fn include_summary_bcs(&self) -> bool {
        self.summary_bcs.unwrap_or(false)
    }

    pub fn include_signature(&self) -> bool {
        self.signature.unwrap_or(true)
    }

    pub fn include_contents(&self) -> bool {
        self.contents.unwrap_or(false)
    }

    pub fn include_contents_bcs(&self) -> bool {
        self.contents_bcs.unwrap_or(false)
    }
}

#[serde_with::serde_as]
#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct TransactionResponse {
    pub digest: sui_sdk_types::types::TransactionDigest,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction: Option<sui_sdk_types::types::Transaction>,

    #[serde_as(as = "Option<fastcrypto::encoding::Base64>")]
    #[schemars(with = "Option<String>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_bcs: Option<Vec<u8>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub signatures: Option<Vec<sui_sdk_types::types::UserSignature>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub effects: Option<sui_sdk_types::types::TransactionEffects>,

    #[serde_as(as = "Option<fastcrypto::encoding::Base64>")]
    #[schemars(with = "Option<String>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effects_bcs: Option<Vec<u8>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<sui_sdk_types::types::TransactionEvents>,

    #[serde_as(as = "Option<fastcrypto::encoding::Base64>")]
    #[schemars(with = "Option<String>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events_bcs: Option<Vec<u8>>,

    #[serde_as(
        as = "Option<sui_types::sui_serde::Readable<sui_types::sui_serde::BigInt<u64>, _>>"
    )]
    #[schemars(with = "Option<crate::rest::_schemars::U64>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<u64>,

    #[serde_as(
        as = "Option<sui_types::sui_serde::Readable<sui_types::sui_serde::BigInt<u64>, _>>"
    )]
    #[schemars(with = "Option<crate::rest::_schemars::U64>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_ms: Option<u64>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct GetTransactionOptions {
    /// Request `Transaction` be included in the response
    ///
    /// Defaults to `true` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction: Option<bool>,

    /// Request `Transaction` encoded as BCS be included in the response
    ///
    /// Defaults to `false` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_bcs: Option<bool>,

    /// Request `Vec<UserSignature>` be included in the response
    ///
    /// Defaults to `true` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signatures: Option<bool>,

    /// Request `TransactionEffects` be included in the response
    ///
    /// Defaults to `true` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effects: Option<bool>,

    /// Request `TransactionEffects` encoded as BCS be included in the response
    ///
    /// Defaults to `false` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effects_bcs: Option<bool>,

    /// Request `TransactionEvents` be included in the response
    ///
    /// Defaults to `true` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<bool>,

    /// Request `TransactionEvents` encoded as BCS be included in the response
    ///
    /// Defaults to `false` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events_bcs: Option<bool>,
}

impl GetTransactionOptions {
    pub fn include_transaction(&self) -> bool {
        self.transaction.unwrap_or(true)
    }

    pub fn include_transaction_bcs(&self) -> bool {
        self.transaction_bcs.unwrap_or(false)
    }

    pub fn include_signatures(&self) -> bool {
        self.signatures.unwrap_or(true)
    }

    pub fn include_effects(&self) -> bool {
        self.effects.unwrap_or(true)
    }

    pub fn include_effects_bcs(&self) -> bool {
        self.effects_bcs.unwrap_or(false)
    }

    pub fn include_events(&self) -> bool {
        self.events.unwrap_or(true)
    }

    pub fn include_events_bcs(&self) -> bool {
        self.events_bcs.unwrap_or(false)
    }
}

/// Options for the execute transaction endpoint
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ExecuteTransactionOptions {
    /// Request `TransactionEffects` be included in the Response.
    ///
    /// Defaults to `true` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effects: Option<bool>,

    /// Request `TransactionEffects` encoded as BCS be included in the Response.
    ///
    /// Defaults to `false` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effects_bcs: Option<bool>,

    /// Request `TransactionEvents` be included in the Response.
    ///
    /// Defaults to `true` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<bool>,

    /// Request `TransactionEvents` encoded as BCS be included in the Response.
    ///
    /// Defaults to `false` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events_bcs: Option<bool>,

    /// Request `BalanceChanges` be included in the Response.
    ///
    /// Defaults to `false` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance_changes: Option<bool>,
    // TODO determine if we want to provide the same level of options for Objects here as we do in
    // the get_object apis
    // /// Request input `Object`s be included in the Response.
    // ///
    // /// Defaults to `false` if not provided.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub input_objects: Option<bool>,

    // /// Request output `Object`s be included in the Response.
    // ///
    // /// Defaults to `false` if not provided.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub output_objects: Option<bool>,
}

impl ExecuteTransactionOptions {
    pub fn include_effects(&self) -> bool {
        self.effects.unwrap_or(true)
    }

    pub fn include_effects_bcs(&self) -> bool {
        self.effects_bcs.unwrap_or(false)
    }

    pub fn include_events(&self) -> bool {
        self.events.unwrap_or(true)
    }

    pub fn include_events_bcs(&self) -> bool {
        self.events_bcs.unwrap_or(false)
    }

    pub fn include_balance_changes(&self) -> bool {
        self.balance_changes.unwrap_or(false)
    }

    pub fn include_input_objects(&self) -> bool {
        false
    }

    pub fn include_output_objects(&self) -> bool {
        false
    }
}

/// Response type for the execute transaction endpoint
#[serde_with::serde_as]
#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ExecuteTransactionResponse {
    pub finality: EffectsFinality,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub effects: Option<sui_sdk_types::types::TransactionEffects>,

    #[serde_as(as = "Option<fastcrypto::encoding::Base64>")]
    #[schemars(with = "Option<String>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effects_bcs: Option<Vec<u8>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<sui_sdk_types::types::TransactionEvents>,

    #[serde_as(as = "Option<fastcrypto::encoding::Base64>")]
    #[schemars(with = "Option<String>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events_bcs: Option<Vec<u8>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance_changes: Option<Vec<sui_sdk_types::types::BalanceChange>>,
    // pub input_objects: Option<Vec<sui_sdk_types::types::Object>>,
    // pub output_objects: Option<Vec<sui_sdk_types::types::Object>>,
}

#[serde_with::serde_as]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "finality", rename_all = "snake_case")]
pub enum EffectsFinality {
    Certified {
        /// Validator aggregated signature
        signature: sui_sdk_types::types::ValidatorAggregatedSignature,
    },
    Checkpointed {
        #[serde_as(as = "sui_types::sui_serde::Readable<sui_types::sui_serde::BigInt<u64>, _>")]
        #[schemars(with = "crate::rest::_schemars::U64")]
        checkpoint: sui_sdk_types::types::CheckpointSequenceNumber,
    },
    QuorumExecuted,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct GetFullCheckpointOptions {
    /// Request `CheckpointSummary` be included in the response
    ///
    /// Defaults to `true` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<bool>,

    /// Request `CheckpointSummary` encoded as BCS be included in the response
    ///
    /// Defaults to `false` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_bcs: Option<bool>,

    /// Request `ValidatorAggregatedSignature` be included in the response
    ///
    /// Defaults to `true` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<bool>,

    /// Request `CheckpointContents` be included in the response
    ///
    /// Defaults to `false` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contents: Option<bool>,

    /// Request `CheckpointContents` encoded as BCS be included in the response
    ///
    /// Defaults to `false` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contents_bcs: Option<bool>,

    /// Request `Transaction` be included in the response
    ///
    /// Defaults to `true` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction: Option<bool>,

    /// Request `Transaction` encoded as BCS be included in the response
    ///
    /// Defaults to `false` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_bcs: Option<bool>,

    /// Request `TransactionEffects` be included in the response
    ///
    /// Defaults to `true` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effects: Option<bool>,

    /// Request `TransactionEffects` encoded as BCS be included in the response
    ///
    /// Defaults to `false` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effects_bcs: Option<bool>,

    /// Request `TransactionEvents` be included in the response
    ///
    /// Defaults to `true` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<bool>,

    /// Request `TransactionEvents` encoded as BCS be included in the response
    ///
    /// Defaults to `false` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events_bcs: Option<bool>,

    /// Request that input objects be included in the response
    ///
    /// Defaults to `true` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_objects: Option<bool>,

    /// Request that output objects be included in the response
    ///
    /// Defaults to `true` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_objects: Option<bool>,

    /// Request that `Object` be included in the response
    ///
    /// Defaults to `true` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<bool>,

    /// Request that `Object` formated as BCS be included in the response
    ///
    /// Defaults to `false` if not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_bcs: Option<bool>,
}

impl GetFullCheckpointOptions {
    pub fn include_summary(&self) -> bool {
        self.summary.unwrap_or(true)
    }

    pub fn include_summary_bcs(&self) -> bool {
        self.summary_bcs.unwrap_or(false)
    }

    pub fn include_signature(&self) -> bool {
        self.signature.unwrap_or(true)
    }

    pub fn include_contents(&self) -> bool {
        self.contents.unwrap_or(false)
    }

    pub fn include_contents_bcs(&self) -> bool {
        self.contents_bcs.unwrap_or(false)
    }

    pub fn include_transaction(&self) -> bool {
        self.transaction.unwrap_or(true)
    }

    pub fn include_transaction_bcs(&self) -> bool {
        self.transaction_bcs.unwrap_or(false)
    }

    pub fn include_effects(&self) -> bool {
        self.effects.unwrap_or(true)
    }

    pub fn include_effects_bcs(&self) -> bool {
        self.effects_bcs.unwrap_or(false)
    }

    pub fn include_events(&self) -> bool {
        self.events.unwrap_or(true)
    }

    pub fn include_events_bcs(&self) -> bool {
        self.events_bcs.unwrap_or(false)
    }

    pub fn include_input_objects(&self) -> bool {
        self.input_objects.unwrap_or(true)
    }

    pub fn include_output_objects(&self) -> bool {
        self.output_objects.unwrap_or(true)
    }

    pub fn include_object(&self) -> bool {
        self.object.unwrap_or(true)
    }

    pub fn include_object_bcs(&self) -> bool {
        self.object_bcs.unwrap_or(false)
    }

    pub fn include_any_transaction_info(&self) -> bool {
        self.include_transaction()
            || self.include_transaction_bcs()
            || self.include_effects()
            || self.include_effects_bcs()
            || self.include_events()
            || self.include_events_bcs()
            || self.include_input_objects()
            || self.include_output_objects()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FullCheckpointResponse {
    pub sequence_number: sui_sdk_types::types::CheckpointSequenceNumber,
    pub digest: sui_sdk_types::types::CheckpointDigest,

    pub summary: Option<sui_sdk_types::types::CheckpointSummary>,
    pub summary_bcs: Option<Vec<u8>>,
    pub signature: Option<sui_sdk_types::types::ValidatorAggregatedSignature>,
    pub contents: Option<sui_sdk_types::types::CheckpointContents>,
    pub contents_bcs: Option<Vec<u8>>,

    pub transactions: Vec<FullCheckpointTransaction>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FullCheckpointTransaction {
    pub digest: sui_sdk_types::types::TransactionDigest,

    pub transaction: Option<sui_sdk_types::types::Transaction>,
    pub transaction_bcs: Option<Vec<u8>>,

    pub effects: Option<sui_sdk_types::types::TransactionEffects>,
    pub effects_bcs: Option<Vec<u8>>,

    pub events: Option<sui_sdk_types::types::TransactionEvents>,
    pub events_bcs: Option<Vec<u8>>,

    pub input_objects: Option<Vec<FullCheckpointObject>>,
    pub output_objects: Option<Vec<FullCheckpointObject>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FullCheckpointObject {
    pub object_id: sui_sdk_types::types::ObjectId,
    pub version: sui_sdk_types::types::Version,
    pub digest: sui_sdk_types::types::ObjectDigest,

    pub object: Option<sui_sdk_types::types::Object>,
    pub object_bcs: Option<Vec<u8>>,
}
