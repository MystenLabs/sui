// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prost_types::FieldMask;
use tap::Pipe;

use simulacrum::store::SimulatorStore;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::merge::Merge;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::ExecuteTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::ExecuteTransactionResponse;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2::SimulateTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::SimulateTransactionResponse;
use sui_rpc::proto::sui::rpc::v2::Transaction;
use sui_rpc::proto::sui::rpc::v2::TransactionEffects;
use sui_rpc::proto::sui::rpc::v2::TransactionEvents;
use sui_rpc::proto::sui::rpc::v2::UserSignature;
use sui_rpc::proto::sui::rpc::v2::transaction_execution_service_server::TransactionExecutionService;
use sui_rpc_api::ErrorReason;
use sui_rpc_api::RpcError;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectType;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Object;
use sui_types::transaction::TransactionData;

use crate::context::Context;
use crate::execution;

const EXECUTE_TRANSACTION_READ_MASK_DEFAULT: &str = "effects";

/// A `TransactionExecutionService` implementation backed by the forking `DataStore`/`Simulacrum`.
pub struct ForkingTransactionExecutionService {
    context: Context,
}

impl ForkingTransactionExecutionService {
    pub fn new(context: Context) -> Self {
        Self { context }
    }
}

#[tonic::async_trait]
impl TransactionExecutionService for ForkingTransactionExecutionService {
    async fn execute_transaction(
        &self,
        request: tonic::Request<ExecuteTransactionRequest>,
    ) -> Result<tonic::Response<ExecuteTransactionResponse>, tonic::Status> {
        execute_transaction_impl(&self.context, request.into_inner())
            .await
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    /// Simulation is not yet supported in this fork — Simulacrum does not implement
    /// `TransactionExecutor::simulate_transaction` in our local copy.
    async fn simulate_transaction(
        &self,
        _request: tonic::Request<SimulateTransactionRequest>,
    ) -> Result<tonic::Response<SimulateTransactionResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented(
            "simulate_transaction is not yet supported",
        ))
    }
}

