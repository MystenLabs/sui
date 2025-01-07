// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use diesel::sql_query;
use diesel_async::RunQueryDsl;
use sui_pg_db::Connection;

use super::{PruningLookupTable, PruningStrategyTrait};

pub struct PerObjectPruning {
    pub pruning_lookup_table: Arc<PruningLookupTable>,
    pub table_name: String,
    pub object_id_column_name: String,
    pub cp_sequence_number_column_name: String,
}

#[async_trait::async_trait]
impl PruningStrategyTrait for PerObjectPruning {
    fn requires_processed_values(&self) -> bool {
        true
    }

    async fn prune(
        &self,
        from: u64,
        to_exclusive: u64,
        conn: &mut Connection,
    ) -> anyhow::Result<usize> {
        let to_prune = self.pruning_lookup_table.take(from, to_exclusive)?;

        // For each (object_id, cp_sequence_number_exclusive), delete all entries in obj_info with
        // cp_sequence_number less than cp_sequence_number_exclusive that match the object_id.

        let values = to_prune
            .iter()
            .map(|(object_id, seq_number)| {
                let object_id_hex = hex::encode(object_id);
                format!("('\\x{}'::BYTEA, {}::BIGINT)", object_id_hex, seq_number)
            })
            .collect::<Vec<_>>()
            .join(",");
        let query = format!(
            "
            WITH to_prune_data (object_id, cp_sequence_number_exclusive) AS (
                VALUES {values}
            )
            DELETE FROM {table_name}
            USING to_prune_data
            WHERE {table_name}.{object_id_column_name} = to_prune_data.object_id
              AND {table_name}.{cp_sequence_number_column_name} < to_prune_data.cp_sequence_number_exclusive
            ",
            values = values,
            table_name = self.table_name,
            object_id_column_name = self.object_id_column_name,
            cp_sequence_number_column_name = self.cp_sequence_number_column_name,
        );
        let rows_deleted = sql_query(query).execute(conn).await?;
        Ok(rows_deleted)
    }
}
