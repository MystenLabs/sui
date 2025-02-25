// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::field_mask::FieldMaskTree;
use crate::field_mask::FieldMaskUtil;
use crate::proto::google::rpc::bad_request::FieldViolation;
use crate::proto::google::rpc::BadRequest;
use crate::proto::node::v2::GetCheckpointRequest;
use crate::proto::node::v2::GetCheckpointResponse;
use crate::types::FullCheckpointObject;
use crate::types::FullCheckpointResponse;
use crate::types::FullCheckpointTransaction;
use crate::types::GetFullCheckpointOptions;
use crate::ErrorReason;
use crate::Result;
use crate::RpcService;
use prost_types::FieldMask;
use sui_sdk_types::CheckpointContents;
use sui_sdk_types::CheckpointDigest;
use sui_sdk_types::CheckpointSequenceNumber;
use sui_sdk_types::SignedCheckpointSummary;
use tap::Pipe;

impl RpcService {
    pub fn get_checkpoint(&self, request: GetCheckpointRequest) -> Result<GetCheckpointResponse> {
        let checkpoint_id = match (request.sequence_number, request.digest) {
            (Some(_), Some(_)) => {
                let description = "only one of `sequence_number` or `digest` can be provided";
                let bad_request = BadRequest {
                    field_violations: vec![
                        FieldViolation::new("sequence_number")
                            .with_description(description)
                            .with_reason(ErrorReason::FieldInvalid),
                        FieldViolation::new("digest")
                            .with_description(description)
                            .with_reason(ErrorReason::FieldInvalid),
                    ],
                };
                return Err(bad_request.into());
            }
            (Some(sequence_number), None) => Some(CheckpointId::SequenceNumber(sequence_number)),
            (None, Some(digest)) => {
                let digest = CheckpointDigest::try_from(&digest).map_err(|e| {
                    FieldViolation::new("digest")
                        .with_description(format!("invalid digest: {e}"))
                        .with_reason(ErrorReason::FieldInvalid)
                })?;
                Some(CheckpointId::Digest(digest))
            }
            (None, None) => None,
        };

        let read_mask = request
            .read_mask
            .unwrap_or_else(|| FieldMask::from_str(GetCheckpointRequest::READ_MASK_DEFAULT));
        GetCheckpointResponse::validate_read_mask(&read_mask).map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
        let read_mask = FieldMaskTree::from(read_mask);

        let SignedCheckpointSummary {
            checkpoint,
            signature,
        } = match checkpoint_id {
            Some(checkpoint_id @ CheckpointId::SequenceNumber(s)) => self
                .reader
                .inner()
                .get_checkpoint_by_sequence_number(s)
                .ok_or(CheckpointNotFoundError(checkpoint_id))?,
            Some(checkpoint_id @ CheckpointId::Digest(d)) => self
                .reader
                .inner()
                .get_checkpoint_by_digest(&d.into())
                .ok_or(CheckpointNotFoundError(checkpoint_id))?,
            None => self.reader.inner().get_latest_checkpoint()?,
        }
        .into_inner()
        .try_into()?;

        let (contents, contents_bcs) =
            if read_mask.contains("contents") || read_mask.contains("contents_bcs") {
                let contents: CheckpointContents = self
                    .reader
                    .inner()
                    .get_checkpoint_contents_by_sequence_number(checkpoint.sequence_number)
                    .ok_or(CheckpointNotFoundError(CheckpointId::SequenceNumber(
                        checkpoint.sequence_number,
                    )))?
                    .try_into()?;

                let contents_bcs = read_mask
                    .contains("contents_bcs")
                    .then(|| bcs::to_bytes(&contents))
                    .transpose()?
                    .map(Into::into);

                (
                    read_mask.contains("contents").then(|| contents.into()),
                    contents_bcs,
                )
            } else {
                (None, None)
            };

        let summary_bcs = read_mask
            .contains("summary_bcs")
            .then(|| bcs::to_bytes(&checkpoint))
            .transpose()?
            .map(Into::into);

        GetCheckpointResponse {
            sequence_number: read_mask
                .contains("sequence_number")
                .then_some(checkpoint.sequence_number),
            digest: read_mask
                .contains("digest")
                .then(|| checkpoint.digest().into()),
            summary: read_mask.contains("summary").then(|| checkpoint.into()),
            summary_bcs,
            signature: read_mask.contains("signature").then(|| signature.into()),
            contents,
            contents_bcs,
        }
        .pipe(Ok)
    }

    pub fn get_full_checkpoint(
        &self,
        checkpoint: CheckpointId,
        options: &GetFullCheckpointOptions,
    ) -> Result<FullCheckpointResponse> {
        let verified_summary = match checkpoint {
            CheckpointId::SequenceNumber(s) => {
                let oldest_checkpoint = self
                    .reader
                    .inner()
                    .get_lowest_available_checkpoint_objects()?;
                if s < oldest_checkpoint {
                    return Err(crate::RpcError::new(
                        tonic::Code::NotFound,
                        "Old checkpoints have been pruned",
                    ));
                }

                self.reader
                    .inner()
                    .get_checkpoint_by_sequence_number(s)
                    .ok_or(CheckpointNotFoundError(checkpoint))?
            }
            CheckpointId::Digest(d) => self
                .reader
                .inner()
                .get_checkpoint_by_digest(&d.into())
                .ok_or(CheckpointNotFoundError(checkpoint))?,
        };

        let checkpoint_contents = self
            .reader
            .inner()
            .get_checkpoint_contents_by_digest(&verified_summary.content_digest)
            .ok_or(CheckpointNotFoundError(checkpoint))?;

        let checkpoint = self
            .reader
            .inner()
            .get_checkpoint_data(verified_summary, checkpoint_contents)?;

        checkpoint_data_to_full_checkpoint_response(checkpoint, options)
    }
}

