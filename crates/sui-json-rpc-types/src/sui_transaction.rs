// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::{self, Display, Formatter, Write};

use enum_dispatch::enum_dispatch;
use fastcrypto::encoding::Base64;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::language_storage::TypeTag;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};

use sui_json::SuiJsonValue;
use sui_types::base_types::{
    EpochId, ObjectID, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest,
};
use sui_types::digests::TransactionEventsDigest;
use sui_types::error::{ExecutionError, SuiError};
use sui_types::gas::GasCostSummary;
use sui_types::messages::{
    Argument, Command, ExecuteTransactionRequestType, ExecutionStatus, GenesisObject,
    InputObjectKind, ProgrammableMoveCall, ProgrammableTransaction, SenderSignedData,
    TransactionData, TransactionDataAPI, TransactionEffects, TransactionEffectsAPI,
    TransactionEvents, TransactionKind, VersionedProtocolMessage,
};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::move_package::disassemble_modules;
use sui_types::object::Owner;
use sui_types::parse_sui_type_tag;
use sui_types::query::TransactionFilter;
use sui_types::signature::GenericSignature;

use crate::balance_changes::BalanceChange;
use crate::object_changes::ObjectChange;
use crate::{Page, SuiEvent, SuiMovePackage, SuiObjectRef};

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, PartialEq, Eq, Copy)]
/// Type for de/serializing number to string
pub struct BigInt(
    #[serde_as(as = "DisplayFromStr")]
    #[schemars(with = "String")]
    u64,
);

impl From<BigInt> for u64 {
    fn from(x: BigInt) -> u64 {
        x.0
    }
}

impl From<u64> for BigInt {
    fn from(v: u64) -> BigInt {
        BigInt(v)
    }
}

impl Display for BigInt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", rename = "TransactionResponseQuery", default)]
pub struct SuiTransactionResponseQuery {
    /// If None, no filter will be applied
    pub filter: Option<TransactionFilter>,
    /// config which fields to include in the response, by default only digest is included
    pub options: Option<SuiTransactionResponseOptions>,
}

impl SuiTransactionResponseQuery {
    pub fn new(
        filter: Option<TransactionFilter>,
        options: Option<SuiTransactionResponseOptions>,
    ) -> Self {
        Self { filter, options }
    }

    pub fn new_with_filter(filter: TransactionFilter) -> Self {
        Self {
            filter: Some(filter),
            options: None,
        }
    }
}

pub type TransactionsPage = Page<SuiTransactionResponse, TransactionDigest>;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Eq, PartialEq, Default)]
#[serde(
    rename_all = "camelCase",
    rename = "TransactionResponseOptions",
    default
)]
pub struct SuiTransactionResponseOptions {
    /// Whether to show transaction input data. Default to be False
    pub show_input: bool,
    /// Whether to show bcs-encoded transaction input data
    pub show_raw_input: bool,
    /// Whether to show transaction effects. Default to be False
    pub show_effects: bool,
    /// Whether to show transaction events. Default to be False
    pub show_events: bool,
    /// Whether to show object_changes. Default to be False
    pub show_object_changes: bool,
    /// Whether to show balance_changes. Default to be False
    pub show_balance_changes: bool,
}

impl SuiTransactionResponseOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn full_content() -> Self {
        Self {
            show_effects: true,
            show_input: true,
            show_raw_input: true,
            show_events: true,
            show_object_changes: true,
            show_balance_changes: true,
        }
    }

    pub fn with_input(mut self) -> Self {
        self.show_input = true;
        self
    }

    pub fn with_raw_input(mut self) -> Self {
        self.show_raw_input = true;
        self
    }

    pub fn with_effects(mut self) -> Self {
        self.show_effects = true;
        self
    }

    pub fn with_events(mut self) -> Self {
        self.show_events = true;
        self
    }

    pub fn with_balance_changes(mut self) -> Self {
        self.show_balance_changes = true;
        self
    }

    pub fn with_object_changes(mut self) -> Self {
        self.show_object_changes = true;
        self
    }

    /// default to return `WaitForEffectsCert` unless some options require
    /// local execution
    pub fn default_execution_request_type(&self) -> ExecuteTransactionRequestType {
        // if people want effects or events, they typically want to wait for local execution
        if self.require_effects() {
            ExecuteTransactionRequestType::WaitForLocalExecution
        } else {
            ExecuteTransactionRequestType::WaitForEffectsCert
        }
    }

    pub fn require_local_execution(&self) -> bool {
        self.show_balance_changes || self.show_object_changes
    }

    pub fn require_input(&self) -> bool {
        self.show_input || self.show_raw_input || self.show_object_changes
    }

    pub fn require_effects(&self) -> bool {
        self.show_effects
            || self.show_events
            || self.show_balance_changes
            || self.show_object_changes
    }

    pub fn only_digest(&self) -> bool {
        self == &Self::default()
    }
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Default)]
#[serde(rename_all = "camelCase", rename = "TransactionResponse")]
pub struct SuiTransactionResponse {
    pub digest: TransactionDigest,
    /// Transaction input data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction: Option<SuiTransaction>,
    /// BCS encoded [SenderSignedData] that includes input object references
    /// returns empty array if `show_raw_transaction` is false
    #[serde_as(as = "Base64")]
    #[schemars(with = "Base64")]
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub raw_transaction: Vec<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effects: Option<SuiTransactionEffects>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<SuiTransactionEvents>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_changes: Option<Vec<ObjectChange>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance_changes: Option<Vec<BalanceChange>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmed_local_execution: Option<bool>,
    /// The checkpoint number when this transaction was included and hence finalized.
    /// This is only returned in the read api, not in the transaction execution api.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<CheckpointSequenceNumber>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub errors: Vec<String>,
}

