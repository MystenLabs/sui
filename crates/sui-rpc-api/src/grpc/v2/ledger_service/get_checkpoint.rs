// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::object_set::mask_requests_object_set;
use super::object_set::missing_object_version_error;
use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;
use crate::error::CheckpointNotFoundError;
use crate::grpc::v2::render::RenderContext;
use crate::ledger_history::query_options::below_serving_floor;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::merge::Merge;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::Checkpoint;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointRequest;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointResponse;
use sui_rpc::proto::sui::rpc::v2::ObjectSet;
use sui_rpc::proto::sui::rpc::v2::get_checkpoint_request::CheckpointId;
use sui_sdk_types::Digest;
use sui_types::balance_change::derive_balance_changes_2;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::storage::error::Kind as StorageErrorKind;

pub const READ_MASK_DEFAULT: &str = crate::read_mask_defaults::CHECKPOINT;

#[tracing::instrument(skip(service))]
pub fn get_checkpoint(
    service: &RpcService,
    request: GetCheckpointRequest,
) -> Result<GetCheckpointResponse, RpcError> {
    let read_mask = {
        let read_mask = request
            .read_mask
            .unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
        read_mask.validate::<Checkpoint>().map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
        FieldMaskTree::from(read_mask)
    };

    // The floor never falls, so a checkpoint below it before the reads is refused without
    // reading its contents. That check is only a shortcut: the one that counts reads the floor
    // again after the reads, because pruning can delete the checkpoint or the data it needs while
    // they run, making them fail with `NotFound` or `Internal`. A checkpoint below the floor is
    // reported as pruned whatever the reads returned, also while it is still stored.
    let floor_before_reads = service.reader.get_lowest_available_checkpoint()?;
    let (sequence_number, checkpoint) = read_checkpoint(
        service,
        request.checkpoint_id,
        &read_mask,
        floor_before_reads,
    );

    let lowest_available_checkpoint = service.reader.get_lowest_available_checkpoint()?;
    if let Some(sequence_number) = sequence_number
        && sequence_number < lowest_available_checkpoint
    {
        return Err(below_serving_floor(lowest_available_checkpoint));
    }
    checkpoint.map(GetCheckpointResponse::new)
}

/// Reads the checkpoint `checkpoint_id` names (the latest one if none). Returns the checkpoint's
/// sequence number when it is known, from the request or from the summary found by digest, with
/// the read's result. A checkpoint below `floor_before_reads` is refused as pruned without reading
/// its contents; the caller checks the floor again after the reads. A lookup by digest that finds
/// nothing cannot tell a pruned checkpoint from one that never existed, and answers `NotFound`
/// with no sequence number.
fn read_checkpoint(
    service: &RpcService,
    checkpoint_id: Option<CheckpointId>,
    read_mask: &FieldMaskTree,
    floor_before_reads: u64,
) -> (Option<u64>, Result<Checkpoint, RpcError>) {
    let store = service.reader.inner();
    let verified_summary = match checkpoint_id {
        Some(CheckpointId::SequenceNumber(s)) => match store.get_checkpoint_by_sequence_number(s) {
            Some(summary) => summary,
            None => {
                return (
                    Some(s),
                    Err(CheckpointNotFoundError::sequence_number(s).into()),
                );
            }
        },
        Some(CheckpointId::Digest(digest)) => {
            let digest = match digest.parse::<Digest>() {
                Ok(digest) => digest,
                Err(e) => {
                    let violation = FieldViolation::new("digest")
                        .with_description(format!("invalid digest: {e}"))
                        .with_reason(ErrorReason::FieldInvalid);
                    return (None, Err(violation.into()));
                }
            };
            match store.get_checkpoint_by_digest(&digest.into()) {
                Some(summary) => summary,
                None => return (None, Err(CheckpointNotFoundError::digest(digest).into())),
            }
        }
        _ => match store.get_latest_checkpoint() {
            Ok(summary) => summary,
            Err(error) => return (None, Err(error.into())),
        },
    };
    let sequence_number = verified_summary.data().sequence_number;
    if sequence_number < floor_before_reads {
        return (
            Some(sequence_number),
            Err(below_serving_floor(floor_before_reads)),
        );
    }
    (
        Some(sequence_number),
        render_checkpoint(service, verified_summary, read_mask),
    )
}