async fn execute_transaction_impl(
    context: &Context,
    request: ExecuteTransactionRequest,
) -> Result<ExecuteTransactionResponse, RpcError> {
    // Parse transaction from proto
    let transaction = request
        .transaction
        .as_ref()
        .ok_or_else(|| FieldViolation::new("transaction").with_reason(ErrorReason::FieldMissing))?
        .pipe(sui_sdk_types::Transaction::try_from)
        .map_err(|e| {
            FieldViolation::new("transaction")
                .with_description(format!("invalid transaction: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;

    // Parse signatures (we don't validate them in forking mode)
    let signatures = request
        .signatures
        .iter()
        .enumerate()
        .map(|(i, signature)| {
            sui_sdk_types::UserSignature::try_from(signature).map_err(|e| {
                FieldViolation::new_at("signatures", i)
                    .with_description(format!("invalid signature: {e}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Validate and parse read_mask
    let read_mask = {
        let read_mask = request
            .read_mask
            .unwrap_or_else(|| FieldMask::from_str(EXECUTE_TRANSACTION_READ_MASK_DEFAULT));
        read_mask
            .validate::<ExecutedTransaction>()
            .map_err(|path| {
                FieldViolation::new("read_mask")
                    .with_description(format!("invalid read_mask path: {path}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })?;
        FieldMaskTree::from(read_mask)
    };

    // Convert to sui_types::TransactionData
    let tx_data: TransactionData = transaction.clone().try_into().map_err(|e| {
        RpcError::new(
            tonic::Code::InvalidArgument,
            format!("Failed to convert transaction: {e}"),
        )
    })?;

    // Execute using shared executor
    let execution::ExecutionResult { effects, .. } =
        execution::execute_transaction(context, tx_data).await?;

    // Build response based on read_mask
    let mut message = ExecutedTransaction::default();

    if read_mask.contains(ExecutedTransaction::DIGEST_FIELD.name) {
        message.digest = Some(transaction.digest().to_string());
    }

    if let Some(submask) = read_mask.subtree(ExecutedTransaction::TRANSACTION_FIELD.name) {
        message.transaction = Some(Transaction::merge_from(transaction.clone(), &submask));
    }

    if let Some(submask) = read_mask.subtree(ExecutedTransaction::SIGNATURES_FIELD.name) {
        message.signatures = signatures
            .into_iter()
            .map(|s| UserSignature::merge_from(s, &submask))
            .collect();
    }

    if let Some(submask) = read_mask.subtree(ExecutedTransaction::EFFECTS_FIELD.name) {
        let effects_sdk: sui_sdk_types::TransactionEffects =
            effects.clone().try_into().map_err(|e| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!("Failed to convert effects: {e}"),
                )
            })?;
        let mut effects_message = TransactionEffects::merge_from(&effects_sdk, &submask);

        if effects_needs_object_type_annotation(&submask) {
            // `merge_from` does not currently populate `object_type` for changed objects.
            // Backfill it from the local store so clients (for example publish dry-run) get
            // the same type metadata they rely on in upstream RPC flows.
            let sim = context.simulacrum.read().await;
            let store = sim.store();
            annotate_effects_object_types(&mut effects_message, |object_id, version| {
                store
                    .get_object_at_version(&object_id, version)
                    .ok()
                    .flatten()
                    .as_ref()
                    .map(object_type_to_string_from_object)
            });
        }

        message.effects = Some(effects_message);
    }

    // Get events if requested
    if let Some(submask) = read_mask.subtree(ExecutedTransaction::EVENTS_FIELD.name) {
        let sim = context.simulacrum.read().await;
        if let Some(events) = sim
            .store()
            .get_transaction_events(effects.transaction_digest())
        {
            let events_sdk: sui_sdk_types::TransactionEvents = events.try_into().map_err(|e| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!("Failed to convert events: {e}"),
                )
            })?;
            message.events = Some(TransactionEvents::merge_from(events_sdk, &submask));
        }
    }

    Ok(ExecuteTransactionResponse::default().with_transaction(message))
}

/// Returns true when the requested effects fields include objects that expose `object_type`.
fn effects_needs_object_type_annotation(mask: &FieldMaskTree) -> bool {
    // Avoid object lookups unless the caller requested fields that include object types.
    mask.contains(TransactionEffects::CHANGED_OBJECTS_FIELD.name)
        || mask.contains(TransactionEffects::UNCHANGED_CONSENSUS_OBJECTS_FIELD.name)
}

/// Best-effort annotation of object type metadata on rendered effects.
///
/// Missing objects are tolerated: if a lookup fails, the corresponding `object_type`
/// remains unset instead of failing the RPC response.
fn annotate_effects_object_types(
    effects: &mut TransactionEffects,
    mut lookup_object_type: impl FnMut(ObjectID, u64) -> Option<String>,
) {
    for changed_object in &mut effects.changed_objects {
        let Some(object_id) = changed_object
            .object_id
            .as_ref()
            .and_then(|id| id.parse::<ObjectID>().ok())
        else {
            continue;
        };

        // Try input version first, then output version. Depending on operation kind and
        // object lifecycle, only one side may be resolvable in a given store.
        let mut versions = [changed_object.input_version, changed_object.output_version]
            .into_iter()
            .flatten()
            .peekable();
        if versions.peek().is_none() {
            continue;
        }

        if let Some(object_type) =
            versions.find_map(|version| lookup_object_type(object_id, version))
        {
            changed_object.object_type = Some(object_type);
        }
    }

    for unchanged_consensus_object in &mut effects.unchanged_consensus_objects {
        let Some(object_id) = unchanged_consensus_object
            .object_id
            .as_ref()
            .and_then(|id| id.parse::<ObjectID>().ok())
        else {
            continue;
        };
        let Some(version) = unchanged_consensus_object.version else {
            continue;
        };
        if let Some(object_type) = lookup_object_type(object_id, version) {
            unchanged_consensus_object.object_type = Some(object_type);
        }
    }
}

/// Formats object type strings using canonical representation expected by RPC clients.
fn object_type_to_string_from_object(object: &Object) -> String {
    match ObjectType::from(object) {
        ObjectType::Package => "package".to_owned(),
        ObjectType::Struct(move_object_type) => move_object_type.to_canonical_string(true),
    }
}