impl SuiTransactionResponse {
    pub fn new(digest: TransactionDigest) -> Self {
        Self {
            digest,
            ..Default::default()
        }
    }

    pub fn status_ok(&self) -> Option<bool> {
        self.effects.as_ref().map(|e| e.status().is_ok())
    }
}

/// We are specifically ignoring events for now until events become more stable.
impl PartialEq for SuiTransactionResponse {
    fn eq(&self, other: &Self) -> bool {
        self.transaction == other.transaction
            && self.effects == other.effects
            && self.timestamp_ms == other.timestamp_ms
            && self.confirmed_local_execution == other.confirmed_local_execution
            && self.checkpoint == other.checkpoint
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename = "TransactionKind", tag = "kind")]
pub enum SuiTransactionKind {
    /// A system transaction that will update epoch information on-chain.
    ChangeEpoch(SuiChangeEpoch),
    /// A system transaction used for initializing the initial state of the chain.
    Genesis(SuiGenesisTransaction),
    /// A system transaction marking the start of a series of transactions scheduled as part of a
    /// checkpoint
    ConsensusCommitPrologue(SuiConsensusCommitPrologue),
    /// A series of commands where the results of one command can be used in future
    /// commands
    ProgrammableTransaction(SuiProgrammableTransaction),
    // .. more transaction types go here
}

impl Display for SuiTransactionKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut writer = String::new();
        match &self {
            Self::ChangeEpoch(e) => {
                writeln!(writer, "Transaction Kind : Epoch Change")?;
                writeln!(writer, "New epoch ID : {}", e.epoch)?;
                writeln!(writer, "Storage gas reward : {}", e.storage_charge)?;
                writeln!(writer, "Computation gas reward : {}", e.computation_charge)?;
                writeln!(writer, "Storage rebate : {}", e.storage_rebate)?;
                writeln!(writer, "Timestamp : {}", e.epoch_start_timestamp_ms)?;
            }
            Self::Genesis(_) => {
                writeln!(writer, "Transaction Kind : Genesis Transaction")?;
            }
            Self::ConsensusCommitPrologue(p) => {
                writeln!(writer, "Transaction Kind : Consensus Commit Prologue")?;
                writeln!(
                    writer,
                    "Epoch: {}, Round: {}, Timestamp : {}",
                    p.epoch, p.round, p.commit_timestamp_ms
                )?;
            }
            Self::ProgrammableTransaction(p) => {
                writeln!(writer, "Transaction Kind : Programmable")?;
                write!(writer, "{p}")?;
            }
        }
        write!(f, "{}", writer)
    }
}

impl TryFrom<TransactionKind> for SuiTransactionKind {
    type Error = anyhow::Error;

    fn try_from(tx: TransactionKind) -> Result<Self, Self::Error> {
        Ok(match tx {
            TransactionKind::ChangeEpoch(e) => Self::ChangeEpoch(SuiChangeEpoch {
                epoch: e.epoch,
                storage_charge: e.storage_charge,
                computation_charge: e.computation_charge,
                storage_rebate: e.storage_rebate,
                epoch_start_timestamp_ms: e.epoch_start_timestamp_ms,
            }),
            TransactionKind::Genesis(g) => Self::Genesis(SuiGenesisTransaction {
                objects: g.objects.iter().map(GenesisObject::id).collect(),
            }),
            TransactionKind::ConsensusCommitPrologue(p) => {
                Self::ConsensusCommitPrologue(SuiConsensusCommitPrologue {
                    epoch: p.epoch,
                    round: p.round,
                    commit_timestamp_ms: p.commit_timestamp_ms,
                })
            }
            TransactionKind::ProgrammableTransaction(p) => {
                Self::ProgrammableTransaction(p.try_into()?)
            }
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SuiChangeEpoch {
    pub epoch: EpochId,
    pub storage_charge: u64,
    pub computation_charge: u64,
    pub storage_rebate: u64,
    pub epoch_start_timestamp_ms: u64,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, PartialEq, Eq)]
#[enum_dispatch(SuiTransactionEffectsAPI)]
#[serde(
    rename = "TransactionEffects",
    rename_all = "camelCase",
    tag = "messageVersion"
)]
pub enum SuiTransactionEffects {
    V1(SuiTransactionEffectsV1),
}

#[enum_dispatch]
pub trait SuiTransactionEffectsAPI {
    fn status(&self) -> &SuiExecutionStatus;
    fn into_status(self) -> SuiExecutionStatus;
    fn shared_objects(&self) -> &[SuiObjectRef];
    fn created(&self) -> &[OwnedObjectRef];
    fn mutated(&self) -> &[OwnedObjectRef];
    fn unwrapped(&self) -> &[OwnedObjectRef];
    fn deleted(&self) -> &[SuiObjectRef];
    fn unwrapped_then_deleted(&self) -> &[SuiObjectRef];
    fn wrapped(&self) -> &[SuiObjectRef];
    fn gas_object(&self) -> &OwnedObjectRef;
    fn events_digest(&self) -> Option<&TransactionEventsDigest>;
    fn dependencies(&self) -> &[TransactionDigest];
    fn executed_epoch(&self) -> EpochId;
    fn transaction_digest(&self) -> &TransactionDigest;
    fn gas_used(&self) -> &SuiGasCostSummary;

