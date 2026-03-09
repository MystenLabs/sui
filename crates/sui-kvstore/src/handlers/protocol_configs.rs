// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use anyhow::bail;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_protocol_config::Chain;
use sui_protocol_config::ProtocolConfig;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::bigtable::proto::bigtable::v2::mutate_rows_request::Entry;
use crate::handlers::BigTableProcessor;
use crate::tables;

/// Pipeline that writes protocol config attributes and feature flags to BigTable.
/// Holds the chain identifier needed to resolve version-specific configs.
pub struct ProtocolConfigsPipeline(pub Chain);

#[async_trait::async_trait]
impl Processor for ProtocolConfigsPipeline {
    const NAME: &'static str = "kvstore_protocol_configs";
    type Value = Entry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        let protocol_version = if checkpoint.summary.sequence_number == 0 {
            checkpoint
                .epoch_info()?
                .context("missing epoch_info at genesis")?
                .protocol_version
                .context("missing protocol_version at genesis")?
                .into()
        } else if let Some(end_of_epoch) = checkpoint.summary.end_of_epoch_data.as_ref() {
            end_of_epoch.next_epoch_protocol_version
        } else {
            return Ok(vec![]);
        };

        let Some(protocol_config) =
            ProtocolConfig::get_for_version_if_supported(protocol_version, self.0)
        else {
            bail!(
                "Protocol version {} is not supported",
                protocol_version.as_u64()
            );
        };

        let configs = protocol_config
            .attr_map()
            .into_iter()
            .map(|(k, v)| (k, v.map(|v| v.to_string())))
            .collect();
        let flags = protocol_config.feature_map();

        let entry = tables::make_entry(
            tables::protocol_configs::encode_key(protocol_version.as_u64()),
            tables::protocol_configs::encode(&configs, &flags)?,
            None,
        );

        Ok(vec![entry])
    }
}

impl BigTableProcessor for ProtocolConfigsPipeline {
    const TABLE: &'static str = tables::protocol_configs::NAME;
    const MIN_EAGER_ROWS: usize = 1;
}
