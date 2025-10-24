// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tap::Pipe;
use tonic::metadata::MetadataMap;

use prost_types::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::TryFromProtoError;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::full_checkpoint_content::ObjectSet;
use sui_types::messages_checkpoint::{CertifiedCheckpointSummary, CheckpointSequenceNumber};
use sui_types::object::Object;
use sui_types::transaction::Transaction;

pub use sui_rpc::client::HeadersInterceptor;
pub use sui_rpc::client::ResponseExt;

pub type Result<T, E = tonic::Status> = std::result::Result<T, E>;
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

use tonic::Status;

#[derive(Clone)]
pub struct Client(sui_rpc::Client);

impl Client {
    pub fn new<T>(uri: T) -> Result<Self>
    where
        T: TryInto<http::Uri>,
        T::Error: Into<BoxError>,
    {
        sui_rpc::Client::new(uri).map(Self)
    }

    pub fn with_headers(self, headers: HeadersInterceptor) -> Self {
        Self(self.0.with_headers(headers))
    }

    pub async fn get_latest_checkpoint(&mut self) -> Result<CertifiedCheckpointSummary> {
        self.get_checkpoint_internal(None).await
    }

    pub async fn get_checkpoint_summary(
        &mut self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<CertifiedCheckpointSummary> {
        self.get_checkpoint_internal(Some(sequence_number)).await
    }

    async fn get_checkpoint_internal(
        &mut self,
        sequence_number: Option<CheckpointSequenceNumber>,
    ) -> Result<CertifiedCheckpointSummary> {
        let mut request = proto::GetCheckpointRequest::default()
            .with_read_mask(FieldMask::from_paths(["summary.bcs", "signature"]));
        request.checkpoint_id = sequence_number.map(|sequence_number| {
            proto::get_checkpoint_request::CheckpointId::SequenceNumber(sequence_number)
        });

        let (metadata, checkpoint, _extentions) = self
            .0
            .ledger_client()
            .get_checkpoint(request)
            .await?
            .into_parts();

        let checkpoint = checkpoint
            .checkpoint
            .ok_or_else(|| tonic::Status::not_found("no checkpoint returned"))?;
        certified_checkpoint_summary_try_from_proto(&checkpoint)
            .map_err(|e| status_from_error_with_metadata(e, metadata))
    }

    pub async fn get_full_checkpoint(
        &mut self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<CheckpointData> {
        let request = proto::GetCheckpointRequest::by_sequence_number(sequence_number)
            .with_read_mask(FieldMask::from_paths([
                "summary.bcs",
                "signature",
                "contents.bcs",
                "transactions.transaction.bcs",
                "transactions.effects.bcs",
                "transactions.effects.unchanged_loaded_runtime_objects",
                "transactions.events.bcs",
                "objects.objects.bcs",
            ]));

        let (metadata, response, _extentions) = self
            .0
            .ledger_client()
            .max_decoding_message_size(128 * 1024 * 1024)
            .get_checkpoint(request)
            .await?
            .into_parts();

        let checkpoint = response
            .checkpoint
            .ok_or_else(|| tonic::Status::not_found("no checkpoint returned"))?;
        sui_types::full_checkpoint_content::Checkpoint::try_from(&checkpoint)
            .map(Into::into)
            .map_err(|e| status_from_error_with_metadata(e, metadata))
    }

    pub async fn get_object(&mut self, object_id: ObjectID) -> Result<Object> {
        self.get_object_internal(object_id, None).await
    }

    pub async fn get_object_with_version(
        &mut self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> Result<Object> {
        self.get_object_internal(object_id, Some(version.value()))
            .await
    }

    async fn get_object_internal(
        &mut self,
        object_id: ObjectID,
        version: Option<u64>,
    ) -> Result<Object> {
        let mut request = proto::GetObjectRequest::new(&object_id.into())
            .with_read_mask(FieldMask::from_paths(["bcs"]));
        request.version = version;

        let (metadata, object, _extentions) = self
            .0
            .ledger_client()
            .get_object(request)
            .await?
            .into_parts();

        let object = object
            .object
            .ok_or_else(|| tonic::Status::not_found("no object returned"))?;
        object_try_from_proto(&object).map_err(|e| status_from_error_with_metadata(e, metadata))
    }

    pub async fn execute_transaction(
        &mut self,
        transaction: &Transaction,
    ) -> Result<TransactionExecutionResponse> {
        let signatures = transaction
            .inner()
            .tx_signatures
            .iter()
            .map(|signature| {
                let mut message = proto::UserSignature::default();
                message.bcs = Some(signature.as_ref().to_vec().into());
                message
            })
            .collect();

        let request = proto::ExecuteTransactionRequest::new({
            let mut tx = proto::Transaction::default();
            tx.bcs = Some(
                proto::Bcs::serialize(&transaction.inner().intent_message.value)
                    .map_err(|e| Status::from_error(e.into()))?,
            );
            tx
        })
        .with_signatures(signatures)
        .with_read_mask(FieldMask::from_paths([
            "effects.bcs",
            "events.bcs",
            "balance_changes",
            "objects.objects.bcs",
        ]));

        let (metadata, response, _extentions) = self
            .0
            .execution_client()
            .execute_transaction(request)
            .await?
            .into_parts();

        execute_transaction_response_try_from_proto(&response)
            .map_err(|e| status_from_error_with_metadata(e, metadata))
    }
}

#[derive(Debug)]
pub struct TransactionExecutionResponse {
    pub effects: TransactionEffects,
    pub events: Option<TransactionEvents>,
    pub balance_changes: Vec<sui_sdk_types::BalanceChange>,
    pub objects: ObjectSet,
}

/// Attempts to parse `CertifiedCheckpointSummary` from a proto::Checkpoint
#[allow(clippy::result_large_err)]
fn certified_checkpoint_summary_try_from_proto(
    checkpoint: &proto::Checkpoint,
) -> Result<CertifiedCheckpointSummary, TryFromProtoError> {
    let summary = checkpoint
        .summary
        .as_ref()
        .and_then(|summary| summary.bcs.as_ref())
        .ok_or_else(|| TryFromProtoError::missing("summary.bcs"))?
        .deserialize()
        .map_err(|e| TryFromProtoError::invalid("summary.bcs", e))?;

    let signature = sui_types::crypto::AuthorityStrongQuorumSignInfo::from(
        sui_sdk_types::ValidatorAggregatedSignature::try_from(
            checkpoint
                .signature
                .as_ref()
                .ok_or_else(|| TryFromProtoError::missing("signature"))?,
        )
        .map_err(|e| TryFromProtoError::invalid("signature", e))?,
    );

    Ok(CertifiedCheckpointSummary::new_from_data_and_sig(
        summary, signature,
    ))
}

/// Attempts to parse `Object` from the bcs fields in `GetObjectResponse`
#[allow(clippy::result_large_err)]
fn object_try_from_proto(object: &proto::Object) -> Result<Object, TryFromProtoError> {
    object
        .bcs
        .as_ref()
        .ok_or_else(|| TryFromProtoError::missing("bcs"))?
        .deserialize()
        .map_err(|e| TryFromProtoError::invalid("bcs", e))
}

/// Attempts to parse `TransactionExecutionResponse` from the fields in `TransactionExecutionResponse`
#[allow(clippy::result_large_err)]
fn execute_transaction_response_try_from_proto(
    response: &proto::ExecuteTransactionResponse,
) -> Result<TransactionExecutionResponse, TryFromProtoError> {
    let executed_transaction = response
        .transaction
        .as_ref()
        .ok_or_else(|| TryFromProtoError::missing("transaction"))?;

    let effects = executed_transaction
        .effects
        .as_ref()
        .and_then(|effects| effects.bcs.as_ref())
        .ok_or_else(|| TryFromProtoError::missing("effects_bcs"))?
        .deserialize()
        .map_err(|e| TryFromProtoError::invalid("effects.bcs", e))?;
    let events = executed_transaction
        .events
        .as_ref()
        .and_then(|events| events.bcs.as_ref())
        .map(|bcs| bcs.deserialize())
        .transpose()
        .map_err(|e| TryFromProtoError::invalid("events.bcs", e))?;

    let balance_changes = executed_transaction
        .balance_changes
        .iter()
        .map(TryInto::try_into)
        .collect::<Result<_, _>>()?;

    let objects = executed_transaction
        .objects()
        .try_into()
        .map_err(|e| TryFromProtoError::invalid("objects.bcs", e))?;

    TransactionExecutionResponse {
        effects,
        events,
        balance_changes,
        objects,
    }
    .pipe(Ok)
}

fn status_from_error_with_metadata<T: Into<BoxError>>(err: T, metadata: MetadataMap) -> Status {
    let mut status = Status::from_error(err.into());
    *status.metadata_mut() = metadata;
    status
}