    /// Return an iterator of mutated objects, but excluding the gas object.
    fn mutated_excluding_gas(&self) -> Vec<OwnedObjectRef>;
}

/// The response from processing a transaction or a certified transaction
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "TransactionEffectsV1", rename_all = "camelCase")]
pub struct SuiTransactionEffectsV1 {
    /// The status of the execution
    pub status: SuiExecutionStatus,
    /// The epoch when this transaction was executed.
    pub executed_epoch: EpochId,
    pub gas_used: SuiGasCostSummary,
    /// The object references of the shared objects used in this transaction. Empty if no shared objects were used.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shared_objects: Vec<SuiObjectRef>,
    /// The transaction digest
    pub transaction_digest: TransactionDigest,
    /// ObjectRef and owner of new objects created.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub created: Vec<OwnedObjectRef>,
    /// ObjectRef and owner of mutated objects, including gas object.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mutated: Vec<OwnedObjectRef>,
    /// ObjectRef and owner of objects that are unwrapped in this transaction.
    /// Unwrapped objects are objects that were wrapped into other objects in the past,
    /// and just got extracted out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unwrapped: Vec<OwnedObjectRef>,
    /// Object Refs of objects now deleted (the old refs).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deleted: Vec<SuiObjectRef>,
    /// Object refs of objects previously wrapped in other objects but now deleted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unwrapped_then_deleted: Vec<SuiObjectRef>,
    /// Object refs of objects now wrapped in other objects.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wrapped: Vec<SuiObjectRef>,
    /// The updated gas object reference. Have a dedicated field for convenient access.
    /// It's also included in mutated.
    pub gas_object: OwnedObjectRef,
    /// The digest of the events emitted during execution,
    /// can be None if the transaction does not emit any event.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events_digest: Option<TransactionEventsDigest>,
    /// The set of transaction digests this transaction depends on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<TransactionDigest>,
}

impl SuiTransactionEffectsAPI for SuiTransactionEffectsV1 {
    fn status(&self) -> &SuiExecutionStatus {
        &self.status
    }
    fn into_status(self) -> SuiExecutionStatus {
        self.status
    }
    fn shared_objects(&self) -> &[SuiObjectRef] {
        &self.shared_objects
    }
    fn created(&self) -> &[OwnedObjectRef] {
        &self.created
    }
    fn mutated(&self) -> &[OwnedObjectRef] {
        &self.mutated
    }
    fn unwrapped(&self) -> &[OwnedObjectRef] {
        &self.unwrapped
    }
    fn deleted(&self) -> &[SuiObjectRef] {
        &self.deleted
    }
    fn unwrapped_then_deleted(&self) -> &[SuiObjectRef] {
        &self.unwrapped_then_deleted
    }
    fn wrapped(&self) -> &[SuiObjectRef] {
        &self.wrapped
    }
    fn gas_object(&self) -> &OwnedObjectRef {
        &self.gas_object
    }
    fn events_digest(&self) -> Option<&TransactionEventsDigest> {
        self.events_digest.as_ref()
    }
    fn dependencies(&self) -> &[TransactionDigest] {
        &self.dependencies
    }

    fn executed_epoch(&self) -> EpochId {
        self.executed_epoch
    }

    fn transaction_digest(&self) -> &TransactionDigest {
        &self.transaction_digest
    }

    fn gas_used(&self) -> &SuiGasCostSummary {
        &self.gas_used
    }

    fn mutated_excluding_gas(&self) -> Vec<OwnedObjectRef> {
        self.mutated
            .iter()
            .filter(|o| *o != &self.gas_object)
            .cloned()
            .collect()
    }
}

impl SuiTransactionEffects {}

impl TryFrom<TransactionEffects> for SuiTransactionEffects {
    type Error = SuiError;

