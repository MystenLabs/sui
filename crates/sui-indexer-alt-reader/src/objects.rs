// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use anyhow::Context;
use async_graphql::dataloader::Loader;
use diesel::BoolExpressionMethods;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use prost_types::FieldMask;
use sui_indexer_alt_schema::objects::StoredObject;
use sui_indexer_alt_schema::schema::kv_objects;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_types::base_types::ObjectID;
use sui_types::object::Object;

use crate::error::Error;
use crate::ledger_grpc_reader::ChunkedLoader;
use crate::ledger_grpc_reader::LedgerGrpcReader;
use crate::ledger_grpc_reader::MAX_BATCH_GET_OBJECTS;
use crate::pg_reader::PgReader;

/// Key for fetching the contents a particular version of an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VersionedObjectKey(pub ObjectID, pub u64);

#[async_trait::async_trait]
impl Loader<VersionedObjectKey> for PgReader {
    type Value = StoredObject;
    type Error = Error;

    async fn load(
        &self,
        keys: &[VersionedObjectKey],
    ) -> Result<HashMap<VersionedObjectKey, StoredObject>, Error> {
        use kv_objects::dsl as o;

        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await?;

        let mut query = o::kv_objects.into_boxed();

        for VersionedObjectKey(id, version) in keys {
            query = query.or_filter(
                o::object_id
                    .eq(id.into_bytes())
                    .and(o::object_version.eq(*version as i64)),
            );
        }

        let objects: Vec<StoredObject> = conn.results(query).await?;

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
impl ChunkedLoader<VersionedObjectKey> for LedgerGrpcReader {
    type Value = Object;
    type Error = Error;

    fn chunk_size(&self) -> usize {
        MAX_BATCH_GET_OBJECTS
    }

    async fn load_chunk(
        &self,
        keys: &[VersionedObjectKey],
    ) -> Result<HashMap<VersionedObjectKey, Object>, Error> {
        let requests = keys
            .iter()
            .map(|key| {
                let mut req = proto::GetObjectRequest::new(&key.0.into());
                req.version = Some(key.1);
                req
            })
            .collect();

        let mut request = proto::BatchGetObjectsRequest::default();
        request.requests = requests;
        request.read_mask = Some(FieldMask::from_paths(["bcs"]));

        let batch_response = self.batch_get_objects(request).await?;

        let mut results = HashMap::new();
        for obj_result in batch_response.objects {
            if let Some(proto::get_object_result::Result::Object(object)) = obj_result.result {
                let obj: Object = object
                    .bcs
                    .as_ref()
                    .context("Missing bcs in object")?
                    .deserialize()
                    .context("Failed to deserialize object")?;
                results.insert(VersionedObjectKey(obj.id(), obj.version().into()), obj);
            }
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use sui_sdk_types::Address;
    use sui_types::base_types::ObjectID;

    use super::*;
    use crate::ledger_grpc_reader::test_support::mock_reader;

    #[tokio::test]
    async fn load_chunks_oversized_batches() {
        let (reader, mock, server) = mock_reader().await;
        let limit = MAX_BATCH_GET_OBJECTS;

        let keys: Vec<VersionedObjectKey> = (0..limit + 50)
            .map(|i| VersionedObjectKey(ObjectID::random(), i as u64))
            .collect();

        let result = reader.load(&keys).await.expect("load should succeed");
        assert!(result.is_empty());

        let batches = mock.object_batches();
        assert_eq!(batches.len(), 2);
        assert!(batches.iter().all(|batch| batch.len() <= limit));

        let mut requested: Vec<(String, Option<u64>)> = batches
            .into_iter()
            .flatten()
            .map(|req| (req.object_id.unwrap_or_default(), req.version))
            .collect();
        requested.sort();
        let mut expected: Vec<(String, Option<u64>)> = keys
            .iter()
            .map(|key| {
                let address: Address = key.0.into();
                (address.to_string(), Some(key.1))
            })
            .collect();
        expected.sort();
        assert_eq!(requested, expected);

        server.abort();
    }
}
