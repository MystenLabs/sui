// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod response_ext;
pub use response_ext::ResponseExt;

pub mod sdk;
use sdk::BoxError;

pub use reqwest;
use tap::Pipe;
use tonic::metadata::MetadataMap;

use crate::proto::node::node_service_client::NodeServiceClient;
use crate::proto::node::{
    ExecuteTransactionResponse, GetCheckpointResponse, GetFullCheckpointResponse, GetObjectResponse,
};
use crate::proto::types::Bcs;
use crate::proto::TryFromProtoError;
use crate::types::ExecuteTransactionOptions;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::{CertifiedCheckpointSummary, CheckpointSequenceNumber};
use sui_types::object::Object;
use sui_types::transaction::Transaction;

pub type Result<T, E = tonic::Status> = std::result::Result<T, E>;

use tonic::transport::channel::ClientTlsConfig;
use tonic::Status;

#[derive(Clone)]
pub struct Client {
    #[allow(unused)]
    uri: http::Uri,
    channel: tonic::transport::Channel,
}

impl Client {
    pub fn new<T>(uri: T) -> Result<Self>
    where
        T: TryInto<http::Uri>,
        T::Error: Into<BoxError>,
    {
        let uri = uri
            .try_into()
            .map_err(Into::into)
            .map_err(Status::from_error)?;
        let mut endpoint = tonic::transport::Endpoint::from(uri.clone());
        if uri.scheme() == Some(&http::uri::Scheme::HTTPS) {
            endpoint = endpoint
                .tls_config(ClientTlsConfig::new().with_enabled_roots())
                .map_err(Into::into)
                .map_err(Status::from_error)?;
        }
        let channel = endpoint.connect_lazy();

        Ok(Self { uri, channel })
    }

    pub fn raw_client(&self) -> NodeServiceClient<tonic::transport::Channel> {
        NodeServiceClient::new(self.channel.clone())
    }

    pub async fn get_latest_checkpoint(&self) -> Result<CertifiedCheckpointSummary> {
        self.get_checkpoint_internal(None).await
    }

