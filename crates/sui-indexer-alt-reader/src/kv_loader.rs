// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;

use async_graphql::dataloader::DataLoader;
use sui_indexer_alt_schema::transactions::StoredTransaction;
use sui_kvstore::TransactionData as KVTransactionData;
use sui_types::{
    base_types::ObjectID,
    crypto::AuthorityQuorumSignInfo,
    digests::{TransactionDigest, TransactionEffectsDigest},
    effects::TransactionEffects,
    event::Event,
    message_envelope::Message,
    messages_checkpoint::{CheckpointContents, CheckpointSummary},
    object::Object,
    signature::GenericSignature,
    transaction::TransactionData,
};

use crate::{
    bigtable_reader::BigtableReader, checkpoints::CheckpointKey, error::Error,
    objects::VersionedObjectKey, pg_reader::PgReader, transactions::TransactionKey,
};

/// A loader for point lookups in kv stores backed by either Bigtable or Postgres.
/// Supported lookups:
/// - Objects by id and version
/// - Checkpoints by sequence number
/// - Transactions by digest
#[derive(Clone)]
pub enum KvLoader {
    Bigtable(Arc<DataLoader<BigtableReader>>),
    Pg(Arc<DataLoader<PgReader>>),
}

/// A wrapper for the contents of a transaction, either from Bigtable or Postgres.
pub enum TransactionContents {
    Bigtable(KVTransactionData),
    Pg(StoredTransaction),
}

impl KvLoader {
    pub fn new_with_bigtable(bigtable_loader: Arc<DataLoader<BigtableReader>>) -> Self {
        Self::Bigtable(bigtable_loader)
    }

    pub fn new_with_pg(pg_loader: Arc<DataLoader<PgReader>>) -> Self {
        Self::Pg(pg_loader)
    }

    pub async fn load_one_object(
        &self,
        id: ObjectID,
        version: u64,
    ) -> Result<Option<Object>, Arc<Error>> {
        let key = VersionedObjectKey(id, version);
        match self {
            Self::Bigtable(loader) => loader.load_one(key).await,
            Self::Pg(loader) => loader
                .load_one(key)
                .await?
                .and_then(|stored| {
                    stored.serialized_object.map(
                        |serialized_object| -> Result<Object, Arc<Error>> {
                            bcs::from_bytes(serialized_object.as_slice())
                                .map_err(|e| Arc::new(Error::Serde(e.into())))
                        },
                    )
                })
                .transpose(),
        }
    }

    pub async fn load_many_objects(
        &self,
        keys: Vec<VersionedObjectKey>,
    ) -> Result<HashMap<VersionedObjectKey, Object>, Arc<Error>> {
        match self {
            Self::Bigtable(loader) => loader.load_many(keys).await,
            Self::Pg(loader) => loader
                .load_many(keys)
                .await?
                .into_iter()
                .flat_map(|(key, stored)| {
                    stored.serialized_object.map(
                        |serialized_object| -> Result<(VersionedObjectKey, Object), Arc<Error>> {
                            Ok((
                                key,
                                bcs::from_bytes(serialized_object.as_slice())
                                    .map_err(|e| Arc::new(Error::Serde(e.into())))?,
                            ))
                        },
                    )
                })
                .collect(),
        }
    }

    pub async fn load_one_checkpoint(
        &self,
        sequence_number: u64,
    ) -> Result<
        Option<(
            CheckpointSummary,
            CheckpointContents,
            AuthorityQuorumSignInfo<true>,
        )>,
        Arc<Error>,
    > {
        let key = CheckpointKey(sequence_number);
        match self {
            Self::Bigtable(loader) => loader.load_one(key).await,
            Self::Pg(loader) => loader
                .load_one(key)
                .await?
                .map(|stored| {
                    let summary: CheckpointSummary = bcs::from_bytes(&stored.checkpoint_summary)
                        .map_err(|e| Error::Serde(e.into()))?;

                    let contents: CheckpointContents = bcs::from_bytes(&stored.checkpoint_contents)
                        .map_err(|e| Error::Serde(e.into()))?;

                    let signature: AuthorityQuorumSignInfo<true> =
                        bcs::from_bytes(&stored.validator_signatures)
                            .map_err(|e| Error::Serde(e.into()))?;

                    Ok((summary, contents, signature))
                })
                .transpose(),
        }
    }

