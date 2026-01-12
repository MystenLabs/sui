// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use async_graphql::dataloader::Loader;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use sui_indexer_alt_schema::cp_sequence_numbers::StoredCpSequenceNumbers;
use sui_indexer_alt_schema::schema::cp_sequence_numbers;

use crate::error::Error;
use crate::pg_reader::PgReader;

/// Key for fetching information about checkpoint sequence numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CpSequenceNumberKey(pub u64);

#[async_trait::async_trait]
impl Loader<CpSequenceNumberKey> for PgReader {
    type Value = StoredCpSequenceNumbers;
    type Error = Error;

    async fn load(
        &self,
        keys: &[CpSequenceNumberKey],
    ) -> Result<HashMap<CpSequenceNumberKey, Self::Value>, Error> {
        use cp_sequence_numbers::dsl as c;

        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await?;

        let ids: Vec<_> = keys.iter().map(|e| e.0 as i64).collect();
        let epochs: Vec<StoredCpSequenceNumbers> = conn
            .results(c::cp_sequence_numbers.filter(c::cp_sequence_number.eq_any(ids)))
            .await?;

        Ok(epochs
            .into_iter()
            .map(|c| (CpSequenceNumberKey(c.cp_sequence_number as u64), c))
            .collect())
    }
}
