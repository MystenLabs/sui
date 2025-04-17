// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc};

use async_graphql::dataloader::Loader;
use diesel::{ExpressionMethods, QueryDsl};
use move_core_types::language_storage::StructTag;
use sui_indexer_alt_schema::{displays::StoredDisplay, schema::sum_displays};

use super::{error::Error as ReadError, pg_reader::PgReader};

/// Key for fetching a Display object by the type it corresponds to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct DisplayKey(pub StructTag);

#[derive(thiserror::Error, Debug, Clone)]
#[error(transparent)]
pub(crate) enum Error {
    Bcs(#[from] bcs::Error),
    Read(#[from] Arc<ReadError>),
}

#[async_trait::async_trait]
impl Loader<DisplayKey> for PgReader {
    type Value = StoredDisplay;
    type Error = Error;

    async fn load(
        &self,
        keys: &[DisplayKey],
    ) -> Result<HashMap<DisplayKey, Self::Value>, Self::Error> {
        use sum_displays::dsl as d;

        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await.map_err(Arc::new)?;

        let types = keys
            .iter()
            .map(|d| bcs::to_bytes(&d.0))
            .collect::<Result<Vec<_>, _>>()?;

        let displays: Vec<StoredDisplay> = conn
            .results(d::sum_displays.filter(d::object_type.eq_any(types.clone())))
            .await
            .map_err(Arc::new)?;

        let raw_type_to_stored: HashMap<_, _> = displays
            .into_iter()
            .map(|d| (d.object_type.clone(), d))
            .collect();

        Ok(keys
            .iter()
            .zip(types)
            .filter_map(|(k, t)| Some((k.clone(), raw_type_to_stored.get(&t).cloned()?)))
            .collect())
    }
}
