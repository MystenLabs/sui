// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use async_graphql::dataloader::Loader;
use diesel::{ExpressionMethods, QueryDsl};
use sui_indexer_alt_schema::{objects::StoredObjInfo, schema::obj_info};
use sui_types::base_types::ObjectID;

use super::reader::{ReadError, Reader};

/// Key for fetching the latest object info record for an object. This record corresponds to the
/// last time the object's ownership information changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct LatestObjectInfoKey(pub ObjectID);

#[async_trait::async_trait]
impl Loader<LatestObjectInfoKey> for Reader {
    type Value = StoredObjInfo;
    type Error = Arc<ReadError>;

    async fn load(
        &self,
        keys: &[LatestObjectInfoKey],
    ) -> Result<HashMap<LatestObjectInfoKey, StoredObjInfo>, Self::Error> {
        use obj_info::dsl as i;

        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await.map_err(Arc::new)?;

        let ids: BTreeSet<_> = keys.iter().map(|k| k.0.into_bytes()).collect();
        let obj_info: Vec<StoredObjInfo> = conn
            .results(
                i::obj_info
                    .filter(i::object_id.eq_any(ids))
                    .distinct_on(i::object_id)
                    .order((i::object_id, i::cp_sequence_number.desc())),
            )
            .await
            .map_err(Arc::new)?;

        let id_to_stored: HashMap<_, _> = obj_info
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