    fn try_from(effect: TransactionEffects) -> Result<Self, Self::Error> {
        let message_version = effect
            .message_version()
            .expect("TransactionEffects defines message_version()");

        match message_version {
            1 => Ok(SuiTransactionEffects::V1(SuiTransactionEffectsV1 {
                status: effect.status().clone().into(),
                executed_epoch: effect.executed_epoch(),
                gas_used: effect.gas_cost_summary().clone().into(),
                shared_objects: to_sui_object_ref(effect.shared_objects().to_vec()),
                transaction_digest: *effect.transaction_digest(),
                created: to_owned_ref(effect.created().to_vec()),
                mutated: to_owned_ref(effect.mutated().to_vec()),
                unwrapped: to_owned_ref(effect.unwrapped().to_vec()),
                deleted: to_sui_object_ref(effect.deleted().to_vec()),
                unwrapped_then_deleted: to_sui_object_ref(effect.unwrapped_then_deleted().to_vec()),
                wrapped: to_sui_object_ref(effect.wrapped().to_vec()),
                gas_object: OwnedObjectRef {
                    owner: effect.gas_object().1,
                    reference: effect.gas_object().0.into(),
                },
                events_digest: effect.events_digest().copied(),
                dependencies: effect.dependencies().to_vec(),
            })),

            _ => Err(SuiError::UnexpectedVersion(format!(
                "Support for TransactionEffects version {} not implemented",
                message_version
            ))),
        }
    }
}

impl Display for SuiTransactionEffects {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut writer = String::new();
        writeln!(writer, "Status : {:?}", self.status())?;
        if !self.created().is_empty() {
            writeln!(writer, "Created Objects:")?;
            for oref in self.created() {
                writeln!(
                    writer,
                    "  - ID: {} , Owner: {}",
                    oref.reference.object_id, oref.owner
                )?;
            }
        }
        if !self.mutated().is_empty() {
            writeln!(writer, "Mutated Objects:")?;
            for oref in self.mutated() {
                writeln!(
                    writer,
                    "  - ID: {} , Owner: {}",
                    oref.reference.object_id, oref.owner
                )?;
            }
        }
        if !self.deleted().is_empty() {
            writeln!(writer, "Deleted Objects:")?;
            for oref in self.deleted() {
                writeln!(writer, "  - ID: {}", oref.object_id)?;
            }
        }
        if !self.wrapped().is_empty() {
            writeln!(writer, "Wrapped Objects:")?;
            for oref in self.wrapped() {
                writeln!(writer, "  - ID: {}", oref.object_id)?;
            }
        }
        if !self.unwrapped().is_empty() {
            writeln!(writer, "Unwrapped Objects:")?;
            for oref in self.unwrapped() {
                writeln!(
                    writer,
                    "  - ID: {} , Owner: {}",
                    oref.reference.object_id, oref.owner
                )?;
            }
        }
        write!(f, "{}", writer)
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DryRunTransactionResponse {
    pub effects: SuiTransactionEffects,
    pub events: SuiTransactionEvents,
}

#[derive(Eq, PartialEq, Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "TransactionEvents", transparent)]
pub struct SuiTransactionEvents {
    pub data: Vec<SuiEvent>,
}

impl SuiTransactionEvents {
    pub fn try_from(
        events: TransactionEvents,
        tx_digest: TransactionDigest,
        timestamp_ms: Option<u64>,
        resolver: &impl GetModule,
    ) -> Result<Self, SuiError> {
        Ok(Self {
            data: events
                .data
                .into_iter()
                .enumerate()
                .map(|(seq, event)| {
                    SuiEvent::try_from(event, tx_digest, seq as u64, timestamp_ms, resolver)
                })
                .collect::<Result<_, _>>()?,
        })
    }
}

/// The response from processing a dev inspect transaction
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "DevInspectResults", rename_all = "camelCase")]
pub struct DevInspectResults {
    /// Summary of effects that likely would be generated if the transaction is actually run.
    /// Note however, that not all dev-inspect transactions are actually usable as transactions so
    /// it might not be possible actually generate these effects from a normal transaction.
    pub effects: SuiTransactionEffects,
    /// Events that likely would be generated if the transaction is actually run.
    pub events: SuiTransactionEvents,
    /// Execution results (including return values) from executing the transaction commands
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<Vec<SuiExecutionResult>>,
    /// Execution error from executing the transaction commands
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "SuiExecutionResult", rename_all = "camelCase")]
pub struct SuiExecutionResult {
    /// The value of any arguments that were mutably borrowed.
    /// Non-mut borrowed values are not included
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mutable_reference_outputs: Vec<(/* argument */ SuiArgument, Vec<u8>, SuiTypeTag)>,
    /// The return values from the command
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub return_values: Vec<(Vec<u8>, SuiTypeTag)>,
}

type ExecutionResult = (
    /*  mutable_reference_outputs */ Vec<(Argument, Vec<u8>, TypeTag)>,
    /*  return_values */ Vec<(Vec<u8>, TypeTag)>,
);

