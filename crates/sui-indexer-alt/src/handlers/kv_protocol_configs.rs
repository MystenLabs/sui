// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    pipeline::Processor,
    postgres::{Connection, handler::Handler},
    types::full_checkpoint_content::Checkpoint,
};
use sui_indexer_alt_schema::{
    checkpoints::StoredGenesis, epochs::StoredProtocolConfig, schema::kv_protocol_configs,
};
use sui_protocol_config::ProtocolConfig;

pub(crate) struct KvProtocolConfigs(pub(crate) StoredGenesis);

#[async_trait]
impl Processor for KvProtocolConfigs {
    const NAME: &'static str = "kv_protocol_configs";
    type Value = StoredProtocolConfig;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let Checkpoint { summary, .. } = checkpoint.as_ref();

        let protocol_version = if summary.sequence_number == 0 {
            self.0.initial_protocol_version()
        } else if let Some(end_of_epoch) = summary.end_of_epoch_data.as_ref() {
            end_of_epoch.next_epoch_protocol_version
        } else {
            return Ok(vec![]);
        };

        let Some(protocol_config) = ProtocolConfig::get_for_version_if_supported(
            protocol_version,
            self.0.chain().context("Failed to identify chain")?,
        ) else {
            bail!(
                "Protocol version {} is not supported",
                protocol_version.as_u64()
            );
        };

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

#[async_trait]
impl Handler for KvProtocolConfigs {
    const MIN_EAGER_ROWS: usize = 1;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit<'a>(values: &[Self::Value], conn: &mut Connection<'a>) -> Result<usize> {
        Ok(diesel::insert_into(kv_protocol_configs::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use sui_indexer_alt_framework::types::test_checkpoint_data_builder::{
        AdvanceEpochConfig, TestCheckpointBuilder,
    };
    use sui_protocol_config::ProtocolVersion;

    use super::*;

    #[tokio::test]
    async fn test_protocol_version_processing() {
        let mut builder = TestCheckpointBuilder::new(0);
        let genesis: Arc<Checkpoint> = Arc::new(builder.build_checkpoint());
        let checkpoint: Arc<Checkpoint> = Arc::new(builder.advance_epoch(AdvanceEpochConfig {
            protocol_version: ProtocolVersion::MIN,
            ..Default::default()
        }));

        let stored_genesis = StoredGenesis {
            genesis_digest: genesis.summary.digest().inner().to_vec(),
            initial_protocol_version: ProtocolVersion::MIN.as_u64() as i64,
        };

        let protocol_configs = KvProtocolConfigs(stored_genesis)
            .process(&checkpoint)
            .await
            .unwrap();

        assert!(!protocol_configs.is_empty());
        for config in protocol_configs {
            assert_eq!(
                config.protocol_version,
                ProtocolVersion::MIN.as_u64() as i64
            );
        }
    }

    /// When the protocol version is too high, the pipeline should fail to process the checkpoint,
    /// but not panic.
    #[tokio::test]
    async fn test_protocol_version_too_high() {
        let mut builder = TestCheckpointBuilder::new(0);
        let genesis: Arc<Checkpoint> = Arc::new(builder.build_checkpoint());
        let checkpoint: Arc<Checkpoint> = Arc::new(builder.advance_epoch(AdvanceEpochConfig {
            protocol_version: ProtocolVersion::MAX + 1,
            ..Default::default()
        }));

        let stored_genesis = StoredGenesis {
            genesis_digest: genesis.summary.digest().inner().to_vec(),
            initial_protocol_version: ProtocolVersion::MIN.as_u64() as i64,
        };

        KvProtocolConfigs(stored_genesis)
            .process(&checkpoint)
            .await
            .unwrap_err();
    }
}