fn render_checkpoint(
    service: &RpcService,
    verified_summary: VerifiedCheckpoint,
    read_mask: &FieldMaskTree,
) -> Result<Checkpoint, RpcError> {
    let summary = verified_summary.data();
    let signature = verified_summary.auth_sig();
    let sequence_number = summary.sequence_number;
    let timestamp_ms = summary.timestamp_ms;

    let latest_checkpoint = service
        .reader
        .inner()
        .get_latest_checkpoint()?
        .sequence_number;
    if sequence_number > latest_checkpoint {
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
            // `get_checkpoint_data` loads every object version the checkpoint's transactions read
            // or wrote, whatever the mask. When the mask needs them, missing data on a node
            // without object version history is a replaced version that node deleted (below the
            // lowest available checkpoint, the caller reports the checkpoint as pruned instead).
            let mask_needs_object_versions = read_mask.contains(Checkpoint::OBJECTS_FIELD.name)
                || read_mask
                    .subtree(Checkpoint::TRANSACTIONS_FIELD.name)
                    .is_some_and(|submask| mask_requests_object_set(&submask));
            let checkpoint_data = service
                .reader
                .inner()
                .get_checkpoint_data(verified_summary, core_contents)
                .map_err(|error| match error.kind() {
                    StorageErrorKind::Missing if mask_needs_object_versions => {
                        missing_object_version_error(&service.reader, error.into())
                    }
                    _ => RpcError::from(error),
                })?;

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
                // every transaction in the checkpoint. Without this, an
                // unauthenticated `GetCheckpoint` with a permissive `read_mask`
                // multiplies one input checkpoint into thousands of per-event
                // renders, each with its own `max_json_move_value_size` budget.
                let mut json_budget = service.config.max_json_move_value_response_size();
                let render_context =
                    RenderContext::new(service.reader.inner(), Some(&checkpoint_data.object_set));
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
                        service.annotate_checkpoint_transaction(
                            &mut transaction,
                            &t,
                            &checkpoint_data.object_set,
                            &submask,
                            &mut json_budget,
                            &render_context,
                        );

                        transaction
                    })
                    .collect();
            }
        }
    }

    Ok(checkpoint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger_history::query_options::assert_below_serving_floor_error;
    use crate::reader::{NotServedHereCause, assert_not_served_here};
    use crate::stub_store::StubStore;

    fn request(checkpoint_id: CheckpointId) -> GetCheckpointRequest {
        let mut request = GetCheckpointRequest::default();
        request.checkpoint_id = Some(checkpoint_id);
        request
    }

    fn request_with_mask(sequence_number: u64, read_mask: &str) -> GetCheckpointRequest {
        let mut request = request(CheckpointId::SequenceNumber(sequence_number));
        request.read_mask = Some(FieldMask::from_str(read_mask));
        request
    }

    fn code(error: RpcError) -> i32 {
        error.into_status_proto().code
    }

    /// A stored checkpoint below the floor is refused as pruned, by sequence number and by
    /// digest alike; the one at the floor is served.
    #[test]
    fn stored_checkpoint_below_the_floor_is_pruned() {
        let checkpoints = StubStore::two_checkpoints();
        let digests = checkpoints
            .iter()
            .map(|checkpoint| checkpoint.summary.digest().to_string())
            .collect::<Vec<_>>();
        let service = StubStore {
            lowest_available_checkpoint: 1,
            ..StubStore::with_checkpoints(checkpoints)
        }
        .into_service();

        assert_below_serving_floor_error(
            get_checkpoint(&service, request(CheckpointId::SequenceNumber(0))).unwrap_err(),
            1,
        );
        assert_below_serving_floor_error(
            get_checkpoint(&service, request(CheckpointId::Digest(digests[0].clone())))
                .unwrap_err(),
            1,
        );
        let served = get_checkpoint(&service, request(CheckpointId::Digest(digests[1].clone())))
            .unwrap()
            .checkpoint
            .unwrap();
        assert_eq!(served.sequence_number, Some(1));
    }

    /// On a node without object version history whose object versions are gone, a checkpoint
    /// read whose mask needs them is refused as not served here, one whose mask needs none keeps
    /// the read's `NotFound`, and a checkpoint below the floor is refused as pruned whatever the
    /// mask.
    #[test]
    fn missing_object_versions_are_not_served_only_when_the_mask_needs_them() {
        let service = StubStore {
            objects: Vec::new(),
            lowest_available_checkpoint: 1,
            retains_object_version_history: false,
            ..StubStore::with_checkpoints(StubStore::two_checkpoints())
        }
        .into_service();

        assert_not_served_here(
            get_checkpoint(&service, request_with_mask(1, "objects")).unwrap_err(),
            NotServedHereCause::NoObjectHistory,
        );
        assert_not_served_here(
            get_checkpoint(&service, request_with_mask(1, "transactions.effects")).unwrap_err(),
            NotServedHereCause::NoObjectHistory,
        );
        let error =
            get_checkpoint(&service, request_with_mask(1, "transactions.digest")).unwrap_err();
        assert_eq!(code(error), tonic::Code::NotFound as i32);
        assert_below_serving_floor_error(
            get_checkpoint(&service, request_with_mask(0, "objects")).unwrap_err(),
            1,
        );
    }

    /// Pruning that passes a checkpoint while its data is read makes the read fail; the floor,
    /// read after the reads, reports the checkpoint as pruned.
    #[test]
    fn checkpoint_pruned_during_the_read_is_below_the_floor() {
        let service = StubStore {
            objects: Vec::new(),
            pruned_to_during_reads: Some(1),
            ..StubStore::with_checkpoints(StubStore::two_checkpoints())
        }
        .into_service();

        assert_below_serving_floor_error(
            get_checkpoint(&service, request_with_mask(0, "objects")).unwrap_err(),
            1,
        );
    }

    /// A sequence number below the lowest available checkpoint is refused as pruned; at the floor
    /// a checkpoint the node does not have is `NotFound`, and so is a digest the node cannot find,
    /// since it cannot tell a pruned digest from one that never existed.
    #[test]
    fn missing_checkpoint_below_the_floor_is_pruned() {
        let service = StubStore {
            lowest_available_checkpoint: 10,
            ..StubStore::new()
        }
        .into_service();

        let error = get_checkpoint(&service, request(CheckpointId::SequenceNumber(9))).unwrap_err();
        assert_below_serving_floor_error(error, 10);

        let error =
            get_checkpoint(&service, request(CheckpointId::SequenceNumber(10))).unwrap_err();
        assert_eq!(code(error), tonic::Code::NotFound as i32);

        let digest = Digest::new([1; 32]).to_string();
        let error = get_checkpoint(&service, request(CheckpointId::Digest(digest))).unwrap_err();
        assert_eq!(code(error), tonic::Code::NotFound as i32);
    }
}