impl DevInspectResults {
    pub fn new(
        effects: TransactionEffects,
        events: TransactionEvents,
        return_values: Result<Vec<ExecutionResult>, ExecutionError>,
        resolver: &impl GetModule,
    ) -> Result<Self, anyhow::Error> {
        let tx_digest = *effects.transaction_digest();
        let mut error = None;
        let mut results = None;
        match return_values {
            Err(e) => error = Some(e.to_string()),
            Ok(srvs) => {
                results = Some(
                    srvs.into_iter()
                        .map(|srv| {
                            let (mutable_reference_outputs, return_values) = srv;
                            let mutable_reference_outputs = mutable_reference_outputs
                                .into_iter()
                                .map(|(a, bytes, tag)| (a.into(), bytes, SuiTypeTag::from(tag)))
                                .collect();
                            let return_values = return_values
                                .into_iter()
                                .map(|(bytes, tag)| (bytes, SuiTypeTag::from(tag)))
                                .collect();
                            SuiExecutionResult {
                                mutable_reference_outputs,
                                return_values,
                            }
                        })
                        .collect(),
                )
            }
        };
        Ok(Self {
            effects: effects.try_into()?,
            events: SuiTransactionEvents::try_from(events, tx_digest, None, resolver)?,
            results,
            error,
        })
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub enum SuiTransactionBuilderMode {
    /// Regular Sui Transactions that are committed on chain
    Commit,
    /// Simulated transaction that allows calling any Move function with
    /// arbitrary values.
    DevInspect,
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "ExecutionStatus", rename_all = "camelCase", tag = "status")]
pub enum SuiExecutionStatus {
    // Gas used in the success case.
    Success,
    // Gas used in the failed case, and the error.
    Failure { error: String },
}

impl SuiExecutionStatus {
    pub fn is_ok(&self) -> bool {
        matches!(self, SuiExecutionStatus::Success { .. })
    }
    pub fn is_err(&self) -> bool {
        matches!(self, SuiExecutionStatus::Failure { .. })
    }
}

impl From<ExecutionStatus> for SuiExecutionStatus {
    fn from(status: ExecutionStatus) -> Self {
        match status {
            ExecutionStatus::Success => Self::Success,
            ExecutionStatus::Failure {
                error,
                command: None,
            } => Self::Failure {
                error: format!("{error:?}"),
            },
            ExecutionStatus::Failure {
                error,
                command: Some(idx),
            } => Self::Failure {
                error: format!("{error:?} in command {idx}"),
            },
        }
    }
}

fn to_sui_object_ref(refs: Vec<ObjectRef>) -> Vec<SuiObjectRef> {
    refs.into_iter().map(SuiObjectRef::from).collect()
}

fn to_owned_ref(owned_refs: Vec<(ObjectRef, Owner)>) -> Vec<OwnedObjectRef> {
    owned_refs
        .into_iter()
        .map(|(oref, owner)| OwnedObjectRef {
            owner,
            reference: oref.into(),
        })
        .collect()
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "GasCostSummary", rename_all = "camelCase")]
pub struct SuiGasCostSummary {
    pub computation_cost: u64,
    pub storage_cost: u64,
    pub storage_rebate: u64,
}

impl From<GasCostSummary> for SuiGasCostSummary {
    fn from(s: GasCostSummary) -> Self {
        Self {
            computation_cost: s.computation_cost,
            storage_cost: s.storage_cost,
            storage_rebate: s.storage_rebate,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, PartialEq, Eq)]
#[serde(rename = "GasData", rename_all = "camelCase")]
pub struct SuiGasData {
    pub payment: Vec<SuiObjectRef>,
    pub owner: SuiAddress,
    pub price: u64,
    pub budget: u64,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, PartialEq, Eq)]
#[enum_dispatch(SuiTransactionDataAPI)]
#[serde(
    rename = "TransactionData",
    rename_all = "camelCase",
    tag = "messageVersion"
)]
pub enum SuiTransactionData {
    V1(SuiTransactionDataV1),
}

#[enum_dispatch]
pub trait SuiTransactionDataAPI {
    fn transaction(&self) -> &SuiTransactionKind;
    fn sender(&self) -> &SuiAddress;
    fn gas_data(&self) -> &SuiGasData;
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, PartialEq, Eq)]
#[serde(rename = "TransactionDataV1", rename_all = "camelCase")]
pub struct SuiTransactionDataV1 {
    pub transaction: SuiTransactionKind,
    pub sender: SuiAddress,
    pub gas_data: SuiGasData,
}

impl SuiTransactionDataAPI for SuiTransactionDataV1 {
    fn transaction(&self) -> &SuiTransactionKind {
        &self.transaction
    }
    fn sender(&self) -> &SuiAddress {
        &self.sender
    }
    fn gas_data(&self) -> &SuiGasData {
        &self.gas_data
    }
}

