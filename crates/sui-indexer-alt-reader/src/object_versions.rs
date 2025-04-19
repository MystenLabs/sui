// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use anyhow::Context as _;
use async_graphql::dataloader::Loader;
use diesel::sql_types::{Array, BigInt, Bytea};
use sui_indexer_alt_schema::objects::StoredObjVersion;
use sui_types::base_types::ObjectID;

use crate::{error::Error as ReadError, pg_reader::PgReader};

/// Key for fetching the latest version of an object. If the object has been deleted or wrapped,
/// the latest version will return the version it was deleted/wrapped at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LatestObjectVersionKey(pub ObjectID);

/// Key for fetching the latest version of an object, with an inclusive version upperbound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct VersionBoundedObjectVersionKey(pub ObjectID, pub u64);

/// Key for fetching the latest version of an object, as of a given checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct CheckpointBoundedObjectVersionKey(pub ObjectID, pub u64);

#[derive(thiserror::Error, Debug, Clone)]
#[error(transparent)]
pub enum Error {
    Deserialization(#[from] Arc<anyhow::Error>),
    Read(#[from] Arc<ReadError>),
}

#[async_trait::async_trait]
impl Loader<LatestObjectVersionKey> for PgReader {
    type Value = StoredObjVersion;
    type Error = Error;

    async fn load(
        &self,
        keys: &[LatestObjectVersionKey],
    ) -> Result<HashMap<LatestObjectVersionKey, StoredObjVersion>, Self::Error> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await.map_err(Arc::new)?;

        let ids: Vec<_> = keys.iter().map(|k| k.0.into_bytes()).collect();
        let query = diesel::sql_query(
            r#"
                SELECT
                    k.object_id,
                    v.object_version,
                    v.object_digest,
                    v.cp_sequence_number
                FROM (
                    SELECT UNNEST($1) object_id
                ) k
                CROSS JOIN LATERAL (
                    SELECT
                        object_version,
                        object_digest,
                        cp_sequence_number
                    FROM
                        obj_versions
                    WHERE
                        obj_versions.object_id = k.object_id
                    ORDER BY
                        object_version DESC
                    LIMIT
                        1
                ) v
            "#,
        )
        .bind::<Array<Bytea>, _>(ids);

        let obj_versions: Vec<StoredObjVersion> = conn.results(query).await.map_err(Arc::new)?;
        let id_to_stored: HashMap<_, _> = obj_versions
            .into_iter()
            .map(|stored| (stored.object_id.clone(), stored))
            .collect();

        Ok(keys
            .iter()
            .filter_map(|key| {
                let slice: &[u8] = key.0.as_ref();
                Some((*key, id_to_stored.get(slice).cloned()?))
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl Loader<VersionBoundedObjectVersionKey> for PgReader {
    type Value = StoredObjVersion;
    type Error = Error;

    async fn load(
        &self,
        keys: &[VersionBoundedObjectVersionKey],
    ) -> Result<HashMap<VersionBoundedObjectVersionKey, StoredObjVersion>, Self::Error> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await.map_err(Arc::new)?;

        let ids: Vec<_> = keys.iter().map(|k| k.0.into_bytes()).collect();
        let versions: Vec<_> = keys.iter().map(|k| k.1 as i64).collect();
        let query = diesel::sql_query(
            r#"
                SELECT
                    k.object_id,
                    v.object_version,
                    v.object_digest,
                    v.cp_sequence_number
                FROM (
                    SELECT
                        UNNEST($1) object_id,
                        UNNEST($2) object_version
                ) k
                CROSS JOIN LATERAL (
                    SELECT
                        object_version,
                        object_digest,
                        cp_sequence_number
                    FROM
                        obj_versions
                    WHERE
                        obj_versions.object_id = k.object_id
                    AND obj_versions.object_version <= k.object_version
                    ORDER BY
                        object_version DESC
                    LIMIT
                        1
                ) v
            "#,
        )
        .bind::<Array<Bytea>, _>(ids)
        .bind::<Array<BigInt>, _>(versions);

        let obj_versions: Vec<StoredObjVersion> = conn.results(query).await.map_err(Arc::new)?;

        // A single data loader request may contain multiple keys for the same object ID. Store
        // them in an ordered map, so that we can find the latest version for each key.
        let mut key_to_stored = BTreeMap::new();
        for obj_version in obj_versions {
            let id = ObjectID::from_bytes(&obj_version.object_id)
                .context("Failed to deserialize ObjectID")
                .map_err(Arc::new)?;

            let version = obj_version.object_version as u64;

            key_to_stored.insert(VersionBoundedObjectVersionKey(id, version), obj_version);
        }

        Ok(keys
            .iter()
            .filter_map(|key| {
                let (bound, stored) = key_to_stored.range(..=key).last()?;
                (key.0 == bound.0).then(|| (*key, stored.clone()))
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl Loader<CheckpointBoundedObjectVersionKey> for PgReader {
    type Value = StoredObjVersion;
    type Error = Error;

    async fn load(
        &self,
        keys: &[CheckpointBoundedObjectVersionKey],
    ) -> Result<HashMap<CheckpointBoundedObjectVersionKey, StoredObjVersion>, Self::Error> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await.map_err(Arc::new)?;

        let ids: Vec<_> = keys.iter().map(|k| k.0.into_bytes()).collect();
        let cps: Vec<_> = keys.iter().map(|k| k.1 as i64).collect();
        let query = diesel::sql_query(
            r#"
                SELECT
                    k.object_id,
                    v.object_version,
                    v.object_digest,
                    v.cp_sequence_number
                FROM (
                    SELECT
                        UNNEST($1) object_id,
                        UNNEST($2) cp_sequence_number
                ) k
                CROSS JOIN LATERAL (
                    SELECT
                        object_version,
                        object_digest,
                        cp_sequence_number
                    FROM
                        obj_versions
                    WHERE
                        obj_versions.object_id = k.object_id
                    AND obj_versions.cp_sequence_number <= k.cp_sequence_number
                    ORDER BY
                        cp_sequence_number DESC,
                        object_version DESC
                    LIMIT
                        1
                ) v
            "#,
        )
        .bind::<Array<Bytea>, _>(ids)
        .bind::<Array<BigInt>, _>(cps);

        let obj_versions: Vec<StoredObjVersion> = conn.results(query).await.map_err(Arc::new)?;

        // A single data loader request may contain multiple keys for the same object ID. Store
        // them in an ordered map, so that we can find the latest version for each key.
        let mut key_to_stored = BTreeMap::new();
        for obj_version in obj_versions {
            let id = ObjectID::from_bytes(&obj_version.object_id)
                .context("Failed to deserialize ObjectID")
                .map_err(Arc::new)?;

            let cp_sequence_number = obj_version.cp_sequence_number as u64;

            key_to_stored.insert(
                CheckpointBoundedObjectVersionKey(id, cp_sequence_number),
                obj_version,
            );
        }

        Ok(keys
            .iter()
            .filter_map(|key| {
                let (bound, stored) = key_to_stored.range(..=key).last()?;
                (key.0 == bound.0).then(|| (*key, stored.clone()))
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use async_graphql::dataloader::Loader;
    use diesel_async::RunQueryDsl as _;
    use prometheus::Registry;
    use sui_indexer_alt_schema::{schema::obj_versions, MIGRATIONS};
    use sui_pg_db::{temp::TempDb, Db, DbArgs};
    use sui_types::digests::ObjectDigest;
    use tokio_util::sync::CancellationToken;

    use super::*;

    /// Create a temporary database, and return a connection pool that can write to it, and a
    /// reader to it.
    async fn setup() -> (TempDb, Db, PgReader) {
        let registry = Registry::new();
        let temp_db = TempDb::new().unwrap();
        let url = temp_db.database().url();

        let writer = Db::for_write(url.clone(), DbArgs::default()).await.unwrap();
        let reader = PgReader::new(
            None,
            Some(url.clone()),
            DbArgs::default(),
            &registry,
            CancellationToken::new(),
        )
        .await
        .unwrap();

        writer.run_migrations(MIGRATIONS).await.unwrap();
        (temp_db, writer, reader)
    }

    fn stored(id: ObjectID, d: ObjectDigest, v: u64, cp: u64) -> StoredObjVersion {
        StoredObjVersion {
            object_id: id.into_bytes().to_vec(),
            object_version: v as i64,
            object_digest: Some(d.into_inner().to_vec()),
            cp_sequence_number: cp as i64,
        }
    }

    #[tokio::test]
    async fn test_version_bounded() {
        use obj_versions::dsl as v;

        let (_temp_db, writer, reader) = setup().await;

        let o0 = ObjectID::random();
        let d0 = ObjectDigest::random();

        let o1 = ObjectID::random();
        let d1 = ObjectDigest::random();

        let o2 = ObjectID::random();

        {
            // Set-up the table with a couple of records.
            let mut conn = writer.connect().await.unwrap();

            diesel::insert_into(v::obj_versions)
                .values(vec![
                    stored(o0, d0, 1, 1),
                    stored(o1, d1, 2, 1),
                    stored(o0, d0, 2, 2),
                    stored(o0, d0, 4, 2),
                ])
                .execute(&mut conn)
                .await
                .unwrap();
        }

        use VersionBoundedObjectVersionKey as K;

        // Exact match on the first version of the object.
        assert_eq!(
            Loader::load(&reader, &[K(o0, 1)]).await.unwrap(),
            HashMap::from_iter([(K(o0, 1), stored(o0, d0, 1, 1))]),
        );

        // Exact match on the last version of the object.
        assert_eq!(
            Loader::load(&reader, &[K(o0, 4)]).await.unwrap(),
            HashMap::from_iter([(K(o0, 4), stored(o0, d0, 4, 2))]),
        );

        // Inexact match on the middle version of the object.
        assert_eq!(
            Loader::load(&reader, &[K(o0, 3)]).await.unwrap(),
            HashMap::from_iter([(K(o0, 3), stored(o0, d0, 2, 2))]),
        );

        // Inexact match on the last version of the object.
        assert_eq!(
            Loader::load(&reader, &[K(o0, 100)]).await.unwrap(),
            HashMap::from_iter([(K(o0, 100), stored(o0, d0, 4, 2))]),
        );

        // No matching object version.
        assert_eq!(
            Loader::load(&reader, &[K(o1, 1)]).await.unwrap(),
            HashMap::new(),
        );

        // No matching object.
        assert_eq!(
            Loader::load(&reader, &[K(o2, 1)]).await.unwrap(),
            HashMap::new(),
        );

        // Multiple requests that map to the same record.
        assert_eq!(
            Loader::load(&reader, &[K(o0, 2), K(o0, 3)]).await.unwrap(),
            HashMap::from_iter([
                (K(o0, 2), stored(o0, d0, 2, 2)),
                (K(o0, 3), stored(o0, d0, 2, 2))
            ]),
        );

        // Multiple requests, one of them not matching.
        assert_eq!(
            Loader::load(&reader, &[K(o0, 1), K(o1, 1)]).await.unwrap(),
            HashMap::from_iter([(K(o0, 1), stored(o0, d0, 1, 1))]),
        );

        // Same again, but with ObjectIDs swapped.
        assert_eq!(
            Loader::load(&reader, &[K(o0, 0), K(o1, 2)]).await.unwrap(),
            HashMap::from_iter([(K(o1, 2), stored(o1, d1, 2, 1))]),
        );

        // All the requests in one.
        assert_eq!(
            Loader::load(
                &reader,
                &[
                    K(o0, 0),
                    K(o0, 1),
                    K(o0, 2),
                    K(o0, 3),
                    K(o0, 4),
                    K(o0, 5),
                    K(o1, 1),
                    K(o1, 2),
                    K(o2, 1),
                ]
            )
            .await
            .unwrap(),
            HashMap::from_iter([
                (K(o0, 1), stored(o0, d0, 1, 1)),
                (K(o0, 2), stored(o0, d0, 2, 2)),
                (K(o0, 3), stored(o0, d0, 2, 2)),
                (K(o0, 4), stored(o0, d0, 4, 2)),
                (K(o0, 5), stored(o0, d0, 4, 2)),
                (K(o1, 2), stored(o1, d1, 2, 1)),
            ])
        );
    }

    #[tokio::test]
    async fn test_checkpoint_bounded() {
        use obj_versions::dsl as v;

        let (_temp_db, writer, reader) = setup().await;

        let o0 = ObjectID::random();
        let d0 = ObjectDigest::random();

        let o1 = ObjectID::random();
        let d1 = ObjectDigest::random();

        let o2 = ObjectID::random();

        {
            // Set-up the table with a couple of records.
            let mut conn = writer.connect().await.unwrap();

            diesel::insert_into(v::obj_versions)
                .values(vec![
                    stored(o0, d0, 1, 1),
                    stored(o1, d1, 2, 1),
                    stored(o0, d0, 2, 2),
                    stored(o0, d0, 4, 2),
                    stored(o1, d1, 3, 3),
                ])
                .execute(&mut conn)
                .await
                .unwrap();
        }

        use CheckpointBoundedObjectVersionKey as K;

        // Exact match on the first checkpoint including an object.
        assert_eq!(
            Loader::load(&reader, &[K(o0, 1)]).await.unwrap(),
            HashMap::from_iter([(K(o0, 1), stored(o0, d0, 1, 1))]),
        );

        // Exact match on the last checkpoint including an object.
        assert_eq!(
            Loader::load(&reader, &[K(o0, 2)]).await.unwrap(),
            HashMap::from_iter([(K(o0, 2), stored(o0, d0, 4, 2))]),
        );

        // Inexact match on the first checkpoint including an object.
        assert_eq!(
            Loader::load(&reader, &[K(o1, 2)]).await.unwrap(),
            HashMap::from_iter([(K(o1, 2), stored(o1, d1, 2, 1))]),
        );

        // Inexact match on the last checkpoint including an object.
        assert_eq!(
            Loader::load(&reader, &[K(o0, 3)]).await.unwrap(),
            HashMap::from_iter([(K(o0, 3), stored(o0, d0, 4, 2))]),
        );

        // No matching checkpoint.
        assert_eq!(
            Loader::load(&reader, &[K(o1, 0)]).await.unwrap(),
            HashMap::new(),
        );

        // No matching object.
        assert_eq!(
            Loader::load(&reader, &[K(o2, 1)]).await.unwrap(),
            HashMap::new(),
        );

        // Multiple requests that map to the same record.
        assert_eq!(
            Loader::load(&reader, &[K(o0, 2), K(o0, 3)]).await.unwrap(),
            HashMap::from_iter([
                (K(o0, 2), stored(o0, d0, 4, 2)),
                (K(o0, 3), stored(o0, d0, 4, 2)),
            ])
        );

        // Multiple requests, one of them not matching.
        assert_eq!(
            Loader::load(&reader, &[K(o0, 1), K(o1, 0)]).await.unwrap(),
            HashMap::from_iter([(K(o0, 1), stored(o0, d0, 1, 1))])
        );

        // Same again, but with ObjectIDs swapped.
        assert_eq!(
            Loader::load(&reader, &[K(o0, 0), K(o1, 1)]).await.unwrap(),
            HashMap::from_iter([(K(o1, 1), stored(o1, d1, 2, 1))])
        );

        // All the requests in one.
        assert_eq!(
            Loader::load(
                &reader,
                &[
                    K(o0, 0),
                    K(o0, 1),
                    K(o0, 2),
                    K(o0, 3),
                    K(o1, 0),
                    K(o1, 1),
                    K(o1, 2),
                    K(o2, 1),
                ]
            )
            .await
            .unwrap(),
            HashMap::from_iter([
                (K(o0, 1), stored(o0, d0, 1, 1)),
                (K(o0, 2), stored(o0, d0, 4, 2)),
                (K(o0, 3), stored(o0, d0, 4, 2)),
                (K(o1, 1), stored(o1, d1, 2, 1)),
                (K(o1, 2), stored(o1, d1, 2, 1)),
            ])
        );
    }
}
