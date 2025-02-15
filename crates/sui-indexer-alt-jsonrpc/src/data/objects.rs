// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc};

use anyhow::Context as _;
use async_graphql::dataloader::{DataLoader, Loader};
use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl};
use sui_indexer_alt_schema::{objects::StoredObject, schema::kv_objects};
use sui_types::base_types::ObjectID;

use super::{
    object_versions::LatestObjectVersionKey,
    reader::{ReadError, Reader},
};

/// Key for fetching the contents a particular version of an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct VersionedObjectKey(pub ObjectID, pub u64);

#[async_trait::async_trait]
impl Loader<VersionedObjectKey> for Reader {
    type Value = StoredObject;
    type Error = Arc<ReadError>;

    async fn load(
        &self,
        keys: &[VersionedObjectKey],
    ) -> Result<HashMap<VersionedObjectKey, StoredObject>, Self::Error> {
        use kv_objects::dsl as o;

        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await.map_err(Arc::new)?;

        let mut query = o::kv_objects.into_boxed();

        for VersionedObjectKey(id, version) in keys {
            query = query.or_filter(
                o::object_id
                    .eq(id.into_bytes())
                    .and(o::object_version.eq(*version as i64)),
            );
        }

        let objects: Vec<StoredObject> = conn.results(query).await.map_err(Arc::new)?;

        let key_to_stored: HashMap<_, _> = objects
            .iter()
            .map(|stored| {
                let id = &stored.object_id[..];
                let version = stored.object_version as u64;
                ((id, version), stored)
            })
            .collect();

        Ok(keys
            .iter()
            .filter_map(|key| {
                let slice: &[u8] = key.0.as_ref();
                let stored = *key_to_stored.get(&(slice, key.1))?;
                Some((*key, stored.clone()))
            })
            .collect())
    }
}

/// Load the contents of the latest version of an object, if it exists. This function does not
/// respect deletion and wrapping. If an object is deleted or wrapped, it may return the contents
/// of the object before the deletion or wrapping, or it may return `None` if the object has been
/// fully pruned from the versions table.
pub(crate) async fn load_latest(
    loader: &DataLoader<Reader>,
    object_id: ObjectID,
) -> Result<Option<StoredObject>, anyhow::Error> {
    let Some(latest_version) = loader
        .load_one(LatestObjectVersionKey(object_id))
        .await
        .context("Failed to load latest version")?
    else {
        return Ok(None);
    };

    let stored = loader
        .load_one(VersionedObjectKey(
            object_id,
            latest_version.object_version as u64,
        ))
        .await
        .context("Failed to load latest object")?;

    Ok(stored)
}
