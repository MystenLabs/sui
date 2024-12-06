// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{Context, Result};
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::pipeline::{concurrent::Handler, Processor};
use sui_indexer_alt_schema::{
    checkpoints::StoredGenesis, epochs::StoredFeatureFlag, schema::kv_feature_flags,
};
use sui_pg_db as db;
use sui_protocol_config::ProtocolConfig;
use sui_types::full_checkpoint_content::CheckpointData;

pub(crate) struct KvFeatureFlags(pub(crate) StoredGenesis);

impl Processor for KvFeatureFlags {
    const NAME: &'static str = "kv_feature_flags";
    type Value = StoredFeatureFlag;

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
            .feature_map()
            .into_iter()
            .map(|(flag_name, flag_value)| StoredFeatureFlag {
                protocol_version,
                flag_name,
                flag_value,
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl Handler for KvFeatureFlags {
    const MIN_EAGER_ROWS: usize = 1;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        Ok(diesel::insert_into(kv_feature_flags::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
