// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
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
    ) -> Result<Option<Object>, Error> {
        let key = VersionedObjectKey(id, version);
        match self {
            Self::Bigtable(loader) => loader.load_one(key).await,
            Self::Pg(loader) => loader
                .load_one(key)
                .await?
                .and_then(|stored| {
                    stored
                        .serialized_object
                        .map(|serialized_object| -> Result<Object, Error> {
                            Ok(bcs::from_bytes(serialized_object.as_slice())
                                .context("Failed to deserialize object")?)
                        })
                })
                .transpose(),
        }
    }

    pub async fn load_many_objects(
        &self,
        keys: Vec<VersionedObjectKey>,
    ) -> Result<HashMap<VersionedObjectKey, Object>, Error> {
        match self {
            Self::Bigtable(loader) => loader.load_many(keys).await,
            Self::Pg(loader) => {
                let stored_objects = loader.load_many(keys).await?;
                let mut results = HashMap::new();

                for (key, stored) in stored_objects {
                    if let Some(serialized_object) = stored.serialized_object {
                        let object = bcs::from_bytes(serialized_object.as_slice())
                            .context("Failed to deserialize object")?;
                        results.insert(key, object);
                    }
                }

                Ok(results)
            }
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
        Error,
    > {
        let key = CheckpointKey(sequence_number);
        match self {
            Self::Bigtable(loader) => loader.load_one(key).await,
            Self::Pg(loader) => loader
                .load_one(key)
                .await?
                .map(|stored| {
                    let summary: CheckpointSummary = bcs::from_bytes(&stored.checkpoint_summary)
                        .context("Failed to deserialize checkpoint summary")?;

                    let contents: CheckpointContents = bcs::from_bytes(&stored.checkpoint_contents)
                        .context("Failed to deserialize checkpoint contents")?;

                    let signature: AuthorityQuorumSignInfo<true> =
                        bcs::from_bytes(&stored.validator_signatures)
                            .context("Failed to deserialize validator signatures")?;

                    Ok((summary, contents, signature))
                })
                .transpose(),
        }
    }

    pub async fn load_one_transaction(
        &self,
        digest: TransactionDigest,
    ) -> Result<Option<TransactionContents>, Error> {
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
            Self::Pg(stored) => bcs::from_bytes(&stored.raw_transaction)
                .context("Failed to deserialize transaction data"),
            Self::Bigtable(kv) => Ok(kv.transaction.data().transaction_data().clone()),
        }
    }

    pub fn digest(&self) -> anyhow::Result<TransactionDigest> {
        match self {
            Self::Pg(stored) => TransactionDigest::try_from(stored.tx_digest.clone())
                .context("Failed to deserialize transaction digest"),
            Self::Bigtable(kv) => Ok(*kv.transaction.digest()),
        }
    }

    pub fn effects_digest(&self) -> anyhow::Result<TransactionEffectsDigest> {
        match self {
            Self::Pg(stored) => {
                let effects: TransactionEffects = bcs::from_bytes(&stored.raw_effects)
                    .context("Failed to deserialize effects")?;

                Ok(effects.digest())
            }
            Self::Bigtable(kv) => Ok(kv.effects.digest()),
        }
    }

    pub fn signatures(&self) -> anyhow::Result<Vec<GenericSignature>> {
        match self {
            Self::Pg(stored) => {
                bcs::from_bytes(&stored.user_signatures).context("Failed to deserialize signatures")
            }
            Self::Bigtable(kv) => Ok(kv.transaction.tx_signatures().to_vec()),
        }
    }

    pub fn effects(&self) -> anyhow::Result<TransactionEffects> {
        match self {
            Self::Pg(stored) => {
                bcs::from_bytes(&stored.raw_effects).context("Failed to deserialize effects")
            }
            Self::Bigtable(kv) => Ok(kv.effects.clone()),
        }
    }

    pub fn events(&self) -> anyhow::Result<Vec<Event>> {
        match self {
            Self::Pg(stored) => {
                bcs::from_bytes(&stored.events).context("Failed to deserialize events")
            }
            Self::Bigtable(kv) => Ok(kv.events.clone().unwrap_or_default().data),
        }
    }

    pub fn raw_transaction(&self) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::Pg(stored) => Ok(stored.raw_transaction.clone()),
            Self::Bigtable(kv) => bcs::to_bytes(kv.transaction.data().transaction_data())
                .context("Failed to serialize transaction"),
        }
    }

    pub fn raw_effects(&self) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::Pg(stored) => Ok(stored.raw_effects.clone()),
            Self::Bigtable(kv) => bcs::to_bytes(&kv.effects).context("Failed to serialize effects"),
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
