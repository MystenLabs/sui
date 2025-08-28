// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use anyhow::Context;
use async_graphql::dataloader::Loader;
use diesel::{ExpressionMethods, QueryDsl};
use move_core_types::language_storage::StructTag;
use sui_indexer_alt_schema::{displays::StoredDisplay, schema::sum_displays};

use crate::{error::Error, pg_reader::PgReader};

/// Key for fetching a Display object by the type it corresponds to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DisplayKey(pub StructTag);

#[async_trait::async_trait]
impl Loader<DisplayKey> for PgReader {
    type Value = StoredDisplay;
    type Error = Error;

    async fn load(&self, keys: &[DisplayKey]) -> Result<HashMap<DisplayKey, Self::Value>, Error> {
        use sum_displays::dsl as d;

        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await?;

        let types = keys
            .iter()
            .map(|d| bcs::to_bytes(&d.0))
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to serialize display types")?;

        let displays: Vec<StoredDisplay> = conn
            .results(d::sum_displays.filter(d::object_type.eq_any(types.clone())))
            .await?;

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