impl SuiTransactionData {
    pub fn move_calls(&self) -> Vec<&SuiProgrammableMoveCall> {
        match self {
            Self::V1(data) => match &data.transaction {
                SuiTransactionKind::ProgrammableTransaction(pt) => pt
                    .commands
                    .iter()
                    .filter_map(|command| match command {
                        SuiCommand::MoveCall(c) => Some(&**c),
                        _ => None,
                    })
                    .collect(),
                _ => vec![],
            },
        }
    }
}

impl Display for SuiTransactionData {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::V1(data) => {
                writeln!(f, "{}", data.transaction)?;
                writeln!(f, "Sender: {}", data.sender)?;
                write!(f, "Gas Payment: ")?;
                for payment in &self.gas_data().payment {
                    write!(f, "{} ", payment)?;
                }
                writeln!(f)?;
                writeln!(f, "Gas Owner: {}", data.gas_data.owner)?;
                writeln!(f, "Gas Price: {}", data.gas_data.price)?;
                writeln!(f, "Gas Budget: {}", data.gas_data.budget)
            }
        }
    }
}

impl TryFrom<TransactionData> for SuiTransactionData {
    type Error = anyhow::Error;

    fn try_from(data: TransactionData) -> Result<Self, Self::Error> {
        let message_version = data
            .message_version()
            .expect("TransactionData defines message_version()");
        let sender = data.sender();
        let gas_data = SuiGasData {
            payment: data
                .gas()
                .iter()
                .map(|obj_ref| SuiObjectRef::from(*obj_ref))
                .collect(),
            owner: data.gas_owner(),
            price: data.gas_price(),
            budget: data.gas_budget(),
        };
        let transaction = data.into_kind().try_into()?;
        match message_version {
            1 => Ok(SuiTransactionData::V1(SuiTransactionDataV1 {
                transaction,
                sender,
                gas_data,
            })),
            _ => Err(anyhow::anyhow!(
                "Support for TransactionData version {} not implemented",
                message_version
            )),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, PartialEq, Eq)]
#[serde(rename = "Transaction", rename_all = "camelCase")]
pub struct SuiTransaction {
    pub data: SuiTransactionData,
    pub tx_signatures: Vec<GenericSignature>,
}

impl TryFrom<SenderSignedData> for SuiTransaction {
    type Error = anyhow::Error;

    fn try_from(data: SenderSignedData) -> Result<Self, Self::Error> {
        Ok(Self {
            data: data.intent_message().value.clone().try_into()?,
            tx_signatures: data.tx_signatures().to_vec(),
        })
    }
}

impl Display for SuiTransaction {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut writer = String::new();
        writeln!(writer, "Transaction Signature: {:?}", self.tx_signatures)?;
        write!(writer, "{}", &self.data)?;
        write!(f, "{}", writer)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SuiGenesisTransaction {
    pub objects: Vec<ObjectID>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SuiConsensusCommitPrologue {
    pub epoch: u64,
    pub round: u64,
    pub commit_timestamp_ms: u64,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "TransferObject", rename_all = "camelCase")]
pub struct SuiTransferObject {
    pub recipient: SuiAddress,
    pub object_ref: SuiObjectRef,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "TransferSui", rename_all = "camelCase")]
pub struct SuiTransferSui {
    pub recipient: SuiAddress,
    pub amount: Option<u64>,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "InputObjectKind")]
pub enum SuiInputObjectKind {
    // A Move package, must be immutable.
    MovePackage(ObjectID),
    // A Move object, either immutable, or owned mutable.
    ImmOrOwnedMoveObject(SuiObjectRef),
    // A Move object that's shared and mutable.
    SharedMoveObject {
        id: ObjectID,
        initial_shared_version: SequenceNumber,
        #[serde(default = "default_shared_object_mutability")]
        mutable: bool,
    },
}

/// A series of commands where the results of one command can be used in future
/// commands
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SuiProgrammableTransaction {
    /// Input objects or primitive values
    pub inputs: Vec<SuiJsonValue>,
    /// The commands to be executed sequentially. A failure in any command will
    /// result in the failure of the entire transaction.
    pub commands: Vec<SuiCommand>,
}

impl Display for SuiProgrammableTransaction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Self { inputs, commands } = self;
        writeln!(f, "Inputs: {inputs:?}")?;
        writeln!(f, "Commands: [")?;
        for c in commands {
            writeln!(f, "  {c},")?;
        }
        writeln!(f, "]")
    }
}

impl TryFrom<ProgrammableTransaction> for SuiProgrammableTransaction {
    type Error = anyhow::Error;

    fn try_from(value: ProgrammableTransaction) -> Result<Self, Self::Error> {
        let ProgrammableTransaction { inputs, commands } = value;
        Ok(SuiProgrammableTransaction {
            inputs: inputs
                .into_iter()
                .map(|arg| arg.try_into())
                .collect::<Result<_, _>>()?,
            commands: commands.into_iter().map(SuiCommand::from).collect(),
        })
    }
}

