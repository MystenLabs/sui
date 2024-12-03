// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod sdk;
use sdk::BoxError;
use sdk::Error;
use sdk::Result;

pub use reqwest;
use tap::Pipe;

use crate::proto::node::node_client::NodeClient;
use crate::types::ExecuteTransactionOptions;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::{CertifiedCheckpointSummary, CheckpointSequenceNumber};
use sui_types::object::Object;
use sui_types::transaction::Transaction;

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
        let uri = uri.try_into().map_err(Error::from_error)?;
        let channel = tonic::transport::Endpoint::from(uri.clone()).connect_lazy();

        Ok(Self { uri, channel })
    }

    pub fn raw_client(&self) -> NodeClient<tonic::transport::Channel> {
        NodeClient::new(self.channel.clone())
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

        let crate::proto::node::GetCheckpointResponse {
            summary_bcs,
            signature,
            ..
        } = self
            .raw_client()
            .get_checkpoint(request)
            .await
            .map_err(Error::from_error)?
            .into_inner();

        let summary = summary_bcs
            .ok_or_else(|| Error::from_error("missing summary"))?
            .deserialize()?;

        let signature = sui_types::crypto::AuthorityStrongQuorumSignInfo::from(
            sui_sdk_types::types::ValidatorAggregatedSignature::try_from(
                &signature.ok_or_else(|| Error::from_error("missing signautre"))?,
            )
            .map_err(Error::from_error)?,
        );

        Ok(CertifiedCheckpointSummary::new_from_data_and_sig(
            summary, signature,
        ))
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

        let crate::proto::node::GetFullCheckpointResponse {
            summary_bcs,
            signature,
            contents_bcs,
            transactions,
            ..
        } = self
            .raw_client()
            .get_full_checkpoint(request)
            .await
            .map_err(Error::from_error)?
            .into_inner();

        let summary = summary_bcs
            .ok_or_else(|| Error::from_error("missing summary"))?
            .deserialize()?;
        let signature = sui_types::crypto::AuthorityStrongQuorumSignInfo::from(
            sui_sdk_types::types::ValidatorAggregatedSignature::try_from(
                &signature.ok_or_else(|| Error::from_error("missing signautre"))?,
            )
            .map_err(Error::from_error)?,
        );
        let checkpoint_summary =
            CertifiedCheckpointSummary::new_from_data_and_sig(summary, signature);

        let checkpoint_contents = contents_bcs
            .ok_or_else(|| Error::from_error("missing contents"))?
            .deserialize::<sui_types::messages_checkpoint::CheckpointContents>()?;

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
                        .ok_or_else(|| Error::from_error("missing transaction"))?
                        .deserialize()?;
                    let transaction = Transaction::from_generic_sig_data(transaction, signatures);
                    let effects = effects_bcs
                        .ok_or_else(|| Error::from_error("missing effects"))?
                        .deserialize()?;
                    let events = events_bcs.map(|bcs| bcs.deserialize()).transpose()?;
                    let input_objects = input_objects
                        .ok_or_else(|| Error::from_error("missing input_objects"))?
                        .objects
                        .into_iter()
                        .map(|object| {
                            object
                                .object_bcs
                                .as_ref()
                                .ok_or_else(|| Error::from_error("missing object"))?
                                .deserialize::<Object>()
                                .map_err(Into::into)
                        })
                        .collect::<Result<_>>()?;

                    let output_objects = output_objects
                        .ok_or_else(|| Error::from_error("missing output_objects"))?
                        .objects
                        .into_iter()
                        .map(|object| {
                            object
                                .object_bcs
                                .ok_or_else(|| Error::from_error("missing object"))?
                                .deserialize::<Object>()
                                .map_err(Into::into)
                        })
                        .collect::<Result<_>>()?;

                    Result::<_>::Ok(sui_types::full_checkpoint_content::CheckpointTransaction {
                        transaction,
                        effects,
                        events,
                        input_objects,
                        output_objects,
                    })
                },
            )
            .collect::<Result<_, _>>()?;

        Ok(CheckpointData {
            checkpoint_summary,
            checkpoint_contents,
            transactions,
        })
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
            object_id: Some(sui_sdk_types::types::ObjectId::from(object_id).into()),
            version,
            options: Some(crate::proto::node::GetObjectOptions {
                object: Some(false),
                object_bcs: Some(true),
            }),
        };

        let crate::proto::node::GetObjectResponse { object_bcs, .. } = self
            .raw_client()
            .get_object(request)
            .await
            .map_err(Error::from_error)?
            .into_inner();

        object_bcs
            .ok_or_else(|| Error::from_error("missing object"))?
            .deserialize()
            .map_err(Into::into)
    }

    pub async fn execute_transaction(
        &self,
        parameters: &ExecuteTransactionOptions,
        transaction: &Transaction,
    ) -> Result<TransactionExecutionResponse> {
        let signatures = transaction
            .inner()
            .tx_signatures
            .clone()
            .into_iter()
            .map(sui_sdk_types::types::UserSignature::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        let request = crate::proto::node::ExecuteTransactionRequest {
            transaction: None,
            transaction_bcs: Some(crate::proto::types::Bcs::serialize(
                &transaction.inner().intent_message.value,
            )?),
            signatures: signatures.into_iter().map(Into::into).collect(),

            options: Some(crate::proto::node::ExecuteTransactionOptions {
                effects: Some(false),
                effects_bcs: Some(true),
                events: Some(false),
                events_bcs: Some(true),
                ..(parameters.to_owned().into())
            }),
        };

        let crate::proto::node::ExecuteTransactionResponse {
            finality,
            effects_bcs,
            events_bcs,
            balance_changes,
            ..
        } = self
            .raw_client()
            .execute_transaction(request)
            .await
            .map_err(Error::from_error)?
            .into_inner();

        let finality = finality
            .as_ref()
            .ok_or_else(|| Error::from_error("missing finality"))?
            .pipe(TryInto::try_into)
            .map_err(Error::from_error)?;

        let effects = effects_bcs
            .ok_or_else(|| Error::from_error("missing effects"))?
            .deserialize()?;
        let events = events_bcs.map(|bcs| bcs.deserialize()).transpose()?;

        let balance_changes = balance_changes
            .map(|balance_changes| {
                balance_changes
                    .balance_changes
                    .iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()
            })
            .transpose()
            .map_err(Error::from_error)?;

        TransactionExecutionResponse {
            finality,
            effects,
            events,
            balance_changes,
        }
        .pipe(Ok)
    }
}

#[derive(Debug)]
pub struct TransactionExecutionResponse {
    pub finality: crate::types::EffectsFinality,

    pub effects: TransactionEffects,
    pub events: Option<TransactionEvents>,
    pub balance_changes: Option<Vec<sui_sdk_types::types::BalanceChange>>,
}
