// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc};

use async_graphql::dataloader::Loader;
use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl};
use sui_indexer_alt_schema::{objects::StoredObject, schema::kv_objects};
use sui_types::{base_types::ObjectID, object::Object, storage::ObjectKey};

use crate::{bigtable_reader::BigtableReader, error::Error, pg_reader::PgReader};

/// Key for fetching the contents a particular version of an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VersionedObjectKey(pub ObjectID, pub u64);

#[async_trait::async_trait]
impl Loader<VersionedObjectKey> for PgReader {
    type Value = StoredObject;
    type Error = Arc<Error>;

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

#[async_trait::async_trait]
impl Loader<VersionedObjectKey> for BigtableReader {
    type Value = Object;
    type Error = Arc<Error>;

    async fn load(
        &self,
        keys: &[VersionedObjectKey],
    ) -> Result<HashMap<VersionedObjectKey, Object>, Self::Error> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let object_keys: Vec<ObjectKey> = keys
            .iter()
            .map(|key| ObjectKey(key.0, key.1.into()))
            .collect();

        Ok(self
            .objects(&object_keys)
            .await?
            .into_iter()
            .map(|o| (VersionedObjectKey(o.id(), o.version().into()), o))
            .collect())
    }
}