    pub async fn load_one_transaction(
        &self,
        digest: TransactionDigest,
    ) -> Result<Option<TransactionContents>, Arc<Error>> {
        let key = TransactionKey(digest);
        match self {
            Self::Bigtable(loader) => Ok(loader
                .load_one(key)
                .await?
                .map(TransactionContents::Bigtable)),
            Self::Pg(loader) => Ok(loader.load_one(key).await?.map(TransactionContents::Pg)),
        }
    }
}

impl TransactionContents {
    pub fn data(&self) -> anyhow::Result<TransactionData> {
        match self {
            Self::Pg(stored) => {
                let data: TransactionData =
                    bcs::from_bytes(&stored.raw_transaction).map_err(|e| {
                        anyhow::anyhow!("Failed to deserialize transaction data: {}", e)
                    })?;

                Ok(data)
            }
            Self::Bigtable(kv) => Ok(kv.transaction.data().transaction_data().clone()),
        }
    }

    pub fn digest(&self) -> anyhow::Result<TransactionDigest> {
        match self {
            Self::Pg(stored) => {
                let digest =
                    TransactionDigest::try_from(stored.tx_digest.clone()).map_err(|e| {
                        anyhow::anyhow!("Failed to deserialize transaction digest: {}", e)
                    })?;

                Ok(digest)
            }
            Self::Bigtable(kv) => Ok(*kv.transaction.digest()),
        }
    }

    pub fn effects_digest(&self) -> anyhow::Result<TransactionEffectsDigest> {
        match self {
            Self::Pg(stored) => {
                let effects: TransactionEffects = bcs::from_bytes(&stored.raw_effects)
                    .map_err(|e| anyhow::anyhow!("Failed to deserialize effects: {}", e))?;

                Ok(effects.digest())
            }
            Self::Bigtable(kv) => Ok(kv.effects.digest()),
        }
    }

    pub fn signatures(&self) -> anyhow::Result<Vec<GenericSignature>> {
        match self {
            Self::Pg(stored) => {
                let signatures: Vec<GenericSignature> = bcs::from_bytes(&stored.user_signatures)
                    .map_err(|e| anyhow::anyhow!("Failed to deserialize signatures: {}", e))?;

                Ok(signatures)
            }
            Self::Bigtable(kv) => Ok(kv.transaction.tx_signatures().to_vec()),
        }
    }

    pub fn effects(&self) -> anyhow::Result<TransactionEffects> {
        match self {
            Self::Pg(stored) => {
                let effects: TransactionEffects = bcs::from_bytes(&stored.raw_effects)
                    .map_err(|e| anyhow::anyhow!("Failed to deserialize effects: {}", e))?;

                Ok(effects)
            }
            Self::Bigtable(kv) => Ok(kv.effects.clone()),
        }
    }

    pub fn events(&self) -> anyhow::Result<Vec<Event>> {
        match self {
            Self::Pg(stored) => {
                let events: Vec<Event> = bcs::from_bytes(&stored.events)
                    .map_err(|e| anyhow::anyhow!("Failed to deserialize events: {}", e))?;

                Ok(events)
            }
            Self::Bigtable(kv) => Ok(kv.events.clone().unwrap_or_default().data),
        }
    }

    pub fn raw_transaction(&self) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::Pg(stored) => Ok(stored.raw_transaction.clone()),
            Self::Bigtable(kv) => bcs::to_bytes(kv.transaction.data().transaction_data())
                .map_err(|e| anyhow::anyhow!("Failed to serialize transaction: {}", e)),
        }
    }

    pub fn raw_effects(&self) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::Pg(stored) => Ok(stored.raw_effects.clone()),
            Self::Bigtable(kv) => bcs::to_bytes(&kv.effects)
                .map_err(|e| anyhow::anyhow!("Failed to serialize effects: {}", e)),
        }
    }

    pub fn timestamp_ms(&self) -> u64 {
        match self {
            Self::Pg(stored) => stored.timestamp_ms as u64,
            Self::Bigtable(kv) => kv.timestamp,
        }
    }

    pub fn cp_sequence_number(&self) -> u64 {
        match self {
            Self::Pg(stored) => stored.cp_sequence_number as u64,
            Self::Bigtable(kv) => kv.checkpoint_number,
        }
    }
}
