// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use async_graphql::dataloader::Loader;
use diesel::{ExpressionMethods, QueryDsl};
use sui_indexer_alt_schema::{objects::StoredObjVersion, schema::obj_versions};
use sui_types::base_types::ObjectID;

use super::reader::{ReadError, Reader};

/// Key for fetching the latest version of an object, not accounting for deletions or wraps. If the
/// object has been deleted or wrapped, the version before the delete/wrap is returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct LatestObjectVersionKey(pub ObjectID);

#[async_trait::async_trait]
impl Loader<LatestObjectVersionKey> for Reader {
    type Value = StoredObjVersion;
    type Error = Arc<ReadError>;

    async fn load(
        &self,
        keys: &[LatestObjectVersionKey],
    ) -> Result<HashMap<LatestObjectVersionKey, StoredObjVersion>, Self::Error> {
        use obj_versions::dsl as v;

        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await.map_err(Arc::new)?;

        let ids: BTreeSet<_> = keys.iter().map(|k| k.0.into_bytes()).collect();
        let obj_versions: Vec<StoredObjVersion> = conn
            .results(
                v::obj_versions
                    .filter(v::object_id.eq_any(ids))
                    .distinct_on(v::object_id)
                    .order((v::object_id, v::object_version.desc())),
            )
            .await
            .map_err(Arc::new)?;

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
