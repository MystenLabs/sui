// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::field_mask::FieldMaskTree;
use crate::field_mask::FieldMaskUtil;
use crate::proto::google::rpc::bad_request::FieldViolation;
use crate::proto::google::rpc::BadRequest;
use crate::proto::node::v2::FullCheckpointObject;
use crate::proto::node::v2::FullCheckpointTransaction;
use crate::proto::node::v2::GetCheckpointRequest;
use crate::proto::node::v2::GetCheckpointResponse;
use crate::proto::node::v2::GetFullCheckpointRequest;
use crate::proto::node::v2::GetFullCheckpointResponse;
use crate::proto::types::Bcs;
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
        let checkpoint_id =
            CheckpointId::try_from_proto_request(request.sequence_number, request.digest)?;

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
        request: GetFullCheckpointRequest,
    ) -> Result<GetFullCheckpointResponse> {
        let checkpoint_id =
            CheckpointId::try_from_proto_request(request.sequence_number, request.digest)?;

        let read_mask = request
            .read_mask
            .unwrap_or_else(|| FieldMask::from_str(GetFullCheckpointRequest::READ_MASK_DEFAULT));
        GetFullCheckpointResponse::validate_read_mask(&read_mask).map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
        let read_mask = FieldMaskTree::from(read_mask);

        let verified_summary = match checkpoint_id {
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
        };

        let checkpoint_contents = self
            .reader
            .inner()
            .get_checkpoint_contents_by_digest(&verified_summary.content_digest)
            .ok_or(CheckpointNotFoundError(CheckpointId::SequenceNumber(
                *verified_summary.sequence_number(),
            )))?;

        let checkpoint = self
            .reader
            .inner()
            .get_checkpoint_data(verified_summary, checkpoint_contents)?;

        checkpoint_data_to_full_checkpoint_response(checkpoint, &read_mask)
    }
}

pub(crate) fn checkpoint_data_to_full_checkpoint_response(
    sui_types::full_checkpoint_content::CheckpointData {
        checkpoint_summary,
        checkpoint_contents,
        transactions,
    }: sui_types::full_checkpoint_content::CheckpointData,
    read_mask: &FieldMaskTree,
) -> Result<GetFullCheckpointResponse> {
    let sequence_number = checkpoint_summary.sequence_number;
    let digest: CheckpointDigest = checkpoint_summary.digest().to_owned().into();
    let (summary, signature) = checkpoint_summary.into_data_and_sig();

    let summary_bcs = read_mask
        .contains("summary_bcs")
        .then(|| bcs::to_bytes(&summary))
        .transpose()?
        .map(Into::into);
    let contents_bcs = read_mask
        .contains("contents_bcs")
        .then(|| bcs::to_bytes(&checkpoint_contents))
        .transpose()?
        .map(Into::into);

    let transactions = read_mask
        .subtree("transactions")
        .map(|read_mask| {
            transactions
                .into_iter()
                .map(|transaction| transaction_to_checkpoint_transaction(transaction, &read_mask))
                .collect::<Result<_>>()
        })
        .transpose()?
        .unwrap_or_default();

    GetFullCheckpointResponse {
        sequence_number: read_mask
            .contains("sequence_number")
            .then_some(sequence_number),
        digest: read_mask.contains("digest").then(|| digest.into()),
        summary: read_mask
            .contains("summary")
            .then(|| sui_sdk_types::CheckpointSummary::try_from(summary))
            .transpose()?
            .map(Into::into),
        summary_bcs,

        signature: read_mask
            .contains("signature")
            .then(|| sui_sdk_types::ValidatorAggregatedSignature::from(signature).into()),
        contents: read_mask
            .contains("contents")
            .then(|| sui_sdk_types::CheckpointContents::try_from(checkpoint_contents))
            .transpose()?
            .map(Into::into),
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
    read_mask: &FieldMaskTree,
) -> Result<FullCheckpointTransaction> {
    let digest = read_mask
        .contains("digest")
        .then(|| sui_sdk_types::TransactionDigest::from(transaction.digest().to_owned()).into());
    let transaction = transaction.into_data().into_inner().intent_message.value;
    let transaction_bcs = read_mask
        .contains("transaction_bcs")
        .then(|| Bcs::serialize(&transaction))
        .transpose()?;
    let transaction = read_mask
        .contains("transaction")
        .then(|| sui_sdk_types::Transaction::try_from(transaction))
        .transpose()?
        .map(Into::into);
    let effects_bcs = read_mask
        .contains("effects_bcs")
        .then(|| Bcs::serialize(&effects))
        .transpose()?;
    let effects = read_mask
        .contains("effects")
        .then(|| sui_sdk_types::TransactionEffects::try_from(effects))
        .transpose()?
        .map(Into::into);
    let events_bcs = read_mask
        .contains("events_bcs")
        .then(|| events.as_ref().map(Bcs::serialize))
        .flatten()
        .transpose()?;
    let events = read_mask
        .contains("events")
        .then(|| events.map(sui_sdk_types::TransactionEvents::try_from))
        .flatten()
        .transpose()?
        .map(Into::into);

    let input_objects = read_mask
        .subtree("input_objects")
        .map(|read_mask| {
            input_objects
                .into_iter()
                .map(|object| object_to_object_response(object, &read_mask))
                .collect::<Result<_>>()
        })
        .transpose()?
        .unwrap_or_default();

    let output_objects = read_mask
        .subtree("output_objects")
        .map(|read_mask| {
            output_objects
                .into_iter()
                .map(|object| object_to_object_response(object, &read_mask))
                .collect::<Result<_>>()
        })
        .transpose()?
        .unwrap_or_default();

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
    read_mask: &FieldMaskTree,
) -> Result<FullCheckpointObject> {
    let object_id = read_mask
        .contains("object_id")
        .then(|| sui_sdk_types::ObjectId::from(object.id()).into());
    let version = read_mask
        .contains("version")
        .then(|| object.version().value());
    let digest = read_mask
        .contains("digest")
        .then(|| sui_sdk_types::ObjectDigest::from(object.digest()).into());

    let object_bcs = read_mask
        .contains("object_bcs")
        .then(|| Bcs::serialize(&object))
        .transpose()?;
    let object = read_mask
        .contains("object")
        .then(|| sui_sdk_types::Object::try_from(object))
        .transpose()?
        .map(Into::into);

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

impl CheckpointId {
    pub fn try_from_proto_request(
        sequence_number: Option<u64>,
        digest: Option<crate::proto::types::Digest>,
    ) -> Result<Option<Self>, BadRequest> {
        match (sequence_number, digest) {
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
                return Err(bad_request);
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
        }
        .pipe(Ok)
    }
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
