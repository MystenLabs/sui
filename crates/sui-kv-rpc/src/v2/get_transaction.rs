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

pub async fn get_transaction(
    client: BigTableClient,
    stages: &StagesConfig,
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

pub async fn batch_get_transactions(
    client: BigTableClient,
    stages: &StagesConfig,
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
        assert_identity_only_object_mask, canonical_transaction_object_keys,
        query_context_with_mock_and_registry, response_object_keys,
        two_transaction_object_checkpoint,
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