/// A single command in a programmable transaction.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum SuiCommand {
    /// A call to either an entry or a public Move function
    MoveCall(Box<SuiProgrammableMoveCall>),
    /// `(Vec<forall T:key+store. T>, address)`
    /// It sends n-objects to the specified address. These objects must have store
    /// (public transfer) and either the previous owner must be an address or the object must
    /// be newly created.
    TransferObjects(Vec<SuiArgument>, SuiArgument),
    /// `(&mut Coin<T>, u64)` -> `Coin<T>`
    /// It splits off some amount into a new coin
    SplitCoin(SuiArgument, SuiArgument),
    /// `(&mut Coin<T>, Vec<Coin<T>>)`
    /// It merges n-coins into the first coin
    MergeCoins(SuiArgument, Vec<SuiArgument>),
    /// Publishes a Move package
    Publish(SuiMovePackage),
    /// Upgrades a Move package
    Upgrade(SuiMovePackage, Vec<ObjectID>, ObjectID, SuiArgument),
    /// `forall T: Vec<T> -> vector<T>`
    /// Given n-values of the same type, it constructs a vector. For non objects or an empty vector,
    /// the type tag must be specified.
    MakeMoveVec(Option<String>, Vec<SuiArgument>),
}

impl Display for SuiCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MoveCall(p) => {
                write!(f, "MoveCall({p})")
            }
            Self::MakeMoveVec(ty_opt, elems) => {
                write!(f, "MakeMoveVec(")?;
                if let Some(ty) = ty_opt {
                    write!(f, "Some{ty}")?;
                } else {
                    write!(f, "None")?;
                }
                write!(f, ",[")?;
                write_sep(f, elems, ",")?;
                write!(f, "])")
            }
            Self::TransferObjects(objs, addr) => {
                write!(f, "TransferObjects([")?;
                write_sep(f, objs, ",")?;
                write!(f, "],{addr})")
            }
            Self::SplitCoin(coin, amount) => write!(f, "SplitCoin({coin},{amount})"),
            Self::MergeCoins(target, coins) => {
                write!(f, "MergeCoins({target},")?;
                write_sep(f, coins, ",")?;
                write!(f, ")")
            }
            Self::Publish(_bytes) => write!(f, "Publish(_)"),
            Self::Upgrade(_bytes, deps, current_package_id, ticket) => {
                write!(f, "Upgrade({ticket},")?;
                write_sep(f, deps, ",")?;
                write!(f, ", {current_package_id}")?;
                write!(f, ", _)")?;
                write!(f, ")")
            }
        }
    }
}

impl From<Command> for SuiCommand {
    fn from(value: Command) -> Self {
        match value {
            Command::MoveCall(m) => SuiCommand::MoveCall(Box::new((*m).into())),
            Command::TransferObjects(args, arg) => SuiCommand::TransferObjects(
                args.into_iter().map(SuiArgument::from).collect(),
                arg.into(),
            ),
            Command::SplitCoin(a1, a2) => SuiCommand::SplitCoin(a1.into(), a2.into()),
            Command::MergeCoins(arg, args) => SuiCommand::MergeCoins(
                arg.into(),
                args.into_iter().map(SuiArgument::from).collect(),
            ),
            Command::Publish(modules) => SuiCommand::Publish(SuiMovePackage {
                disassembled: disassemble_modules(modules.iter()).unwrap_or_default(),
            }),
            Command::MakeMoveVec(tag_opt, args) => SuiCommand::MakeMoveVec(
                tag_opt.map(|tag| tag.to_string()),
                args.into_iter().map(SuiArgument::from).collect(),
            ),
            Command::Upgrade(modules, dep_ids, current_package_id, ticket) => SuiCommand::Upgrade(
                SuiMovePackage {
                    disassembled: disassemble_modules(modules.iter()).unwrap_or_default(),
                },
                dep_ids,
                current_package_id,
                SuiArgument::from(ticket),
            ),
        }
    }
}

/// An argument to a programmable transaction command
#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum SuiArgument {
    /// The gas coin. The gas coin can only be used by-ref, except for with
    /// `TransferObjects`, which can use it by-value.
    GasCoin,
    /// One of the input objects or primitive values (from
    /// `ProgrammableTransaction` inputs)
    Input(u16),
    /// The result of another command (from `ProgrammableTransaction` commands)
    Result(u16),
    /// Like a `Result` but it accesses a nested result. Currently, the only usage
    /// of this is to access a value from a Move call with multiple return values.
    NestedResult(u16, u16),
}

impl Display for SuiArgument {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GasCoin => write!(f, "GasCoin"),
            Self::Input(i) => write!(f, "Input({i})"),
            Self::Result(i) => write!(f, "Result({i})"),
            Self::NestedResult(i, j) => write!(f, "NestedResult({i},{j})"),
        }
    }
}

