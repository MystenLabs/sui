// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use async_graphql::dataloader::{DataLoader, Loader};
use prost_types::FieldMask;
use sui_kvstore::TransactionEventsData;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2beta2 as proto;
use sui_rpc_api::client::Client as GrpcClient;
use sui_types::{
    crypto::AuthorityQuorumSignInfo,
    effects::TransactionEvents,
    messages_checkpoint::{CheckpointContents, CheckpointSummary},
    object::Object,
    signature::GenericSignature,
    transaction::TransactionData,
};

use crate::{
    checkpoints::CheckpointKey,
    error::Error,
    events::TransactionEventsKey,
    kv_loader::TransactionContents,
    objects::VersionedObjectKey,
    transactions::TransactionKey,
};

/// A reader backed by gRPC LedgerService.
#[derive(Clone)]
pub struct GrpcReader(Arc<GrpcClient>);

impl GrpcReader {
    pub fn new(client: Arc<GrpcClient>) -> Self {
        Self(client)
    }

    pub fn as_data_loader(&self) -> DataLoader<Self> {
        DataLoader::new(self.clone(), tokio::spawn)
    }
}

#[async_trait::async_trait]
impl Loader<VersionedObjectKey> for GrpcReader {
    type Value = Object;
    type Error = Error;

    async fn load(
        &self,
        keys: &[VersionedObjectKey],
    ) -> Result<HashMap<VersionedObjectKey, Object>, Error> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut results = HashMap::new();
        for key in keys {
            match self
                .0
                .get_object_with_version(key.0, key.1.into())
                .await
            {
                Ok(obj) => {
                    results.insert(*key, obj);
                }
                Err(status) if status.code() == tonic::Code::NotFound => continue,
                Err(e) => return Err(Error::Tonic(e.into())),
            }
        }
        Ok(results)
    }
}

#[async_trait::async_trait]
impl Loader<CheckpointKey> for GrpcReader {
    type Value = (
        CheckpointSummary,
        CheckpointContents,
        AuthorityQuorumSignInfo<true>,
    );
    type Error = Error;

    async fn load(
        &self,
        keys: &[CheckpointKey],
    ) -> Result<HashMap<CheckpointKey, Self::Value>, Error> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut results = HashMap::new();
        for key in keys {
            let request = proto::GetCheckpointRequest::by_sequence_number(key.0)
                .with_read_mask(FieldMask::from_paths([
                    "summary.bcs",
                    "signature",
                    "contents.bcs",
                ]));

            match self.0.raw_client().get_checkpoint(request).await {
                Ok(response) => {
                    let checkpoint = response
                        .into_inner()
                        .checkpoint
                        .context("No checkpoint returned")?;

                    let summary: CheckpointSummary = checkpoint
                        .summary
                        .as_ref()
                        .and_then(|s| s.bcs.as_ref())
                        .context("Missing summary.bcs")?
                        .deserialize()
                        .context("Failed to deserialize checkpoint summary")?;

                    let contents: CheckpointContents = checkpoint
                        .contents
                        .as_ref()
                        .and_then(|c| c.bcs.as_ref())
                        .context("Missing contents.bcs")?
                        .deserialize()
                        .context("Failed to deserialize checkpoint contents")?;

                    let signature: AuthorityQuorumSignInfo<true> = {
                        let sdk_sig = sui_sdk_types::ValidatorAggregatedSignature::try_from(
                            checkpoint
                                .signature
                                .as_ref()
                                .context("Missing signature")?,
                        )
                        .context("Failed to parse signature")?;
                        AuthorityQuorumSignInfo::from(sdk_sig)
                    };

                    results.insert(*key, (summary, contents, signature));
                }
                Err(status) if status.code() == tonic::Code::NotFound => continue,
                Err(e) => return Err(Error::Tonic(e.into())),
            }
        }
        Ok(results)
    }
}

#[async_trait::async_trait]
impl Loader<TransactionKey> for GrpcReader {
    type Value = TransactionContents;
    type Error = Error;

    async fn load(
        &self,
        keys: &[TransactionKey],
    ) -> Result<HashMap<TransactionKey, Self::Value>, Error> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut results = HashMap::new();
        for key in keys {
            let request = proto::GetTransactionRequest::new(&key.0.into())
                .with_read_mask(FieldMask::from_paths([
                    "transaction.bcs",
                    "effects.bcs",
                    "events.bcs",
                ]));

            match self.0.raw_client().get_transaction(request).await {
                Ok(response) => {
                    let executed = response
                        .into_inner()
                        .transaction
                        .context("No transaction returned")?;

                    let transaction_data: TransactionData = executed
                        .transaction
                        .as_ref()
                        .and_then(|t| t.bcs.as_ref())
                        .context("Missing transaction.bcs")?
                        .deserialize()
                        .context("Failed to deserialize transaction data")?;

                    let signatures: Vec<GenericSignature> = executed
                        .signatures
                        .iter()
                        .map(|sig| {
                            sig.bcs
                                .as_ref()
                                .context("Missing signature.bcs")?
                                .deserialize()
                                .context("Failed to deserialize signature")
                        })
                        .collect::<anyhow::Result<Vec<_>>>()?;

                    let contents = TransactionContents::from_executed_transaction(
                        &executed,
                        transaction_data,
                        signatures,
                    )?;

                    results.insert(*key, contents);
                }
                Err(status) if status.code() == tonic::Code::NotFound => continue,
                Err(e) => return Err(Error::Tonic(e.into())),
            }
        }
        Ok(results)
    }
}

#[async_trait::async_trait]
impl Loader<TransactionEventsKey> for GrpcReader {
    type Value = TransactionEventsData;
    type Error = Error;

    async fn load(
        &self,
        keys: &[TransactionEventsKey],
    ) -> Result<HashMap<TransactionEventsKey, Self::Value>, Self::Error> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut results = HashMap::new();
        for key in keys {
            let request = proto::GetTransactionRequest::new(&key.0.into())
                .with_read_mask(FieldMask::from_paths(["events.bcs", "timestamp"]));

            match self.0.raw_client().get_transaction(request).await {
                Ok(response) => {
                    let executed = response
                        .into_inner()
                        .transaction
                        .context("No transaction returned")?;

                    let events = executed
                        .events
                        .as_ref()
                        .and_then(|e| e.bcs.as_ref())
                        .map(|bcs| -> anyhow::Result<_> {
                            let tx_events: TransactionEvents = bcs.deserialize()
                                .context("Failed to deserialize transaction events")?;
                            Ok(tx_events.data)
                        })
                        .transpose()?
                        .unwrap_or_default();

                    let timestamp_ms = executed
                        .timestamp
                        .map(|ts| ts.seconds as u64 * 1000 + ts.nanos as u64 / 1_000_000)
                        .unwrap_or(0);

                    results.insert(
                        *key,
                        TransactionEventsData {
                            events,
                            timestamp_ms,
                        },
                    );
                }
                Err(status) if status.code() == tonic::Code::NotFound => continue,
                Err(e) => return Err(Error::Tonic(e.into())),
            }
        }
        Ok(results)
    }
}
