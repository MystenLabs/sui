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
    checkpoints::StoredGenesis, epochs::StoredFeatureFlag, schema::kv_feature_flags,
};
use sui_protocol_config::ProtocolConfig;

pub(crate) struct KvFeatureFlags(pub(crate) StoredGenesis);

#[async_trait]
impl Processor for KvFeatureFlags {
    const NAME: &'static str = "kv_feature_flags";
    type Value = StoredFeatureFlag;

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

#[async_trait]
impl Handler for KvFeatureFlags {
    const MIN_EAGER_ROWS: usize = 1;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit<'a>(values: &[Self::Value], conn: &mut Connection<'a>) -> Result<usize> {
        Ok(diesel::insert_into(kv_feature_flags::table)
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
    async fn test_feature_flag_processing() {
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

        let feature_flags = KvFeatureFlags(stored_genesis)
            .process(&checkpoint)
            .await
            .unwrap();

        assert!(!feature_flags.is_empty());
        for flag in feature_flags {
            assert_eq!(flag.protocol_version, ProtocolVersion::MIN.as_u64() as i64);
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

        KvFeatureFlags(stored_genesis)
            .process(&checkpoint)
            .await
            .unwrap_err();
    }
}
