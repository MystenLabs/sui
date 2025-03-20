// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::field_mask::FieldMaskTree;
use crate::field_mask::FieldMaskUtil;
use crate::proto::google::rpc::bad_request::FieldViolation;
use crate::proto::node::v2::GetTransactionRequest;
use crate::proto::node::v2::GetTransactionResponse;
use crate::ErrorReason;
use crate::Result;
use crate::RpcService;
use prost_types::FieldMask;
use sui_sdk_types::TransactionDigest;
use tap::Pipe;

pub(crate) mod execution;
mod resolve;

impl RpcService {
    pub fn get_transaction(
        &self,
        GetTransactionRequest { digest, read_mask }: GetTransactionRequest,
    ) -> Result<GetTransactionResponse> {
        let transaction_digest = digest
            .ok_or_else(|| {
                FieldViolation::new("digest")
                    .with_description("missing digest")
                    .with_reason(ErrorReason::FieldMissing)
            })?
            .pipe_ref(TransactionDigest::try_from)
            .map_err(|e| {
                FieldViolation::new("digest")
                    .with_description(format!("invalid digest: {e}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })?;

        let read_mask = read_mask
            .unwrap_or_else(|| FieldMask::from_str(GetTransactionRequest::READ_MASK_DEFAULT));
        GetTransactionResponse::validate_read_mask(&read_mask).map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
        let read_mask = FieldMaskTree::from(read_mask);

        let crate::reader::TransactionRead {
            digest,
            transaction,
            signatures,
            effects,
            events,
            checkpoint,
            timestamp_ms,
        } = self.reader.get_transaction_read(transaction_digest)?;

        let transaction_bcs = read_mask
            .contains("transaction_bcs")
            .then(|| bcs::to_bytes(&transaction))
            .transpose()?
            .map(Into::into);

        let effects_bcs = read_mask
            .contains("effects_bcs")
            .then(|| bcs::to_bytes(&effects))
            .transpose()?
            .map(Into::into);

        let events_bcs = read_mask
            .contains("events_bcs")
            .then(|| events.as_ref().map(bcs::to_bytes))
            .flatten()
            .transpose()?
            .map(Into::into);

        let signatures_bytes = read_mask
            .contains("signatures_bytes")
            .then(|| {
                signatures
                    .iter()
                    .map(|signature| signature.to_bytes().into())
                    .collect()
            })
            .unwrap_or_default();

        GetTransactionResponse {
            digest: read_mask.contains("digest").then(|| digest.into()),
            transaction: read_mask
                .contains("transaction")
                .then(|| transaction.into()),
            transaction_bcs,
            signatures: read_mask
                .contains("signatures")
                .then(|| signatures.into_iter().map(Into::into).collect())
                .unwrap_or_default(),
            signatures_bytes,
            effects: read_mask.contains("effects").then(|| effects.into()),
            effects_bcs,
            events: read_mask
                .contains("events")
                .then(|| events.map(Into::into))
                .flatten(),
            events_bcs,
            checkpoint: read_mask
                .contains("checkpoint")
                .then_some(checkpoint)
                .flatten(),
            timestamp: read_mask
                .contains("timestamp")
                .then(|| timestamp_ms.map(crate::proto::types::timestamp_ms_to_proto))
                .flatten(),
        }
        .pipe(Ok)
    }
}
