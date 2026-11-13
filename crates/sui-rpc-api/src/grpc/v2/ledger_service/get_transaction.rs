// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::object_set::fetch_transaction_object_set;
use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;
use crate::TransactionNotFoundError;
use crate::grpc::v2::render::RenderContext;
use crate::ledger_history::query_options::below_serving_floor;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::merge::Merge;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::BatchGetTransactionsRequest;
use sui_rpc::proto::sui::rpc::v2::BatchGetTransactionsResponse;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2::GetTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::GetTransactionResponse;
use sui_rpc::proto::sui::rpc::v2::GetTransactionResult;
use sui_rpc::proto::sui::rpc::v2::ObjectSet as ProtoObjectSet;
use sui_rpc::proto::sui::rpc::v2::Transaction;
use sui_rpc::proto::sui::rpc::v2::UserSignature;
use sui_rpc::proto::timestamp_ms_to_proto;
use sui_sdk_types::Digest;
use sui_types::balance_change::derive_balance_changes_2;
use sui_types::full_checkpoint_content::ObjectSet;

pub const MAX_BATCH_REQUESTS: usize = 200;
pub const READ_MASK_DEFAULT: &str = crate::read_mask_defaults::TRANSACTION;

#[tracing::instrument(skip(service))]
pub fn get_transaction(
    service: &RpcService,
    request: GetTransactionRequest,
) -> Result<GetTransactionResponse, RpcError> {
    let transaction_digest = request
        .digest
        .ok_or_else(|| {
            FieldViolation::new("digest")
                .with_description("missing digest")
                .with_reason(ErrorReason::FieldMissing)
        })?
        .parse::<Digest>()
        .map_err(|e| {
            FieldViolation::new("digest")
                .with_description(format!("invalid digest: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;

    let read_mask = {
        let read_mask = request
            .read_mask
            .unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
        read_mask
            .validate::<ExecutedTransaction>()
            .map_err(|path| {
                FieldViolation::new("read_mask")
                    .with_description(format!("invalid read_mask path: {path}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })?;
        FieldMaskTree::from(read_mask)
    };

    let latest_checkpoint = service
        .reader
        .inner()
        .get_latest_checkpoint()?
        .sequence_number;
    let render_context = RenderContext::empty(service.reader.inner());
    let floor_before_reads = service.reader.get_lowest_available_checkpoint()?;
    let (transaction_checkpoint, transaction) = read_transaction(
        service,
        transaction_digest,
        &read_mask,
        &render_context,
        latest_checkpoint,
        floor_before_reads,
    );

    let lowest_available_checkpoint = service.reader.get_lowest_available_checkpoint()?;
    below_floor_or(
        transaction_checkpoint,
        lowest_available_checkpoint,
        transaction,
    )
    .map(GetTransactionResponse::new)
}

#[tracing::instrument(skip(service))]
pub fn batch_get_transactions(
    service: &RpcService,
    BatchGetTransactionsRequest {
        digests, read_mask, ..
    }: BatchGetTransactionsRequest,
) -> Result<BatchGetTransactionsResponse, RpcError> {
    let read_mask = {
        let read_mask = read_mask.unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
        read_mask
            .validate::<ExecutedTransaction>()
            .map_err(|path| {
                FieldViolation::new("read_mask")
                    .with_description(format!("invalid read_mask path: {path}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })?;
        FieldMaskTree::from(read_mask)
    };

    if digests.len() > MAX_BATCH_REQUESTS {
        return Err(RpcError::new(
            tonic::Code::InvalidArgument,
            format!("number of batch requests exceed limit of {MAX_BATCH_REQUESTS}"),
        ));
    }

    let latest_checkpoint = service
        .reader
        .inner()
        .get_latest_checkpoint()?
        .sequence_number;
    let render_context = RenderContext::empty(service.reader.inner());
    let floor_before_reads = service.reader.get_lowest_available_checkpoint()?;
    let reads = digests
        .into_iter()
        .enumerate()
        .map(|(idx, digest)| {
            let digest: Digest = match digest.parse() {
                Ok(digest) => digest,
                Err(e) => {
                    let violation = FieldViolation::new_at("digests", idx)
                        .with_description(format!("invalid digest: {e}"))
                        .with_reason(ErrorReason::FieldInvalid);
                    return (None, Err(violation.into()));
                }
            };
            read_transaction(
                service,
                digest,
                &read_mask,
                &render_context,
                latest_checkpoint,
                floor_before_reads,
            )
        })
        .collect::<Vec<_>>();

    // One floor, read after every read in the batch.
    let lowest_available_checkpoint = service.reader.get_lowest_available_checkpoint()?;
    let transactions = reads
        .into_iter()
        .map(|(transaction_checkpoint, transaction)| {
            match below_floor_or(
                transaction_checkpoint,
                lowest_available_checkpoint,
                transaction,
            ) {
                Ok(transaction) => GetTransactionResult::new_transaction(transaction),
                Err(error) => GetTransactionResult::new_error(error.into_status_proto()),
            }
        })
        .collect();

    Ok(BatchGetTransactionsResponse::new(transactions))
}

/// Reads and renders transaction `digest`. Returns the checkpoint the transaction is in, when
/// known, with the read's result. A transaction whose checkpoint is not known, or is above
/// `latest_checkpoint`, is `NotFound`. One below `floor_before_reads` is refused as pruned without
/// reading it, since the floor never falls; that is only a shortcut, and the caller checks the
/// floor again after the reads (see [`below_floor_or`]).
fn read_transaction(
    service: &RpcService,
    digest: Digest,
    read_mask: &FieldMaskTree,
    render_context: &RenderContext,
    latest_checkpoint: u64,
    floor_before_reads: u64,
) -> (Option<u64>, Result<ExecutedTransaction, RpcError>) {
    let Some(checkpoint) = service
        .reader
        .inner()
        .get_transaction_checkpoint(&digest.into())
    else {
        return (None, Err(TransactionNotFoundError(digest).into()));
    };
    if checkpoint > latest_checkpoint {
        return (None, Err(TransactionNotFoundError(digest).into()));
    }
    if checkpoint < floor_before_reads {
        return (
            Some(checkpoint),
            Err(below_serving_floor(floor_before_reads)),
        );
    }

    let transaction = service
        .reader
        .get_transaction_read(digest)
        .and_then(|transaction_read| {
            let objects =
                fetch_transaction_object_set(&service.reader, &transaction_read, read_mask)?;
            render_executed_transaction(
                service,
                transaction_read,
                &objects,
                checkpoint,
                read_mask,
                render_context,
            )
        });
    (Some(checkpoint), transaction)
}

/// `transaction`, the result of reading a transaction in `checkpoint`, unless `checkpoint` is
/// below `lowest_available_checkpoint`: then `OutOfRange` (`BELOW_EARLIEST_AVAILABLE`) whatever the
/// read returned, also while the transaction is still stored. Read the floor after the reads:
/// pruning can delete the transaction or the object versions it touched while they run, making
/// them fail with `NotFound` or `Internal`.
fn below_floor_or<T>(
    checkpoint: Option<u64>,
    lowest_available_checkpoint: u64,
    transaction: Result<T, RpcError>,
) -> Result<T, RpcError> {
    match checkpoint {
        Some(checkpoint) if checkpoint < lowest_available_checkpoint => {
            Err(below_serving_floor(lowest_available_checkpoint))
        }
        _ => transaction,
    }
}

/// Renders historical object and event JSON against the same backing-only package context,
/// independently of which objects the read mask requests.
pub(crate) fn render_executed_transaction(
    service: &RpcService,
    crate::reader::TransactionRead {
        digest,
        transaction,
        signatures,
        effects,
        events,
        timestamp_ms,
        unchanged_loaded_runtime_objects,
    }: crate::reader::TransactionRead,
    objects: &ObjectSet,
    checkpoint: u64,
    mask: &FieldMaskTree,
    render_context: &RenderContext,
) -> Result<ExecutedTransaction, RpcError> {
    let mut message = ExecutedTransaction::default();

    if mask.contains(ExecutedTransaction::DIGEST_FIELD) {
        message.digest = Some(digest.to_string());
    }

    if let Some(submask) = mask.subtree(ExecutedTransaction::TRANSACTION_FIELD) {
        message.transaction = Some(Transaction::merge_from(&transaction, &submask));
    }

    if let Some(submask) = mask.subtree(ExecutedTransaction::SIGNATURES_FIELD) {
        message.signatures = signatures
            .into_iter()
            .map(|s| UserSignature::merge_from(&s, &submask))
            .collect();
    }

    let unchanged_loaded_runtime_objects = unchanged_loaded_runtime_objects.unwrap_or_default();

    if let Some(submask) = mask.subtree(ExecutedTransaction::EFFECTS_FIELD) {
        let effects = service.render_effects_to_proto(
            &effects,
            &unchanged_loaded_runtime_objects,
            objects,
            &submask,
        );

        message.effects = Some(effects);
    }

    if let Some(submask) = mask.subtree(ExecutedTransaction::EVENTS_FIELD) {
        message.events =
            events.map(|events| service.render_events_to_proto(&events, &submask, render_context));
    }

    if mask.contains(ExecutedTransaction::CHECKPOINT_FIELD) {
        message.set_checkpoint(checkpoint);
    }

    if mask.contains(ExecutedTransaction::TIMESTAMP_FIELD) {
        message.timestamp = timestamp_ms.map(timestamp_ms_to_proto);
    }

    if mask.contains(ExecutedTransaction::BALANCE_CHANGES_FIELD) {
        message.balance_changes = derive_balance_changes_2(&effects, objects)
            .into_iter()
            .map(Into::into)
            .collect();
    }

    if let Some(object_mask) = mask
        .subtree(ExecutedTransaction::OBJECTS_FIELD)
        .and_then(|submask| submask.subtree(ProtoObjectSet::OBJECTS_FIELD))
    {
        message.objects = Some(
            ProtoObjectSet::default().with_objects(
                objects
                    .iter()
                    .map(|object| {
                        service.render_object_to_proto(object, &object_mask, render_context)
                    })
                    .collect(),
            ),
        );
    }

    Ok(message)
}

#[cfg(test)]
mod tests {
    use sui_rpc::proto::sui::rpc::v2::get_transaction_result;
    use sui_types::effects::TransactionEffectsAPI;
    use sui_types::full_checkpoint_content::Checkpoint;

    use super::*;
    use crate::ledger_history::query_options::assert_below_serving_floor_error;
    use crate::reader::{NotServedHereCause, assert_not_served_here};
    use crate::stub_store::StubStore;

    /// The digest of the one transaction in each of `checkpoints`.
    fn digests(checkpoints: &[Checkpoint]) -> Vec<String> {
        checkpoints
            .iter()
            .map(|checkpoint| {
                let [transaction] = checkpoint.transactions.as_slice() else {
                    panic!("expected one transaction per checkpoint");
                };
                transaction.effects.transaction_digest().to_string()
            })
            .collect()
    }

    fn request(digest: &str, read_mask: &str) -> GetTransactionRequest {
        let mut request = GetTransactionRequest::default();
        request.digest = Some(digest.to_owned());
        request.read_mask = Some(FieldMask::from_str(read_mask));
        request
    }

    fn code(error: RpcError) -> i32 {
        error.into_status_proto().code
    }

    /// A transaction in a checkpoint below the floor is refused as pruned whatever the read
    /// returned: while it is still stored, and when pruning has deleted object versions it needs.
    /// At the floor it is served, or fails with the read's own error.
    #[test]
    fn transaction_below_the_floor_is_pruned_whatever_the_read_returned() {
        let checkpoints = StubStore::two_checkpoints();
        let [first, second] = digests(&checkpoints).try_into().unwrap();

        let stored = StubStore {
            lowest_available_checkpoint: 1,
            ..StubStore::with_checkpoints(checkpoints.clone())
        }
        .into_service();
        assert_below_serving_floor_error(
            get_transaction(&stored, request(&first, "digest")).unwrap_err(),
            1,
        );
        let served = get_transaction(&stored, request(&second, "digest")).unwrap();
        assert_eq!(served.transaction.unwrap().digest, Some(second.clone()));

        let objects_pruned = |lowest_available_checkpoint| {
            StubStore {
                objects: Vec::new(),
                lowest_available_checkpoint,
                ..StubStore::with_checkpoints(checkpoints.clone())
            }
            .into_service()
        };
        let error = get_transaction(&objects_pruned(0), request(&first, "objects")).unwrap_err();
        assert_eq!(code(error), tonic::Code::Internal as i32);
        assert_below_serving_floor_error(
            get_transaction(&objects_pruned(1), request(&first, "objects")).unwrap_err(),
            1,
        );
    }

    /// Pruning that passes a transaction's checkpoint while the transaction is read makes the
    /// read fail; the floor, read after the reads, reports the transaction as pruned. In a batch
    /// that applies per item, against the floor read after the whole batch.
    #[test]
    fn transaction_pruned_during_the_read_is_below_the_floor() {
        let checkpoints = StubStore::two_checkpoints();
        let [first, second] = digests(&checkpoints).try_into().unwrap();
        let pruned_during_reads = || {
            StubStore {
                objects: Vec::new(),
                pruned_to_during_reads: Some(1),
                ..StubStore::with_checkpoints(checkpoints.clone())
            }
            .into_service()
        };

        assert_below_serving_floor_error(
            get_transaction(&pruned_during_reads(), request(&first, "objects")).unwrap_err(),
            1,
        );

        let mut request = BatchGetTransactionsRequest::default();
        request.digests = vec![first, second];
        request.read_mask = Some(FieldMask::from_str("objects"));
        let response = batch_get_transactions(&pruned_during_reads(), request).unwrap();
        let codes = response
            .transactions
            .iter()
            .map(|result| match &result.result {
                Some(get_transaction_result::Result::Error(status)) => status.code,
                _ => panic!("expected an error in {response:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            codes,
            [tonic::Code::OutOfRange as i32, tonic::Code::Internal as i32]
        );
    }

    /// A batch checks each transaction against one floor: below it is pruned, at it is served,
    /// and a digest with no known checkpoint is `NotFound`.
    #[test]
    fn batch_refuses_each_transaction_below_the_floor() {
        let checkpoints = StubStore::two_checkpoints();
        let [first, second] = digests(&checkpoints).try_into().unwrap();
        let unknown = Digest::new([1; 32]).to_string();
        let service = StubStore {
            lowest_available_checkpoint: 1,
            ..StubStore::with_checkpoints(checkpoints)
        }
        .into_service();

        let mut request = BatchGetTransactionsRequest::default();
        request.digests = vec![first, second.clone(), unknown];
        request.read_mask = Some(FieldMask::from_str("digest"));
        let response = batch_get_transactions(&service, request).unwrap();
        let codes = response
            .transactions
            .iter()
            .map(|result| match &result.result {
                Some(get_transaction_result::Result::Transaction(transaction)) => {
                    assert_eq!(transaction.digest, Some(second.clone()));
                    tonic::Code::Ok as i32
                }
                Some(get_transaction_result::Result::Error(status)) => status.code,
                _ => panic!("unexpected result in {response:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            codes,
            [
                tonic::Code::OutOfRange as i32,
                tonic::Code::Ok as i32,
                tonic::Code::NotFound as i32,
            ]
        );
    }

    /// On a node without object version history, a transaction read whose mask needs object
    /// versions the node deleted is refused as not served here; one that needs none is served.
    #[test]
    fn missing_object_versions_are_not_served_without_history() {
        let checkpoints = StubStore::two_checkpoints();
        let [_, second] = digests(&checkpoints).try_into().unwrap();
        let service = StubStore {
            objects: Vec::new(),
            retains_object_version_history: false,
            ..StubStore::with_checkpoints(checkpoints)
        }
        .into_service();

        assert_not_served_here(
            get_transaction(&service, request(&second, "objects")).unwrap_err(),
            NotServedHereCause::NoObjectHistory,
        );
        get_transaction(&service, request(&second, "digest")).unwrap();
    }
}
