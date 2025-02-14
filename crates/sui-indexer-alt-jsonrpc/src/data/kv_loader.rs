// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::objects::VersionedObjectKey;
use super::pg_reader::PgReader;
use super::read_error::ReadError;
use super::{bigtable_reader::BigtableReader, checkpoints::CheckpointKey};
use async_graphql::dataloader::DataLoader;
use std::collections::HashMap;
use std::sync::Arc;
use sui_types::{
    crypto::AuthorityQuorumSignInfo,
    messages_checkpoint::{CheckpointContents, CheckpointSummary},
    object::Object,
};

/// A loader for point lookups in kv stores backed by either Bigtable or Postgres.
/// Supported lookups:
/// - Objects by id and version
/// - Checkpoints by sequence number
/// - Transactions by digest
#[derive(Clone)]
pub(crate) enum KVLoader {
    Bigtable(Arc<DataLoader<BigtableReader>>),
    Pg(Arc<DataLoader<PgReader>>),
}

impl KVLoader {
    pub(crate) fn new_with_bigtable(bigtable_reader: BigtableReader) -> Self {
        Self::Bigtable(Arc::new(DataLoader::new(bigtable_reader, tokio::spawn)))
    }

    pub(crate) fn new_with_pg(pg_reader: PgReader) -> Self {
        Self::Pg(Arc::new(DataLoader::new(pg_reader, tokio::spawn)))
    }

    pub(crate) async fn load_one_object(
        &self,
        key: VersionedObjectKey,
    ) -> Result<Option<Object>, Arc<ReadError>> {
        match self {
            Self::Bigtable(loader) => loader.load_one(key).await,
            Self::Pg(loader) => loader
                .load_one(key)
                .await?
                .and_then(|stored| {
                    stored.serialized_object.map(|serialized_object| {
                        Ok(bcs::from_bytes(&serialized_object)
                            .map_err(|e| ReadError::Serde(e.into()))?)
                    })
                })
                .transpose(),
        }
    }

    pub(crate) async fn load_many_objects(
        &self,
        keys: Vec<VersionedObjectKey>,
    ) -> Result<HashMap<VersionedObjectKey, Object>, Arc<ReadError>> {
        match self {
            Self::Bigtable(loader) => loader.load_many(keys).await,
            Self::Pg(loader) => loader
                .load_many(keys)
                .await?
                .into_iter()
                .flat_map(|(key, stored)| {
                    stored.serialized_object.map(|serialized_object| {
                        Ok((
                            key,
                            bcs::from_bytes(&serialized_object)
                                .map_err(|e| ReadError::Serde(e.into()))?,
                        ))
                    })
                })
                .collect(),
        }
    }

    pub(crate) async fn load_one_checkpoint(
        &self,
        key: CheckpointKey,
    ) -> Result<
        Option<(
            CheckpointSummary,
            CheckpointContents,
            AuthorityQuorumSignInfo<true>,
        )>,
        Arc<ReadError>,
    > {
        match self {
            Self::Bigtable(loader) => loader.load_one(key).await,
            Self::Pg(loader) => loader
                .load_one(key)
                .await?
                .map(|stored| {
                    let summary: CheckpointSummary = bcs::from_bytes(&stored.checkpoint_summary)
                        .map_err(|e| ReadError::Serde(e.into()))?;

                    let contents: CheckpointContents = bcs::from_bytes(&stored.checkpoint_contents)
                        .map_err(|e| ReadError::Serde(e.into()))?;

                    let signature: AuthorityQuorumSignInfo<true> =
                        bcs::from_bytes(&stored.validator_signatures)
                            .map_err(|e| ReadError::Serde(e.into()))?;

                    Ok((summary, contents, signature))
                })
                .transpose(),
        }
    }
}
