// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::convert::Infallible;
use std::str::FromStr;
use std::sync::Arc;

use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream::BoxStream;
use sui_rpc::field::{FieldMask, FieldMaskTree, FieldMaskUtil};
use sui_rpc::proto::sui::rpc::v2::{
    BatchGetTransactionsRequest, BatchGetTransactionsResponse, ExecutedTransaction,
    GetTransactionRequest, GetTransactionResponse, GetTransactionResult,
};
use sui_rpc_api::{
    ErrorReason, RpcError, TransactionNotFoundError,
    proto::google::rpc::bad_request::FieldViolation,
};
use sui_types::base_types::TransactionDigest;

use crate::bigtable_client::BigTableClient;
use crate::config::{DEFAULT_RENDER_AHEAD, PipelineStage, StagesConfig};
use crate::pipeline::{RenderAheadError, ResolvedWatermarked, render_ahead};
use crate::render::transaction_to_response;
use crate::resolve::ResolvedTransaction;
use crate::served_height::ServedHeight;
use crate::{PackageResolver, resolve};

pub const MAX_BATCH_REQUESTS: usize = 200;
pub const READ_MASK_DEFAULT: &str = sui_rpc_api::read_mask_defaults::TRANSACTION;

pub(crate) fn validate_read_mask(read_mask: Option<FieldMask>) -> Result<FieldMaskTree, RpcError> {
    let read_mask = read_mask.unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
    read_mask
        .validate::<ExecutedTransaction>()
        .map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
    Ok(FieldMaskTree::from(read_mask))
}