impl From<Argument> for SuiArgument {
    fn from(value: Argument) -> Self {
        match value {
            Argument::GasCoin => Self::GasCoin,
            Argument::Input(i) => Self::Input(i),
            Argument::Result(i) => Self::Result(i),
            Argument::NestedResult(i, j) => Self::NestedResult(i, j),
        }
    }
}

/// The command for calling a Move function, either an entry function or a public
/// function (which cannot return references).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SuiProgrammableMoveCall {
    /// The package containing the module and function.
    pub package: ObjectID,
    /// The specific module in the package containing the function.
    pub module: String,
    /// The function to be called.
    pub function: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// The type arguments to the function.
    pub type_arguments: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// The arguments to the function.
    pub arguments: Vec<SuiArgument>,
}

fn write_sep<T: Display>(
    f: &mut Formatter<'_>,
    items: impl IntoIterator<Item = T>,
    sep: &str,
) -> std::fmt::Result {
    let mut xs = items.into_iter().peekable();
    while let Some(x) = xs.next() {
        if xs.peek().is_some() {
            write!(f, "{sep}")?;
        }
        write!(f, "{x}")?;
    }
    Ok(())
}

impl Display for SuiProgrammableMoveCall {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Self {
            package,
            module,
            function,
            type_arguments,
            arguments,
        } = self;
        write!(f, "{package}::{module}::{function}")?;
        if !type_arguments.is_empty() {
            write!(f, "<")?;
            write_sep(f, type_arguments, ",")?;
            write!(f, ">")?;
        }
        write!(f, "(")?;
        write_sep(f, arguments, ",")?;
        write!(f, ")")
    }
}

impl From<ProgrammableMoveCall> for SuiProgrammableMoveCall {
    fn from(value: ProgrammableMoveCall) -> Self {
        let ProgrammableMoveCall {
            package,
            module,
            function,
            type_arguments,
            arguments,
        } = value;
        Self {
            package,
            module: module.to_string(),
            function: function.to_string(),
            type_arguments: type_arguments.into_iter().map(|t| t.to_string()).collect(),
            arguments: arguments.into_iter().map(SuiArgument::from).collect(),
        }
    }
}

const fn default_shared_object_mutability() -> bool {
    true
}

impl From<InputObjectKind> for SuiInputObjectKind {
    fn from(input: InputObjectKind) -> Self {
        match input {
            InputObjectKind::MovePackage(id) => Self::MovePackage(id),
            InputObjectKind::ImmOrOwnedMoveObject(oref) => Self::ImmOrOwnedMoveObject(oref.into()),
            InputObjectKind::SharedMoveObject {
                id,
                initial_shared_version,
                mutable,
            } => Self::SharedMoveObject {
                id,
                initial_shared_version,
                mutable,
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
#[serde(rename = "TypeTag", rename_all = "camelCase")]
pub struct SuiTypeTag(String);

impl TryInto<TypeTag> for SuiTypeTag {
    type Error = anyhow::Error;
    fn try_into(self) -> Result<TypeTag, Self::Error> {
        parse_sui_type_tag(&self.0)
    }
}

impl From<TypeTag> for SuiTypeTag {
    fn from(tag: TypeTag) -> Self {
        Self(format!("{}", tag))
    }
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum RPCTransactionRequestParams {
    TransferObjectRequestParams(TransferObjectParams),
    MoveCallRequestParams(MoveCallParams),
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TransferObjectParams {
    pub recipient: SuiAddress,
    pub object_id: ObjectID,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MoveCallParams {
    pub package_object_id: ObjectID,
    pub module: String,
    pub function: String,
    #[serde(default)]
    pub type_arguments: Vec<SuiTypeTag>,
    pub arguments: Vec<SuiJsonValue>,
}

#[serde_as]
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TransactionBytes {
    /// BCS serialized transaction data bytes without its type tag, as base-64 encoded string.
    pub tx_bytes: Base64,
    /// the gas objects to be used
    pub gas: Vec<SuiObjectRef>,
    /// objects to be used in this transaction
    pub input_objects: Vec<SuiInputObjectKind>,
}

impl TransactionBytes {
    pub fn from_data(data: TransactionData) -> Result<Self, anyhow::Error> {
        Ok(Self {
            tx_bytes: Base64::from_bytes(bcs::to_bytes(&data)?.as_slice()),
            gas: data
                .gas()
                .iter()
                .map(|obj_ref| SuiObjectRef::from(*obj_ref))
                .collect(),
            input_objects: data
                .input_objects()?
                .into_iter()
                .map(SuiInputObjectKind::from)
                .collect(),
        })
    }

    pub fn to_data(self) -> Result<TransactionData, anyhow::Error> {
        bcs::from_bytes::<TransactionData>(&self.tx_bytes.to_vec().map_err(|e| anyhow::anyhow!(e))?)
            .map_err(|e| anyhow::anyhow!(e))
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "OwnedObjectRef")]
pub struct OwnedObjectRef {
    pub owner: Owner,
    pub reference: SuiObjectRef,
}
