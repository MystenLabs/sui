// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc};

use async_graphql::dataloader::Loader;
use sui_indexer_alt_schema::objects::StoredObject;
use sui_types::{base_types::ObjectID, base_types::SequenceNumber};

use super::reader::{ReadError, Reader};

/// Load objects by key (object_id, version).
#[async_trait::async_trait]
impl Loader<(ObjectID, SequenceNumber)> for Reader {
    type Value = StoredObject;
    type Error = Arc<ReadError>;

    async fn load(
        &self,
        keys: &[(ObjectID, SequenceNumber)],
    ) -> Result<HashMap<(ObjectID, SequenceNumber), Self::Value>, Self::Error> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let conditions = keys
            .iter()
            .map(|key| {
                format!(
                    "(object_id = '\\x{}'::bytea AND object_version = {})",
                    hex::encode(key.0.to_vec()),
                    key.1.value()
                )
            })
            .collect::<Vec<_>>();
        let query = format!("SELECT * FROM kv_objects WHERE {}", conditions.join(" OR "));

        let mut conn = self.connect().await.map_err(Arc::new)?;
        let objects: Vec<StoredObject> = conn.raw_query(&query).await.map_err(Arc::new)?;

        let results: HashMap<_, _> = objects
            .into_iter()
            .map(|stored| {
                (
                    (
                        ObjectID::from_bytes(&stored.object_id).unwrap(),
                        SequenceNumber::from_u64(stored.object_version as u64),
                    ),
                    stored,
                )
            })
            .collect();

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use diesel_async::RunQueryDsl;
    use sui_indexer_alt_schema::{objects::StoredObject, schema::kv_objects};
    use sui_types::{
        base_types::{ObjectID, SequenceNumber, SuiAddress},
        object::{Object, Owner},
    };

    use crate::test_env::IndexerReaderTestEnv;

    async fn insert_objects(
        test_env: &IndexerReaderTestEnv,
        id_versions: impl IntoIterator<Item = (ObjectID, SequenceNumber)>,
    ) {
        let mut conn = test_env.indexer.db().connect().await.unwrap();
        let stored_objects = id_versions
            .into_iter()
            .map(|(id, version)| {
                let object = Object::with_id_owner_version_for_testing(
                    id,
                    version,
                    Owner::AddressOwner(SuiAddress::ZERO),
                );
                let serialized_object = bcs::to_bytes(&object).unwrap();
                StoredObject {
                    object_id: id.to_vec(),
                    object_version: version.value() as i64,
                    serialized_object: Some(serialized_object),
                }
            })
            .collect::<Vec<_>>();
        diesel::insert_into(kv_objects::table)
            .values(stored_objects)
            .execute(&mut conn)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_load_single_object() {
        let test_env = IndexerReaderTestEnv::new().await;
        let id_version = (ObjectID::ZERO, SequenceNumber::from_u64(1));
        insert_objects(&test_env, vec![id_version]).await;
        let object = test_env
            .loader()
            .load_one(id_version)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(object.object_id, id_version.0.to_vec());
        assert_eq!(object.object_version, id_version.1.value() as i64);
    }

    #[tokio::test]
    async fn test_load_multiple_objects() {
        let test_env = IndexerReaderTestEnv::new().await;
        let mut id_versions = vec![
            (ObjectID::ZERO, SequenceNumber::from_u64(1)),
            (ObjectID::ZERO, SequenceNumber::from_u64(2)),
            (ObjectID::ZERO, SequenceNumber::from_u64(10)),
            (ObjectID::from_single_byte(1), SequenceNumber::from_u64(1)),
            (ObjectID::from_single_byte(1), SequenceNumber::from_u64(2)),
            (ObjectID::from_single_byte(1), SequenceNumber::from_u64(10)),
        ];
        insert_objects(&test_env, id_versions.clone()).await;

        let objects = test_env
            .loader()
            .load_many(id_versions.clone())
            .await
            .unwrap();
        assert_eq!(objects.len(), id_versions.len());
        for (id, version) in &id_versions {
            let object = objects.get(&(*id, *version)).unwrap();
            assert_eq!(object.object_id, id.to_vec());
            assert_eq!(object.object_version, version.value() as i64);
        }

        // Add a ID/version that doesn't exist in the table.
        // Query will still succeed, but will return the same set of objects as before.
        id_versions.push((ObjectID::from_single_byte(2), SequenceNumber::from_u64(1)));
        let objects = test_env
            .loader()
            .load_many(id_versions.clone())
            .await
            .unwrap();
        assert_eq!(objects.len(), id_versions.len() - 1);
    }
}