/// Serve a transaction whose checkpoint is at or below `served_height`. One above it is `NotFound`,
/// as if not yet written: its `transactions` row can already exist while the `objects` rows a read
/// mask with objects joins are still being written.
pub async fn get_transaction(
    client: BigTableClient,
    stages: &StagesConfig,
    served_height: ServedHeight,
    request: GetTransactionRequest,
    resolver: &PackageResolver,
) -> Result<GetTransactionResponse, RpcError> {
    let transaction_digest = request
        .digest
        .ok_or_else(|| {
            FieldViolation::new("digest")
                .with_description("missing digest")
                .with_reason(ErrorReason::FieldMissing)
        })?
        .parse::<TransactionDigest>()
        .map_err(|e| {
            FieldViolation::new("digest")
                .with_description(format!("invalid digest: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;

    let read_mask = validate_read_mask(request.read_mask)?;

    let transactions_stage = stages.stage(PipelineStage::Transactions);
    let objects_stage = stages.stage(PipelineStage::Objects);
    // Read to the end rather than stopping at the first item, so no read is dropped before
    // BigTable ends its response, which would cancel it and reset its HTTP/2 stream.
    let (_, transaction, objects) = resolve::resolve_transactions(
        client,
        vec![transaction_digest],
        served_height.checkpoint,
        &read_mask,
        transactions_stage,
        objects_stage,
    )
    .try_collect::<Vec<_>>()
    .await?
    .pop()
    .ok_or(TransactionNotFoundError(transaction_digest.into()))?;
    Ok(GetTransactionResponse::new(
        transaction_to_response(transaction, &read_mask, objects.as_ref(), resolver).await?,
    ))
}

/// [`get_transaction`] for each digest, answering per digest, in request order. Requests for the
/// same digest share one lookup: each distinct digest is read once and rendered once, and every
/// request for it gets the result. Lookups are rendered as their reads complete, at most
/// `DEFAULT_RENDER_AHEAD` at a time (the list endpoints' default render-ahead), while later reads
/// run. A digest without a row, or above the served height, is `NOT_FOUND`, and a render error
/// answers that digest's requests; a failed read fails the whole batch.
pub async fn batch_get_transactions(
    client: BigTableClient,
    stages: &StagesConfig,
    served_height: ServedHeight,
    BatchGetTransactionsRequest {
        digests, read_mask, ..
    }: BatchGetTransactionsRequest,
    resolver: &PackageResolver,
) -> Result<BatchGetTransactionsResponse, RpcError> {
    let read_mask = validate_read_mask(read_mask)?;

    if digests.len() > MAX_BATCH_REQUESTS {
        return Err(RpcError::new(
            tonic::Code::InvalidArgument,
            format!("number of batch requests exceed limit of {MAX_BATCH_REQUESTS}"),
        ));
    }

    let digests = digests
        .iter()
        .map(|digest| TransactionDigest::from_str(digest))
        .collect::<Result<Vec<_>, _>>()?;

    // Requests for the same digest share one lookup: `lookups` holds each distinct digest once,
    // in order of its first request, and `requests_left` counts the requests for each.
    let mut requests_left: HashMap<TransactionDigest, usize> = HashMap::new();
    let mut lookups = Vec::new();
    for &digest in &digests {
        *requests_left.entry(digest).or_insert_with(|| {
            lookups.push(digest);
            0
        }) += 1;
    }

    let transactions_stage = stages.stage(PipelineStage::Transactions);
    let objects_stage = stages.stage(PipelineStage::Objects);
    let resolved: BoxStream<'static, Result<ResolvedWatermarked<ResolvedTransaction>, RpcError>> =
        resolve::resolve_transactions(
            client,
            lookups,
            served_height.checkpoint,
            &read_mask,
            transactions_stage,
            objects_stage,
        )
        .map_ok(ResolvedWatermarked::Item)
        .boxed();

    let mut rendered = render_ahead(resolved, DEFAULT_RENDER_AHEAD, {
        let read_mask = Arc::new(read_mask);
        let resolver = resolver.clone();
        move |(digest, transaction, objects): ResolvedTransaction| {
            let read_mask = read_mask.clone();
            let resolver = resolver.clone();
            async move {
                let result =
                    match transaction_to_response(transaction, &read_mask, &objects, &resolver)
                        .await
                    {
                        Ok(transaction) => GetTransactionResult::new_transaction(transaction),
                        Err(err) => GetTransactionResult::new_error(err.into_status_proto()),
                    };
                Ok::<_, Infallible>((digest, result))
            }
        }
    });

    let mut results: HashMap<TransactionDigest, GetTransactionResult> = HashMap::new();
    while let Some(frame) = rendered.next().await {
        match frame {
            // Rows arrive in no particular order, so results are kept by digest.
            Ok(ResolvedWatermarked::Item((digest, result))) => {
                if requests_left.contains_key(&digest) {
                    results.insert(digest, result);
                }
            }
            Ok(ResolvedWatermarked::Watermark { .. }) => {}
            Err(RenderAheadError::Upstream(err)) => return Err(err),
            Err(RenderAheadError::Render(never)) => match never {},
        }
    }

    // A digest's result goes to every request for it; the last of those requests takes it. A
    // digest without a result had no row, or its checkpoint is above the served height.
    let transactions = digests
        .into_iter()
        .map(|digest| {
            let last_request = requests_left.get_mut(&digest).is_some_and(|left| {
                *left -= 1;
                *left == 0
            });
            let result = if last_request {
                results.remove(&digest)
            } else {
                results.get(&digest).cloned()
            };
            result.unwrap_or_else(|| {
                let err: RpcError = TransactionNotFoundError(digest.into()).into();
                GetTransactionResult::new_error(err.into_status_proto())
            })
        })
        .collect();
    Ok(BatchGetTransactionsResponse::new(transactions))
}

#[cfg(test)]
mod tests {
    use mysten_common::ZipDebugEqIteratorExt;
    use std::collections::BTreeSet;

    use sui_kvstore::testing::{ReadRowsResponseOrder, insert_checkpoint_rows};
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;
    use crate::StageConfig;
    use crate::v2::test_utils::{
        UNBOUNDED_SERVED_HEIGHT, assert_identity_only_object_mask,
        canonical_transaction_object_keys, query_context_with_mock_and_registry,
        response_object_keys, two_transaction_object_checkpoint,
    };

    fn request(
        digest: TransactionDigest,
        paths: impl IntoIterator<Item = &'static str>,
    ) -> GetTransactionRequest {
        let mut request = GetTransactionRequest::default();
        request.digest = Some(digest.to_string());
        request.read_mask = Some(FieldMask::from_paths(paths));
        request
    }

    /// Assert `response` answers the digests `[second, missing, first, second, missing]` of
    /// `checkpoint`, in that order, with each repeated digest answered as at its first request.
    fn assert_batch_objects(
        response: BatchGetTransactionsResponse,
        checkpoint: &sui_types::full_checkpoint_content::Checkpoint,
        identity_only: bool,
    ) {
        assert_eq!(response.transactions.len(), 5);
        assert_eq!(response.transactions[3], response.transactions[0]);
        assert_eq!(response.transactions[4], response.transactions[1]);
        for (result, expected_index) in
            response
                .transactions
                .into_iter()
                .zip_debug_eq([Some(1), None, Some(0), Some(1), None])
        {
            let Some(index) = expected_index else {
                let status = result
                    .to_result()
                    .expect_err("missing digest should return an error");
                assert_eq!(status.code, tonic::Code::NotFound as i32);
                continue;
            };
            let transaction = result
                .to_result()
                .expect("known digest should return a transaction");
            assert_eq!(
                response_object_keys(&transaction),
                canonical_transaction_object_keys(checkpoint, index)
            );
            let sibling_created_id =
                TestCheckpointBuilder::derive_object_id(if index == 0 { 11 } else { 10 });
            assert!(
                response_object_keys(&transaction)
                    .iter()
                    .all(|key| key.0 != sibling_created_id),
                "transaction object set must exclude its sibling's created object"
            );
            if identity_only {
                assert_identity_only_object_mask(&transaction);
            }
        }
    }

    #[tokio::test]
    async fn get_transaction_objects_honor_parent_and_nested_masks() {
        let (ctx, _registry, mock, server) =
            query_context_with_mock_and_registry("get_transaction", 1).await;
        let checkpoint = two_transaction_object_checkpoint();
        insert_checkpoint_rows(&mock, &checkpoint).await;
        let stages = StagesConfig::default();

        mock.clear_read_rows_calls().await;
        let digest = checkpoint.transactions[0].transaction.digest();
        let transaction = get_transaction(
            ctx.client().clone(),
            &stages,
            UNBOUNDED_SERVED_HEIGHT,
            request(digest, ["digest"]),
            ctx.package_resolver(),
        )
        .await
        .expect("digest-only GetTransaction should succeed")
        .transaction
        .expect("GetTransaction should return a transaction");
        assert!(transaction.objects.is_none());
        assert!(
            mock.read_rows_calls()
                .await
                .iter()
                .all(|call| call.table != sui_kvstore::tables::objects::NAME),
            "digest-only GetTransaction must not read the object table"
        );

        for (paths, identity_only) in [
            (["objects"].as_slice(), false),
            (
                ["objects.objects.object_id", "objects.objects.version"].as_slice(),
                true,
            ),
        ] {
            for index in 0..checkpoint.transactions.len() {
                let digest = checkpoint.transactions[index].transaction.digest();
                let transaction = get_transaction(
                    ctx.client().clone(),
                    &stages,
                    UNBOUNDED_SERVED_HEIGHT,
                    request(digest, paths.iter().copied()),
                    ctx.package_resolver(),
                )
                .await
                .expect("GetTransaction should succeed")
                .transaction
                .expect("GetTransaction should return a transaction");
                let sibling_created_id =
                    TestCheckpointBuilder::derive_object_id(if index == 0 { 11 } else { 10 });

                assert_eq!(
                    response_object_keys(&transaction),
                    canonical_transaction_object_keys(&checkpoint, index)
                );
                assert!(
                    response_object_keys(&transaction)
                        .iter()
                        .all(|key| key.0 != sibling_created_id),
                    "transaction object set must exclude its sibling's created object"
                );
                if identity_only {
                    assert_identity_only_object_mask(&transaction);
                }
            }
        }

        server.abort();
    }

    #[tokio::test]
    async fn transaction_above_served_height_is_not_found() {
        let (ctx, _registry, mock, server) =
            query_context_with_mock_and_registry("get_transaction", 2).await;
        let checkpoint = TestCheckpointBuilder::new(1)
            .start_transaction(0)
            .create_owned_object(10)
            .finish_transaction()
            .build_checkpoint();
        insert_checkpoint_rows(&mock, &checkpoint).await;
        let stages = StagesConfig::default();
        let digest = checkpoint.transactions[0].transaction.digest();

        for (served_checkpoint, served) in [(0, false), (1, true)] {
            let served_height = ServedHeight {
                checkpoint: served_checkpoint,
                epoch: 0,
            };
            let single = get_transaction(
                ctx.client().clone(),
                &stages,
                served_height,
                request(digest, ["digest", "objects"]),
                ctx.package_resolver(),
            )
            .await
            .map_err(tonic::Status::from);

            let mut batch_request = BatchGetTransactionsRequest::default();
            batch_request.digests = vec![digest.to_string()];
            batch_request.read_mask = Some(FieldMask::from_paths(["digest", "objects"]));
            let batch = batch_get_transactions(
                ctx.client().clone(),
                &stages,
                served_height,
                batch_request,
                ctx.package_resolver(),
            )
            .await
            .expect("BatchGetTransactions succeeds")
            .transactions
            .pop()
            .expect("one result per digest")
            .to_result();

            if served {
                let single = single.expect("served transaction is found");
                assert_eq!(
                    single.transaction.and_then(|tx| tx.digest),
                    Some(digest.to_string())
                );
                let batch = batch.expect("served transaction is found");
                assert_eq!(batch.digest, Some(digest.to_string()));
            } else {
                let status = single.expect_err("transaction above the served height");
                assert_eq!(status.code(), tonic::Code::NotFound);
                let status = batch.expect_err("transaction above the served height");
                assert_eq!(status.code, tonic::Code::NotFound as i32);
            }
        }

        server.abort();
    }

    /// At the chain tip a transaction's row can be written before the object rows it references.
    /// Above the served height that transaction is `NotFound`, without reading its objects, and a
    /// batch still serves its other transactions.
    #[tokio::test]
    async fn transaction_above_served_height_skips_unwritten_objects() {
        use sui_kvstore::tables::transactions;
        use sui_types::balance_change::derive_balance_changes_2;

        let (ctx, _registry, mock, server) =
            query_context_with_mock_and_registry("get_transaction", 2).await;
        let served = TestCheckpointBuilder::new(1)
            .start_transaction(0)
            .create_owned_object(10)
            .finish_transaction()
            .build_checkpoint();
        insert_checkpoint_rows(&mock, &served).await;
        let tip = TestCheckpointBuilder::new(2)
            .start_transaction(1)
            .create_owned_object(11)
            .finish_transaction()
            .build_checkpoint();
        let tip_transaction = &tip.transactions[0];
        let row = transactions::encode(
            &tip_transaction.transaction,
            &tip_transaction.signatures,
            &tip_transaction.effects,
            &tip_transaction.events,
            tip.summary.sequence_number,
            tip.summary.timestamp_ms,
            &derive_balance_changes_2(&tip_transaction.effects, &tip.object_set),
            &tip_transaction.unchanged_loaded_runtime_objects,
        )
        .expect("encode transaction");
        let tip_digest = tip_transaction.transaction.digest();
        mock.insert_row(
            transactions::NAME,
            transactions::encode_key(&tip_digest),
            row,
        )
        .await;

        let stages = StagesConfig::default();
        let served_height = ServedHeight {
            checkpoint: served.summary.sequence_number,
            epoch: 0,
        };
        let status: tonic::Status = get_transaction(
            ctx.client().clone(),
            &stages,
            served_height,
            request(tip_digest, ["digest", "objects"]),
            ctx.package_resolver(),
        )
        .await
        .expect_err("transaction above the served height")
        .into();
        assert_eq!(status.code(), tonic::Code::NotFound);

        let served_digest = served.transactions[0].transaction.digest();
        let mut batch_request = BatchGetTransactionsRequest::default();
        batch_request.digests = vec![served_digest.to_string(), tip_digest.to_string()];
        batch_request.read_mask = Some(FieldMask::from_paths(["digest", "objects"]));
        let results = batch_get_transactions(
            ctx.client().clone(),
            &stages,
            served_height,
            batch_request,
            ctx.package_resolver(),
        )
        .await
        .expect("BatchGetTransactions succeeds")
        .transactions;
        let [served_result, tip_result] = <[_; 2]>::try_from(results).expect("one per digest");
        let served_transaction = served_result.to_result().expect("served transaction");
        assert_eq!(served_transaction.digest, Some(served_digest.to_string()));
        let status = tip_result
            .to_result()
            .expect_err("transaction above the served height");
        assert_eq!(status.code, tonic::Code::NotFound as i32);

        server.abort();
    }

    /// Repeated and missing digests come back in request order, a repeated digest answered as at
    /// its first request, while each distinct digest and object is read once. Rows arrive in the
    /// reverse of request order, so results placed by arrival would land out of order. A request
    /// limited to one BigTable read at once, with every read one key, answers the same way
    /// rather than hanging.
    #[tokio::test]
    async fn batch_get_transaction_objects_preserve_order_and_deduplicate_reads() {
        let (ctx, _registry, mock, server) =
            query_context_with_mock_and_registry("batch_get_transactions", 1).await;
        let checkpoint = two_transaction_object_checkpoint();
        insert_checkpoint_rows(&mock, &checkpoint).await;
        mock.set_read_rows_response_order(ReadRowsResponseOrder::ReverseRequestOrder)
            .await;
        let stages = StagesConfig::default();
        let first_digest = checkpoint.transactions[0].transaction.digest();
        let second_digest = checkpoint.transactions[1].transaction.digest();
        let missing_digest = TransactionDigest::random();
        let digests = vec![
            second_digest.to_string(),
            missing_digest.to_string(),
            first_digest.to_string(),
            second_digest.to_string(),
            missing_digest.to_string(),
        ];
        let mut expected_transaction_rows = [first_digest, second_digest, missing_digest]
            .iter()
            .map(sui_kvstore::tables::transactions::encode_key)
            .collect::<Vec<_>>();
        expected_transaction_rows.sort();

        mock.clear_read_rows_calls().await;
        let mut parent_request = BatchGetTransactionsRequest::default();
        parent_request.digests = digests.clone();
        parent_request.read_mask = Some(FieldMask::from_paths(["objects"]));
        let parent_response = batch_get_transactions(
            ctx.client().clone(),
            &stages,
            UNBOUNDED_SERVED_HEIGHT,
            parent_request,
            ctx.package_resolver(),
        )
        .await
        .expect("parent objects BatchGetTransactions should succeed");
        assert_batch_objects(parent_response, &checkpoint, false);
        let mut actual_transaction_rows = mock
            .read_rows_calls()
            .await
            .into_iter()
            .filter(|call| call.table == sui_kvstore::tables::transactions::NAME)
            .flat_map(|call| call.row_keys)
            .map(|key| key.to_vec())
            .collect::<Vec<_>>();
        actual_transaction_rows.sort();
        assert_eq!(
            actual_transaction_rows, expected_transaction_rows,
            "each distinct digest is read once"
        );

        mock.clear_read_rows_calls().await;
        let mut nested_request = BatchGetTransactionsRequest::default();
        nested_request.digests = digests.clone();
        nested_request.read_mask = Some(FieldMask::from_paths([
            "objects.objects.object_id",
            "objects.objects.version",
        ]));
        let nested_response = batch_get_transactions(
            ctx.client().clone(),
            &stages,
            UNBOUNDED_SERVED_HEIGHT,
            nested_request,
            ctx.package_resolver(),
        )
        .await
        .expect("nested objects BatchGetTransactions should succeed");
        assert_batch_objects(nested_response, &checkpoint, true);

        let mut actual_object_rows = mock
            .read_rows_calls()
            .await
            .into_iter()
            .filter(|call| call.table == sui_kvstore::tables::objects::NAME)
            .flat_map(|call| call.row_keys)
            .map(|key| key.to_vec())
            .collect::<Vec<_>>();
        actual_object_rows.sort();
        let mut expected_object_rows = (0..checkpoint.transactions.len())
            .flat_map(|index| canonical_transaction_object_keys(&checkpoint, index))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|key| sui_kvstore::tables::objects::encode_key(&key))
            .collect::<Vec<_>>();
        expected_object_rows.sort();
        assert_eq!(actual_object_rows, expected_object_rows);

        let (addr, one_permit_server) = mock.start().await.expect("start mock BigTable");
        let one_permit = BigTableClient::new(
            sui_kvstore::BigTableClient::new_local(addr.to_string(), "test".to_string())
                .await
                .expect("connect to mock BigTable"),
            1,
            crate::bigtable_client::Metrics::for_testing().0,
            "batch_get_transactions",
        );
        let one_key_read = StageConfig {
            max_seeks_per_read: Some(1),
            concurrency: Some(1),
            ..Default::default()
        };
        let one_key_stages = StagesConfig {
            transactions: Some(one_key_read.clone()),
            objects: Some(one_key_read),
            ..Default::default()
        };
        let mut one_permit_request = BatchGetTransactionsRequest::default();
        one_permit_request.digests = digests;
        one_permit_request.read_mask = Some(FieldMask::from_paths(["objects"]));
        let one_permit_response = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            batch_get_transactions(
                one_permit,
                &one_key_stages,
                UNBOUNDED_SERVED_HEIGHT,
                one_permit_request,
                ctx.package_resolver(),
            ),
        )
        .await
        .expect("BatchGetTransactions does not hang at a request concurrency of 1")
        .expect("one-permit BatchGetTransactions should succeed");
        assert_batch_objects(one_permit_response, &checkpoint, false);

        one_permit_server.abort();
        server.abort();
    }
}
