// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeSet, HashMap};

use async_graphql::dataloader::Loader;
use diesel::{ExpressionMethods, QueryDsl};
use sui_indexer_alt_schema::{cp_blooms::StoredCpBlooms, schema::cp_blooms};

use crate::{error::Error, pg_reader::PgReader};

/// Key for fetching a checkpoint's content by its sequence number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CpBloomsKey(pub u64);

#[async_trait::async_trait]
impl Loader<CpBloomsKey> for PgReader {
    type Value = StoredCpBlooms;
    type Error = Error;

    async fn load(&self, keys: &[CpBloomsKey]) -> Result<HashMap<CpBloomsKey, Self::Value>, Error> {
        use cp_blooms::dsl as c;

        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await?;

        let seqs: BTreeSet<_> = keys.iter().map(|d| d.0 as i64).collect();
        let cp_blooms: Vec<StoredCpBlooms> = conn
            .results(c::cp_blooms.filter(c::cp_sequence_number.eq_any(seqs)))
            .await?;

        Ok(cp_blooms
            .into_iter()
            .map(|c| (CpBloomsKey(c.cp_sequence_number as u64), c))
            .collect())
    }
}
