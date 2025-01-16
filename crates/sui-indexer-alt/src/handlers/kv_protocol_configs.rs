// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{Context, Result};
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::pipeline::{concurrent::Handler, Processor};
use sui_indexer_alt_schema::{
    checkpoints::StoredGenesis, epochs::StoredProtocolConfig, schema::kv_protocol_configs,
};
use sui_pg_db as db;
use sui_protocol_config::ProtocolConfig;
use sui_types::full_checkpoint_content::CheckpointData;

pub(crate) struct KvProtocolConfigs(pub(crate) StoredGenesis);

impl Processor for KvProtocolConfigs {
    const NAME: &'static str = "kv_protocol_configs";
    type Value = StoredProtocolConfig;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let CheckpointData {
            checkpoint_summary, ..
        } = checkpoint.as_ref();

        let protocol_version = if checkpoint_summary.sequence_number == 0 {
            self.0.initial_protocol_version()
        } else if let Some(end_of_epoch) = checkpoint_summary.end_of_epoch_data.as_ref() {
            end_of_epoch.next_epoch_protocol_version
        } else {
            return Ok(vec![]);
        };

        let protocol_config = ProtocolConfig::get_for_version(
            protocol_version,
            self.0.chain().context("Failed to identify chain")?,
        );

        let protocol_version = protocol_version.as_u64() as i64;
        Ok(protocol_config
            .attr_map()
            .into_iter()
            .map(|(config_name, value)| StoredProtocolConfig {
                protocol_version,
                config_name,
                config_value: value.map(|v| v.to_string()),
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl Handler for KvProtocolConfigs {
    const MIN_EAGER_ROWS: usize = 1;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        Ok(diesel::insert_into(kv_protocol_configs::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