pub(crate) fn checkpoint_data_to_full_checkpoint_response(
    sui_types::full_checkpoint_content::CheckpointData {
        checkpoint_summary,
        checkpoint_contents,
        transactions,
    }: sui_types::full_checkpoint_content::CheckpointData,
    options: &GetFullCheckpointOptions,
) -> Result<FullCheckpointResponse> {
    let sequence_number = checkpoint_summary.sequence_number;
    let digest = checkpoint_summary.digest().to_owned().into();
    let (summary, signature) = checkpoint_summary.into_data_and_sig();

    let summary_bcs = options
        .include_summary_bcs()
        .then(|| bcs::to_bytes(&summary))
        .transpose()?;
    let contents_bcs = options
        .include_contents_bcs()
        .then(|| bcs::to_bytes(&checkpoint_contents))
        .transpose()?;

    let transactions = transactions
        .into_iter()
        .map(|transaction| transaction_to_checkpoint_transaction(transaction, options))
        .collect::<Result<_>>()?;

    FullCheckpointResponse {
        sequence_number,
        digest,
        summary: options
            .include_summary()
            .then(|| summary.try_into())
            .transpose()?,
        summary_bcs,
        signature: options.include_signature().then(|| signature.into()),
        contents: options
            .include_contents()
            .then(|| checkpoint_contents.try_into())
            .transpose()?,
        contents_bcs,
        transactions,
    }
    .pipe(Ok)
}

fn transaction_to_checkpoint_transaction(
    sui_types::full_checkpoint_content::CheckpointTransaction {
        transaction,
        effects,
        events,
        input_objects,
        output_objects,
    }: sui_types::full_checkpoint_content::CheckpointTransaction,
    options: &GetFullCheckpointOptions,
) -> Result<FullCheckpointTransaction> {
    let digest = transaction.digest().to_owned().into();
    let transaction = transaction.into_data().into_inner().intent_message.value;
    let transaction_bcs = options
        .include_transaction_bcs()
        .then(|| bcs::to_bytes(&transaction))
        .transpose()?;
    let transaction = options
        .include_transaction()
        .then(|| transaction.try_into())
        .transpose()?;
    let effects_bcs = options
        .include_effects_bcs()
        .then(|| bcs::to_bytes(&effects))
        .transpose()?;
    let effects = options
        .include_effects()
        .then(|| effects.try_into())
        .transpose()?;
    let events_bcs = options
        .include_events_bcs()
        .then(|| events.as_ref().map(bcs::to_bytes))
        .flatten()
        .transpose()?;
    let events = options
        .include_events()
        .then(|| events.map(TryInto::try_into))
        .flatten()
        .transpose()?;

    let input_objects = options
        .include_input_objects()
        .then(|| {
            input_objects
                .into_iter()
                .map(|object| object_to_object_response(object, options))
                .collect::<Result<_>>()
        })
        .transpose()?;
    let output_objects = options
        .include_output_objects()
        .then(|| {
            output_objects
                .into_iter()
                .map(|object| object_to_object_response(object, options))
                .collect::<Result<_>>()
        })
        .transpose()?;

    FullCheckpointTransaction {
        digest,
        transaction,
        transaction_bcs,
        effects,
        effects_bcs,
        events,
        events_bcs,
        input_objects,
        output_objects,
    }
    .pipe(Ok)
}

fn object_to_object_response(
    object: sui_types::object::Object,
    options: &GetFullCheckpointOptions,
) -> Result<FullCheckpointObject> {
    let object_id = object.id().into();
    let version = object.version().value();
    let digest = object.digest().into();

    let object_bcs = options
        .include_object_bcs()
        .then(|| bcs::to_bytes(&object))
        .transpose()?;
    let object = options
        .include_object()
        .then(|| object.try_into())
        .transpose()?;

    FullCheckpointObject {
        object_id,
        version,
        digest,
        object,
        object_bcs,
    }
    .pipe(Ok)
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CheckpointId {
    SequenceNumber(CheckpointSequenceNumber),
    Digest(CheckpointDigest),
}

#[derive(Debug)]
pub struct CheckpointNotFoundError(pub CheckpointId);

impl std::fmt::Display for CheckpointNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Checkpoint ")?;

        match self.0 {
            CheckpointId::SequenceNumber(n) => write!(f, "{n}")?,
            CheckpointId::Digest(d) => write!(f, "{d}")?,
        }

        write!(f, " not found")
    }
}

impl std::error::Error for CheckpointNotFoundError {}

impl From<CheckpointNotFoundError> for crate::RpcError {
    fn from(value: CheckpointNotFoundError) -> Self {
        Self::new(tonic::Code::NotFound, value.to_string())
    }
}
