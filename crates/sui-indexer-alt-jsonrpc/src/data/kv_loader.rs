// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::error::Error;
use super::objects::VersionedObjectKey;
use super::pg_reader::PgReader;
use super::{bigtable_reader::BigtableReader, checkpoints::CheckpointKey};
use async_graphql::dataloader::DataLoader;
use std::collections::HashMap;
use std::sync::Arc;
use sui_types::base_types::ObjectID;
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
pub(crate) enum KvLoader {
    Bigtable(Arc<DataLoader<BigtableReader>>),
    Pg(Arc<DataLoader<PgReader>>),
}

impl KvLoader {
    pub(crate) fn new_with_bigtable(bigtable_loader: Arc<DataLoader<BigtableReader>>) -> Self {
        Self::Bigtable(bigtable_loader)
    }

    pub(crate) fn new_with_pg(pg_loader: Arc<DataLoader<PgReader>>) -> Self {
        Self::Pg(pg_loader)
    }

    pub(crate) async fn load_one_object(
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

    pub(crate) async fn load_many_objects(
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

    pub(crate) async fn load_one_checkpoint(
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
}
