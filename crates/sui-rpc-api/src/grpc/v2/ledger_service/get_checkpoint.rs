// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;
use crate::error::CheckpointNotFoundError;
use crate::read_mask_defaults;
use mysten_common::ZipDebugEqIteratorExt;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::merge::Merge;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::BatchGetCheckpointsRequest;
use sui_rpc::proto::sui::rpc::v2::BatchGetCheckpointsResponse;
use sui_rpc::proto::sui::rpc::v2::Checkpoint;
use sui_rpc::proto::sui::rpc::v2::Event;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointRequest;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointResponse;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointResult;
use sui_rpc::proto::sui::rpc::v2::ObjectSet;
use sui_rpc::proto::sui::rpc::v2::TransactionEvents;
use sui_rpc::proto::sui::rpc::v2::get_checkpoint_request::CheckpointId;
use sui_sdk_types::Digest;
use sui_types::balance_change::derive_balance_changes_2;
use sui_types::full_checkpoint_content::ObjectSet as TypesObjectSet;

pub const READ_MASK_DEFAULT: &str = crate::read_mask_defaults::CHECKPOINT;
pub const MAX_BATCH_REQUESTS: usize = 100;

enum ResolvedCheckpointId {
    Latest,
    SequenceNumber(u64),
    Digest(Digest),
}

