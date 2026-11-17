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

/// Pipeline that writes the protocol-config attributes and feature flags of each protocol version
/// the chain runs to the `protocol_configs_v2` table, in separate columns: the genesis version at
/// checkpoint 0, and the next epoch's version at each end-of-epoch checkpoint.
/// Holds the chain identifier needed to resolve version-specific configs.
pub struct ProtocolConfigsV2Pipeline(pub Chain);

#[async_trait::async_trait]
impl Processor for ProtocolConfigsV2Pipeline {
    const NAME: &'static str = "kvstore_protocol_configs_v2";
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

        // Unset attributes render as `NullValue` and are kept, so readers see a stable keyset.
        let attributes = protocol_config
            .render::<prost_types::Value>(&mut mysten_common::rpc_format::Unmetered)
            .context("rendering protocol config to prost Value")?;

        let entry = tables::make_entry(
            tables::protocol_configs_v2::encode_key(protocol_version.as_u64()),
            tables::protocol_configs_v2::encode(attributes, protocol_config.feature_map()),
            None,
        );

        Ok(vec![entry])
    }
}

impl BigTableProcessor for ProtocolConfigsV2Pipeline {
    const TABLE: &'static str = tables::protocol_configs_v2::NAME;
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use prost_types::value::Kind;
    use sui_protocol_config::ProtocolVersion;
    use sui_types::test_checkpoint_data_builder::AdvanceEpochConfig;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;
    use crate::ProtocolConfigV2Data;
    use crate::bigtable::proto::bigtable::v2::mutation;

    /// A boolean attribute, unset at `ProtocolVersion::MIN` and set at `ProtocolVersion::MAX`.
    const BOOL_ATTRIBUTE: &str = "bridge_should_try_to_finalize_committee";

    /// Decode every entry into its protocol version and row.
    fn decode_entries(entries: Vec<Entry>) -> Vec<(u64, ProtocolConfigV2Data)> {
        entries
            .into_iter()
            .map(|entry| {
                let key: [u8; 8] = entry.row_key.as_ref().try_into().unwrap();
                let cells: Vec<(Bytes, Bytes)> = entry
                    .mutations
                    .into_iter()
                    .map(|m| match m.mutation {
                        Some(mutation::Mutation::SetCell(cell)) => {
                            (cell.column_qualifier, cell.value)
                        }
                        other => panic!("unexpected mutation {other:?}"),
                    })
                    .collect();
                let row = tables::protocol_configs_v2::decode(&cells).unwrap();
                (u64::from_be_bytes(key), row)
            })
            .collect()
    }

    /// The single row the end-of-epoch checkpoint moving to `protocol_version` writes.
    async fn end_of_epoch_row(protocol_version: ProtocolVersion) -> ProtocolConfigV2Data {
        let checkpoint = Arc::new(TestCheckpointBuilder::new(1).advance_epoch(
            AdvanceEpochConfig {
                protocol_version,
                ..Default::default()
            },
        ));
        let entries = ProtocolConfigsV2Pipeline(Chain::Unknown)
            .process(&checkpoint)
            .await
            .unwrap();
        let mut rows = decode_entries(entries);
        assert_eq!(rows.len(), 1, "one row per end-of-epoch checkpoint");
        let (version, row) = rows.pop().unwrap();
        assert_eq!(version, protocol_version.as_u64());
        row
    }

    /// Feature flags land in the feature flags column and attributes in the attributes column,
    /// with no name in both.
    #[tokio::test]
    async fn splits_feature_flags_from_attributes() {
        let row = end_of_epoch_row(ProtocolVersion::MAX).await;
        let config = ProtocolConfig::get_for_version(ProtocolVersion::MAX, Chain::Unknown);

        assert_eq!(row.feature_flags, config.feature_map());
        assert!(row.feature_flags.values().any(|enabled| *enabled));
        assert_eq!(
            row.attributes,
            config
                .render::<prost_types::Value>(&mut mysten_common::rpc_format::Unmetered)
                .unwrap(),
        );
        for name in row.feature_flags.keys() {
            assert!(
                !row.attributes.contains_key(name),
                "feature flag `{name}` is in the attributes column",
            );
        }
    }

    /// A boolean attribute stays in the attributes column rather than being taken for a flag.
    #[tokio::test]
    async fn keeps_boolean_attribute_in_attributes() {
        let row = end_of_epoch_row(ProtocolVersion::MAX).await;
        assert!(matches!(
            row.attributes[BOOL_ATTRIBUTE].kind,
            Some(Kind::BoolValue(_))
        ));
        assert!(!row.feature_flags.contains_key(BOOL_ATTRIBUTE));
    }

    /// Attributes unset at a version are kept as `NullValue`, so every version has the same
    /// attribute names.
    #[tokio::test]
    async fn keeps_unset_attributes_as_null() {
        let oldest = end_of_epoch_row(ProtocolVersion::MIN).await;
        let newest = end_of_epoch_row(ProtocolVersion::MAX).await;
        assert!(matches!(
            oldest.attributes[BOOL_ATTRIBUTE].kind,
            Some(Kind::NullValue(_))
        ));
        assert!(
            oldest.attributes.keys().eq(newest.attributes.keys()),
            "attribute names differ between protocol versions",
        );
    }

    /// Checkpoints that neither start the chain nor end an epoch write no row.
    #[tokio::test]
    async fn skips_checkpoints_inside_an_epoch() {
        let mut builder = TestCheckpointBuilder::new(1);
        for object in 0..3 {
            builder = builder
                .start_transaction(0)
                .create_owned_object(object)
                .finish_transaction();
            let checkpoint = Arc::new(builder.build_checkpoint());
            let entries = ProtocolConfigsV2Pipeline(Chain::Unknown)
                .process(&checkpoint)
                .await
                .unwrap();
            assert!(entries.is_empty());
        }
    }
}