    pub async fn get_checkpoint_summary(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<CertifiedCheckpointSummary> {
        self.get_checkpoint_internal(Some(sequence_number)).await
    }

    async fn get_checkpoint_internal(
        &self,
        sequence_number: Option<CheckpointSequenceNumber>,
    ) -> Result<CertifiedCheckpointSummary> {
        let request = crate::proto::node::GetCheckpointRequest {
            sequence_number,
            digest: None,
            options: Some(crate::proto::node::GetCheckpointOptions {
                summary: Some(false),
                summary_bcs: Some(true),
                signature: Some(true),
                contents: Some(false),
                contents_bcs: Some(false),
            }),
        };

        let (
            metadata,
            GetCheckpointResponse {
                summary_bcs,
                signature,
                ..
            },
            _extentions,
        ) = self
            .raw_client()
            .get_checkpoint(request)
            .await?
            .into_parts();

        certified_checkpoint_summary_try_from_proto(summary_bcs, signature)
            .map_err(|e| status_from_error_with_metadata(e, metadata))
    }

    pub async fn get_full_checkpoint(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<CheckpointData> {
        let request = crate::proto::node::GetFullCheckpointRequest {
            sequence_number: Some(sequence_number),
            digest: None,
            options: Some(crate::proto::node::GetFullCheckpointOptions {
                summary: Some(false),
                summary_bcs: Some(true),
                signature: Some(true),
                contents: Some(false),
                contents_bcs: Some(true),
                transaction: Some(false),
                transaction_bcs: Some(true),
                effects: Some(false),
                effects_bcs: Some(true),
                events: Some(false),
                events_bcs: Some(true),
                input_objects: Some(true),
                output_objects: Some(true),
                object: Some(false),
                object_bcs: Some(true),
            }),
        };

        let (metadata, response, _extentions) = self
            .raw_client()
            .max_decoding_message_size(64 * 1024 * 1024)
            .get_full_checkpoint(request)
            .await?
            .into_parts();

        checkpoint_data_try_from_proto(response)
            .map_err(|e| status_from_error_with_metadata(e, metadata))
    }

    pub async fn get_object(&self, object_id: ObjectID) -> Result<Object> {
        self.get_object_internal(object_id, None).await
    }

    pub async fn get_object_with_version(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> Result<Object> {
        self.get_object_internal(object_id, Some(version.value()))
            .await
    }

    async fn get_object_internal(
        &self,
        object_id: ObjectID,
        version: Option<u64>,
    ) -> Result<Object> {
        let request = crate::proto::node::GetObjectRequest {
            object_id: Some(sui_sdk_types::ObjectId::from(object_id).into()),
            version,
            options: Some(crate::proto::node::GetObjectOptions {
                object: Some(false),
                object_bcs: Some(true),
            }),
        };

        let (metadata, GetObjectResponse { object_bcs, .. }, _extentions) =
            self.raw_client().get_object(request).await?.into_parts();

        object_try_from_proto(object_bcs).map_err(|e| status_from_error_with_metadata(e, metadata))
    }

    pub async fn execute_transaction(
        &self,
        parameters: &ExecuteTransactionOptions,
        transaction: &Transaction,
    ) -> Result<TransactionExecutionResponse> {
        let signatures = transaction
            .inner()
            .tx_signatures
            .iter()
            .map(|signature| signature.as_ref().to_vec().into())
            .collect();

        let request = crate::proto::node::ExecuteTransactionRequest {
            transaction: None,
            transaction_bcs: Some(
                crate::proto::types::Bcs::serialize(&transaction.inner().intent_message.value)
                    .map_err(|e| Status::from_error(e.into()))?,
            ),
            signatures: None,
            signatures_bytes: Some(crate::proto::node::UserSignaturesBytes { signatures }),

            options: Some(crate::proto::node::ExecuteTransactionOptions {
                effects: Some(false),
                effects_bcs: Some(true),
                events: Some(false),
                events_bcs: Some(true),
                ..(parameters.to_owned().into())
            }),
        };

        let (metadata, response, _extentions) = self
            .raw_client()
            .execute_transaction(request)
            .await?
            .into_parts();

        execute_transaction_response_try_from_proto(response)
            .map_err(|e| status_from_error_with_metadata(e, metadata))
    }
}

#[derive(Debug)]
pub struct TransactionExecutionResponse {
    pub finality: crate::types::EffectsFinality,

    pub effects: TransactionEffects,
    pub events: Option<TransactionEvents>,
    pub balance_changes: Option<Vec<sui_sdk_types::BalanceChange>>,
}

/// Attempts to parse `CertifiedCheckpointSummary` from the bcs fields in `GetCheckpointResponse`
fn certified_checkpoint_summary_try_from_proto(
    summary_bcs: Option<Bcs>,
    signature: Option<crate::proto::types::ValidatorAggregatedSignature>,
) -> Result<CertifiedCheckpointSummary, TryFromProtoError> {
    let summary = summary_bcs
        .ok_or_else(|| TryFromProtoError::missing("summary_bcs"))?
        .deserialize()
        .map_err(TryFromProtoError::from_error)?;

    let signature = sui_types::crypto::AuthorityStrongQuorumSignInfo::from(
        sui_sdk_types::ValidatorAggregatedSignature::try_from(
            signature
                .as_ref()
                .ok_or_else(|| TryFromProtoError::missing("signature"))?,
        )
        .map_err(TryFromProtoError::from_error)?,
    );

    Ok(CertifiedCheckpointSummary::new_from_data_and_sig(
        summary, signature,
    ))
}

/// Attempts to parse `CheckpointData` from the bcs fields in `GetFullCheckpointResponse`
fn checkpoint_data_try_from_proto(
    GetFullCheckpointResponse {
        summary_bcs,
        signature,
        contents_bcs,
        transactions,
        ..
    }: GetFullCheckpointResponse,
) -> Result<CheckpointData, TryFromProtoError> {
    let checkpoint_summary = certified_checkpoint_summary_try_from_proto(summary_bcs, signature)?;

    let checkpoint_contents = contents_bcs
        .ok_or_else(|| TryFromProtoError::missing("contents_bcs"))?
        .deserialize::<sui_types::messages_checkpoint::CheckpointContents>()
        .map_err(TryFromProtoError::from_error)?;

    let transactions = transactions
        .into_iter()
        .zip(
            checkpoint_contents
                .clone()
                .into_iter_with_signatures()
                .map(|(_digests, signatures)| signatures),
        )
        .map(
            |(
                crate::proto::node::FullCheckpointTransaction {
                    transaction_bcs,
                    effects_bcs,
                    events_bcs,
                    input_objects,
                    output_objects,
                    ..
                },
                signatures,
            )| {
                let transaction = transaction_bcs
                    .ok_or_else(|| TryFromProtoError::missing("transaction_bcs"))?
                    .deserialize()
                    .map_err(TryFromProtoError::from_error)?;
                let transaction = Transaction::from_generic_sig_data(transaction, signatures);
                let effects = effects_bcs
                    .ok_or_else(|| TryFromProtoError::missing("effects_bcs"))?
                    .deserialize()
                    .map_err(TryFromProtoError::from_error)?;
                let events = events_bcs
                    .map(|bcs| bcs.deserialize())
                    .transpose()
                    .map_err(TryFromProtoError::from_error)?;
                let input_objects = input_objects
                    .ok_or_else(|| TryFromProtoError::missing("input_objects"))?
                    .objects
                    .into_iter()
                    .map(|object| object_try_from_proto(object.object_bcs))
                    .collect::<Result<_, TryFromProtoError>>()?;

                let output_objects = output_objects
                    .ok_or_else(|| TryFromProtoError::missing("output_objects"))?
                    .objects
                    .into_iter()
                    .map(|object| object_try_from_proto(object.object_bcs))
                    .collect::<Result<_, TryFromProtoError>>()?;

                Result::<_, TryFromProtoError>::Ok(
                    sui_types::full_checkpoint_content::CheckpointTransaction {
                        transaction,
                        effects,
                        events,
                        input_objects,
                        output_objects,
                    },
                )
            },
        )
        .collect::<Result<_, _>>()?;

    Ok(CheckpointData {
        checkpoint_summary,
        checkpoint_contents,
        transactions,
    })
}

/// Attempts to parse `Object` from the bcs fields in `GetObjectResponse`
fn object_try_from_proto(object_bcs: Option<Bcs>) -> Result<Object, TryFromProtoError> {
    object_bcs
        .as_ref()
        .ok_or_else(|| TryFromProtoError::missing("object_bcs"))?
        .deserialize()
        .map_err(TryFromProtoError::from_error)
}

/// Attempts to parse `TransactionExecutionResponse` from the fields in `TransactionExecutionResponse`
fn execute_transaction_response_try_from_proto(
    ExecuteTransactionResponse {
        finality,
        effects_bcs,
        events_bcs,
        balance_changes,
        ..
    }: ExecuteTransactionResponse,
) -> Result<TransactionExecutionResponse, TryFromProtoError> {
    let finality = finality
        .as_ref()
        .ok_or_else(|| TryFromProtoError::missing("finality"))?
        .try_into()?;

    let effects = effects_bcs
        .ok_or_else(|| TryFromProtoError::missing("effects_bcs"))?
        .deserialize()
        .map_err(TryFromProtoError::from_error)?;
    let events = events_bcs
        .map(|bcs| bcs.deserialize())
        .transpose()
        .map_err(TryFromProtoError::from_error)?;

    let balance_changes = balance_changes
        .map(|balance_changes| {
            balance_changes
                .balance_changes
                .iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()
        })
        .transpose()?;

    TransactionExecutionResponse {
        finality,
        effects,
        events,
        balance_changes,
    }
    .pipe(Ok)
}

fn status_from_error_with_metadata<T: Into<BoxError>>(err: T, metadata: MetadataMap) -> Status {
    let mut status = Status::from_error(err.into());
    *status.metadata_mut() = metadata;
    status
}
