// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;
use sui_kvstore::{BigTableClient, KeyValueStoreReader, TransactionData};
use sui_rpc::field::{FieldMask, FieldMaskTree, FieldMaskUtil};
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2::{
    BatchGetTransactionsRequest, BatchGetTransactionsResponse, ExecutedTransaction,
    GetTransactionRequest, GetTransactionResponse, GetTransactionResult, Transaction,
    TransactionEffects, TransactionEvents, UserSignature,
};
use sui_rpc_api::{
    proto::google::rpc::bad_request::FieldViolation, proto::timestamp_ms_to_proto, ErrorReason,
    RpcError, TransactionNotFoundError,
};
use sui_types::base_types::TransactionDigest;

pub const READ_MASK_DEFAULT: &str = "digest";

pub async fn get_transaction(
    mut client: BigTableClient,
    request: GetTransactionRequest,
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

    let mut response = client.get_transactions(&[transaction_digest]).await?;
    let transaction = response
        .pop()
        .ok_or(TransactionNotFoundError(transaction_digest.into()))?;
    Ok(GetTransactionResponse::new(transaction_to_response(
        transaction,
        &read_mask,
    )?))
}

pub async fn batch_get_transactions(
    mut client: BigTableClient,
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

    let digests = digests
        .iter()
        .map(|digest| TransactionDigest::from_str(digest))
        .collect::<Result<Vec<_>, _>>()?;
    let transactions = client.get_transactions(&digests).await?;
    let mut tx_iter = transactions.into_iter().peekable();
    let transactions = digests
        .into_iter()
        .map(|digest| {
            if let Some(tx) = tx_iter.peek() {
                if tx.transaction.digest() == &digest {
                    return match transaction_to_response(
                        tx_iter.next().expect("invariant's checked above"),
                        &read_mask,
                    ) {
                        Ok(tx) => GetTransactionResult::new_transaction(tx),
                        Err(err) => GetTransactionResult::new_error(err.into_status_proto()),
                    };
                }
            }
            let err: RpcError = TransactionNotFoundError(digest.into()).into();
            GetTransactionResult::new_error(err.into_status_proto())
        })
        .collect();
    Ok(BatchGetTransactionsResponse::new(transactions))
}

fn transaction_to_response(
    source: TransactionData,
    mask: &FieldMaskTree,
) -> Result<ExecutedTransaction, RpcError> {
    let mut message = ExecutedTransaction::default();

    if mask.contains(ExecutedTransaction::DIGEST_FIELD.name) {
        message.digest = Some(source.transaction.digest().to_string());
    }

    if let Some(submask) = mask.subtree(ExecutedTransaction::TRANSACTION_FIELD.name) {
        let transaction =
            sui_sdk_types::Transaction::try_from(source.transaction.transaction_data().clone())?;
        message.transaction = Some(Transaction::merge_from(transaction, &submask));
    }

    if let Some(submask) = mask.subtree(ExecutedTransaction::SIGNATURES_FIELD.name) {
        message.signatures = source
            .transaction
            .tx_signatures()
            .iter()
            .map(|s| {
                sui_sdk_types::UserSignature::try_from(s.clone())
                    .map(|s| UserSignature::merge_from(s, &submask))
            })
            .collect::<Result<_, _>>()?;
    }

    if let Some(submask) = mask.subtree(ExecutedTransaction::EFFECTS_FIELD.name) {
        let effects = TransactionEffects::merge_from(
            &sui_sdk_types::TransactionEffects::try_from(source.effects)?,
            &submask,
        );
        // TODO: add support for object_types in the KV store
        message.effects = Some(effects);
    }

    if let Some(submask) = mask.subtree(ExecutedTransaction::EVENTS_FIELD.name) {
        if let Some(events) = source.events {
            message.events = Some(TransactionEvents::merge_from(
                sui_sdk_types::TransactionEvents::try_from(events)?,
                &submask,
            ));
            // TODO: add support for JSON layout
        }
    }
    if mask.contains(ExecutedTransaction::CHECKPOINT_FIELD.name) {
        message.checkpoint = Some(source.checkpoint_number);
    }
    if mask.contains(ExecutedTransaction::TIMESTAMP_FIELD.name) {
        message.timestamp = Some(timestamp_ms_to_proto(source.timestamp));
    }
    // TODO: add support for balance changes
    Ok(message)
}
