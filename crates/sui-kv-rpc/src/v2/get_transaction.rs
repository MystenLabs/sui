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
