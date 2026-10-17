// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;
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
use crate::config::{PipelineStage, StagesConfig};
use crate::render::transaction_to_response;
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
    let mut resolved = resolve::resolve_transactions(
        client,
        vec![transaction_digest],
        served_height.checkpoint,
        &read_mask,
        transactions_stage,
        objects_stage,
    )
    .await?;
    let (transaction, objects) = resolved
        .remove(&transaction_digest)
        .ok_or(TransactionNotFoundError(transaction_digest.into()))?;
    Ok(GetTransactionResponse::new(
        transaction_to_response(transaction, &read_mask, objects.as_ref(), resolver).await?,
    ))
}

/// [`get_transaction`] for each digest, answering per digest.
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
    let transactions_stage = stages.stage(PipelineStage::Transactions);
    let objects_stage = stages.stage(PipelineStage::Objects);
    let resolved = resolve::resolve_transactions(
        client,
        digests.clone(),
        served_height.checkpoint,
        &read_mask,
        transactions_stage,
        objects_stage,
    )
    .await?;

    let mut transactions = Vec::with_capacity(digests.len());
    for digest in digests {
        if let Some((tx, objects)) = resolved.get(&digest) {
            match transaction_to_response(tx.clone(), &read_mask, objects.as_ref(), resolver).await
            {
                Ok(tx) => transactions.push(GetTransactionResult::new_transaction(tx)),
                Err(err) => {
                    transactions.push(GetTransactionResult::new_error(err.into_status_proto()))
                }
            }
        } else {
            let err: RpcError = TransactionNotFoundError(digest.into()).into();
            transactions.push(GetTransactionResult::new_error(err.into_status_proto()));
        }
    }
    Ok(BatchGetTransactionsResponse::new(transactions))
}

#[cfg(test)]
mod tests {
    use mysten_common::ZipDebugEqIteratorExt;
    use std::collections::BTreeSet;

    use sui_kvstore::testing::insert_checkpoint_rows;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;
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

    fn assert_batch_objects(
        response: BatchGetTransactionsResponse,
        checkpoint: &sui_types::full_checkpoint_content::Checkpoint,
        identity_only: bool,
    ) {
        assert_eq!(response.transactions.len(), 3);
        for (result, expected_index) in
            response
                .transactions
                .into_iter()
                .zip_debug_eq([Some(1), None, Some(0)])
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

    #[tokio::test]
    async fn batch_get_transaction_objects_preserve_order_and_deduplicate_reads() {
        let (ctx, _registry, mock, server) =
            query_context_with_mock_and_registry("batch_get_transactions", 1).await;
        let checkpoint = two_transaction_object_checkpoint();
        insert_checkpoint_rows(&mock, &checkpoint).await;
        let stages = StagesConfig::default();
        let first_digest = checkpoint.transactions[0].transaction.digest();
        let second_digest = checkpoint.transactions[1].transaction.digest();
        let missing_digest = TransactionDigest::random();
        let digests = vec![
            second_digest.to_string(),
            missing_digest.to_string(),
            first_digest.to_string(),
        ];

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

        mock.clear_read_rows_calls().await;
        let mut nested_request = BatchGetTransactionsRequest::default();
        nested_request.digests = digests;
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

        server.abort();
    }
}
