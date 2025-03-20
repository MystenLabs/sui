// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::field_mask::FieldMaskTree;
use crate::field_mask::FieldMaskUtil;
use crate::message::MessageMerge;
use crate::message::MessageMergeFrom;
use crate::proto::google::rpc::bad_request::FieldViolation;
use crate::proto::rpc::v2beta::BatchGetTransactionsRequest;
use crate::proto::rpc::v2beta::BatchGetTransactionsResponse;
use crate::proto::rpc::v2beta::ExecutedTransaction;
use crate::proto::rpc::v2beta::GetTransactionRequest;
use crate::proto::rpc::v2beta::Transaction;
use crate::proto::rpc::v2beta::TransactionEffects;
use crate::proto::rpc::v2beta::TransactionEvents;
use crate::proto::rpc::v2beta::UserSignature;
use crate::proto::types::timestamp_ms_to_proto;
use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;
use prost_types::FieldMask;
use sui_sdk_types::TransactionDigest;

#[tracing::instrument(skip(service))]
pub fn get_transaction(
    service: &RpcService,
    request: GetTransactionRequest,
) -> Result<ExecutedTransaction, RpcError> {
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
            .unwrap_or_else(|| FieldMask::from_str(GetTransactionRequest::READ_MASK_DEFAULT));
        read_mask
            .validate::<ExecutedTransaction>()
            .map_err(|path| {
                FieldViolation::new("read_mask")
                    .with_description(format!("invalid read_mask path: {path}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })?;
        FieldMaskTree::from(read_mask)
    };

    let transaction_read = service.reader.get_transaction_read(transaction_digest)?;

    Ok(ExecutedTransaction::merge_from(
        transaction_read,
        &read_mask,
    ))
}

#[tracing::instrument(skip(service))]
pub fn batch_get_transactions(
    service: &RpcService,
    BatchGetTransactionsRequest { digests, read_mask }: BatchGetTransactionsRequest,
) -> Result<BatchGetTransactionsResponse, RpcError> {
    let read_mask = {
        let read_mask = read_mask
            .unwrap_or_else(|| FieldMask::from_str(BatchGetTransactionsRequest::READ_MASK_DEFAULT));
        read_mask
            .validate::<ExecutedTransaction>()
            .map_err(|path| {
                FieldViolation::new("read_mask")
                    .with_description(format!("invalid read_mask path: {path}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })?;
        FieldMaskTree::from(read_mask)
    };

    let transactions = digests
        .into_iter()
        .enumerate()
        .map(|(idx, digest)| {
            let digest = digest.parse().map_err(|e| {
                FieldViolation::new_at("digests", idx)
                    .with_description(format!("invalid digest: {e}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })?;

            service
                .reader
                .get_transaction_read(digest)
                .map(|transaction_read| {
                    ExecutedTransaction::merge_from(transaction_read, &read_mask)
                })
        })
        .collect::<Result<_, _>>()?;

    Ok(BatchGetTransactionsResponse { transactions })
}

impl MessageMerge<crate::reader::TransactionRead> for ExecutedTransaction {
    fn merge(
        &mut self,
        source: crate::reader::TransactionRead,
        mask: &crate::field_mask::FieldMaskTree,
    ) {
        if mask.contains(Self::DIGEST_FIELD.name) {
            self.digest = Some(source.digest.to_string());
        }

        if let Some(submask) = mask.subtree(Self::TRANSACTION_FIELD.name) {
            self.transaction = Some(Transaction::merge_from(source.transaction, &submask));
        }

        if let Some(submask) = mask.subtree(Self::SIGNATURES_FIELD.name) {
            self.signatures = source
                .signatures
                .into_iter()
                .map(|s| UserSignature::merge_from(s, &submask))
                .collect();
        }

        if let Some(submask) = mask.subtree(Self::EFFECTS_FIELD.name) {
            self.effects = Some(TransactionEffects::merge_from(&source.effects, &submask));
        }

        if let Some(submask) = mask.subtree(Self::EVENTS_FIELD.name) {
            self.events = source
                .events
                .map(|events| TransactionEvents::merge_from(events, &submask));
        }

        if mask.contains(Self::CHECKPOINT_FIELD.name) {
            self.checkpoint = source.checkpoint;
        }

        if mask.contains(Self::TIMESTAMP_FIELD.name) {
            self.timestamp = source.timestamp_ms.map(timestamp_ms_to_proto);
        }
    }
}