#[tracing::instrument(skip(service))]
pub fn get_checkpoint(
    service: &RpcService,
    request: GetCheckpointRequest,
) -> Result<GetCheckpointResponse, RpcError> {
    let read_mask =
        read_mask_defaults::validate_read_mask::<Checkpoint>(request.read_mask, READ_MASK_DEFAULT)?;

    let checkpoint_id = match request.checkpoint_id {
        Some(CheckpointId::SequenceNumber(s)) => ResolvedCheckpointId::SequenceNumber(s),
        Some(CheckpointId::Digest(digest)) => {
            let digest = digest.parse::<Digest>().map_err(|e| {
                FieldViolation::new("digest")
                    .with_description(format!("invalid digest: {e}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })?;
            ResolvedCheckpointId::Digest(digest)
        }
        None => ResolvedCheckpointId::Latest,
        _ => ResolvedCheckpointId::Latest,
    };

    let latest_checkpoint = service
        .reader
        .inner()
        .get_latest_checkpoint()?
        .sequence_number;
    let lowest_available_checkpoint = service.reader.get_lowest_available_checkpoint()?;
    let mut json_budget = service.config.max_json_move_value_response_size();

    get_checkpoint_impl(
        service,
        checkpoint_id,
        &read_mask,
        lowest_available_checkpoint,
        latest_checkpoint,
        &mut json_budget,
    )
    .map(GetCheckpointResponse::new)
}

#[tracing::instrument(skip(service))]
pub fn batch_get_checkpoints(
    service: &RpcService,
    BatchGetCheckpointsRequest {
        requests,
        read_mask,
        ..
    }: BatchGetCheckpointsRequest,
) -> Result<BatchGetCheckpointsResponse, RpcError> {
    if requests.len() > MAX_BATCH_REQUESTS {
        return Err(RpcError::new(
            tonic::Code::InvalidArgument,
            format!("number of batch requests exceed limit of {MAX_BATCH_REQUESTS}"),
        ));
    }

    let read_mask =
        read_mask_defaults::validate_read_mask::<Checkpoint>(read_mask, READ_MASK_DEFAULT)?;

    let checkpoint_ids = requests
        .into_iter()
        .enumerate()
        .map(|(idx, request)| match request.checkpoint_id {
            Some(CheckpointId::SequenceNumber(s)) => Ok(ResolvedCheckpointId::SequenceNumber(s)),
            Some(CheckpointId::Digest(digest)) => digest
                .parse::<Digest>()
                .map(ResolvedCheckpointId::Digest)
                .map_err(|e| {
                    FieldViolation::new("digest")
                        .with_description(format!("invalid digest: {e}"))
                        .with_reason(ErrorReason::FieldInvalid)
                        .nested_at("requests", idx)
                        .into()
                }),
            None => Ok(ResolvedCheckpointId::Latest),
            _ => Ok(ResolvedCheckpointId::Latest),
        })
        .collect::<Result<Vec<_>, RpcError>>()?;

    let latest_checkpoint = service
        .reader
        .inner()
        .get_latest_checkpoint()?
        .sequence_number;
    let lowest_available_checkpoint = service.reader.get_lowest_available_checkpoint()?;
    let mut json_budget = service.config.max_json_move_value_response_size();

    let checkpoints = checkpoint_ids
        .into_iter()
        .map(|checkpoint_id| {
            get_checkpoint_impl(
                service,
                checkpoint_id,
                &read_mask,
                lowest_available_checkpoint,
                latest_checkpoint,
                &mut json_budget,
            )
        })
        .map(|result| match result {
            Ok(checkpoint) => GetCheckpointResult::new_checkpoint(checkpoint),
            Err(error) => GetCheckpointResult::new_error(error.into_status_proto()),
        })
        .collect();

    Ok(BatchGetCheckpointsResponse::new(checkpoints))
}

fn get_checkpoint_impl(
    service: &RpcService,
    checkpoint_id: ResolvedCheckpointId,
    read_mask: &FieldMaskTree,
    lowest_available_checkpoint: u64,
    latest_checkpoint: u64,
    json_budget: &mut usize,
) -> Result<Checkpoint, RpcError> {
    let verified_summary = match checkpoint_id {
        // `Latest` resolves to the tip the caller read, so every entry in a
        // batch sees the same checkpoint and the bounds check below can't
        // race with a newer checkpoint landing mid-request.
        ResolvedCheckpointId::Latest => service
            .reader
            .inner()
            .get_checkpoint_by_sequence_number(latest_checkpoint)
            .ok_or(CheckpointNotFoundError::sequence_number(latest_checkpoint))?,
        ResolvedCheckpointId::SequenceNumber(s) => service
            .reader
            .inner()
            .get_checkpoint_by_sequence_number(s)
            .ok_or(CheckpointNotFoundError::sequence_number(s))?,
        ResolvedCheckpointId::Digest(digest) => service
            .reader
            .inner()
            .get_checkpoint_by_digest(&digest.into())
            .ok_or(CheckpointNotFoundError::digest(digest))?,
    };

    let summary = verified_summary.data();
    let signature = verified_summary.auth_sig();
    let sequence_number = summary.sequence_number;
    let timestamp_ms = summary.timestamp_ms;

    if !(lowest_available_checkpoint..=latest_checkpoint).contains(&sequence_number) {
        return Err(CheckpointNotFoundError::sequence_number(sequence_number).into());
    }

    let mut checkpoint = Checkpoint::default();

    checkpoint.merge(summary, read_mask);
    checkpoint.merge(signature.clone(), read_mask);

    if read_mask.contains(Checkpoint::CONTENTS_FIELD.name)
        || read_mask.contains(Checkpoint::TRANSACTIONS_FIELD.name)
        || read_mask.contains(Checkpoint::OBJECTS_FIELD.name)
    {
        let core_contents = service
            .reader
            .inner()
            .get_checkpoint_contents_by_sequence_number(sequence_number)
            .ok_or(CheckpointNotFoundError::sequence_number(sequence_number))?;

        if read_mask.contains(Checkpoint::CONTENTS_FIELD.name) {
            checkpoint.merge(core_contents.clone(), read_mask);
        }

        if read_mask.contains(Checkpoint::TRANSACTIONS_FIELD.name)
            || read_mask.contains(Checkpoint::OBJECTS_FIELD.name)
        {
            let checkpoint_data = service
                .reader
                .inner()
                .get_checkpoint_data(verified_summary, core_contents)?;

            if let Some(submask) = read_mask
                .subtree(Checkpoint::OBJECTS_FIELD)
                .and_then(|submask| submask.subtree(ObjectSet::OBJECTS_FIELD))
            {
                let set = checkpoint_data
                    .object_set
                    .iter()
                    .map(|o| sui_rpc::proto::sui::rpc::v2::Object::merge_from(o, &submask))
                    .collect();
                checkpoint.objects = Some(ObjectSet::default().with_objects(set));
            }

            if let Some(submask) = read_mask.subtree(Checkpoint::TRANSACTIONS_FIELD.name) {
                // Share a single JSON-rendering budget across every event in
                // every transaction in the checkpoint (and across all
                // checkpoints of a batch). Without this, an unauthenticated
                // request with a permissive `read_mask` multiplies one input
                // checkpoint into thousands of per-event renders, each with
                // its own `max_json_move_value_size` budget.
                checkpoint.transactions = checkpoint_data
                    .transactions
                    .into_iter()
                    .map(|t| {
                        let balance_changes =
                            if submask.contains(ExecutedTransaction::BALANCE_CHANGES_FIELD) {
                                derive_balance_changes_2(&t.effects, &checkpoint_data.object_set)
                                    .into_iter()
                                    .map(Into::into)
                                    .collect()
                            } else {
                                Vec::new()
                            };
                        let mut transaction = ExecutedTransaction::merge_from(&t, &submask);
                        transaction.checkpoint = submask
                            .contains(ExecutedTransaction::CHECKPOINT_FIELD)
                            .then_some(sequence_number);
                        transaction.timestamp = submask
                            .contains(ExecutedTransaction::TIMESTAMP_FIELD)
                            .then(|| sui_rpc::proto::timestamp_ms_to_proto(timestamp_ms));
                        transaction.balance_changes = balance_changes;

                        if let Some(events_mask) =
                            submask.subtree(ExecutedTransaction::EVENTS_FIELD.name)
                            && let Some(event_mask) =
                                events_mask.subtree(TransactionEvents::EVENTS_FIELD.name)
                            && event_mask.contains(Event::JSON_FIELD.name)
                            && let Some(events) = transaction.events.as_mut()
                            && let Some(sdk_events) = &t.events
                        {
                            for (message, event) in
                                events.events.iter_mut().zip_debug_eq(&sdk_events.data)
                            {
                                message.json = service
                                    .render_json_with_budget(
                                        &event.type_,
                                        &event.contents,
                                        &TypesObjectSet::default(),
                                        json_budget,
                                    )
                                    .map(Box::new);
                            }
                        }

                        transaction
                    })
                    .collect();
            }
        }
    }

    Ok(checkpoint)
}
