// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use clap::Args;
use tracing::info;

use sui_data_store::{ObjectKey, VersionQuery};
use sui_types::base_types::ObjectID;

use crate::store::ForkingStore;

#[derive(Args, Clone, Debug, Default)]
pub struct StartupSeeds {
    /// Explicit object IDs to prefetch at startup.
    #[clap(long, value_delimiter = ',')]
    pub objects: Vec<ObjectID>,
}

impl StartupSeeds {
    fn collect_missing_object_ids<T>(
        requested_object_ids: &[ObjectID],
        fetched_objects: &[Option<T>],
    ) -> Vec<ObjectID> {
        requested_object_ids
            .iter()
            .enumerate()
            .filter_map(|(idx, object_id)| {
                if matches!(fetched_objects.get(idx), Some(Some(_))) {
                    None
                } else {
                    Some(*object_id)
                }
            })
            .collect()
    }

    /// Prefetch explicit startup objects and fail startup if any requested object is missing.
    pub async fn prefetch_startup_objects(
        &self,
        store: &ForkingStore,
        startup_checkpoint: u64,
    ) -> Result<()> {
        if self.objects.is_empty() {
            return Ok(());
        }

        info!(
            startup_checkpoint,
            object_count = self.objects.len(),
            "Prefetching explicit startup objects"
        );

        let object_keys: Vec<_> = self
            .objects
            .iter()
            .copied()
            .map(|object_id| ObjectKey {
                object_id,
                version_query: VersionQuery::AtCheckpoint(startup_checkpoint),
            })
            .collect();

        let fetched_objects = store
            .get_objects(&object_keys)
            .context("Failed to prefetch startup objects from object store")?;

        let requested_ids = object_keys
            .iter()
            .map(|key| key.object_id)
            .collect::<Vec<_>>();
        let missing_objects = Self::collect_missing_object_ids(&requested_ids, &fetched_objects);
        if !missing_objects.is_empty() {
            let missing = missing_objects
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::bail!(
                "Failed to prefetch explicit startup objects at checkpoint {}. Missing object IDs: {}",
                startup_checkpoint,
                missing
            );
        }

        let fetched = fetched_objects.iter().flatten().count();
        info!(
            startup_checkpoint,
            requested = object_keys.len(),
            fetched,
            "Startup object prefetch completed"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use sui_types::base_types::ObjectID;

    use super::StartupSeeds;

    #[derive(Parser, Debug)]
    struct SeedCli {
        #[clap(flatten)]
        seeds: StartupSeeds,
    }

    fn parse_object_id(value: &str) -> ObjectID {
        ObjectID::from_hex_literal(value).expect("valid object id")
    }

    #[test]
    fn collects_missing_object_ids() {
        let id1 = parse_object_id("0x11");
        let id2 = parse_object_id("0x22");
        let id3 = parse_object_id("0x33");
        let requested = vec![id1, id2, id3];
        let fetched = vec![Some(()), None, Some(())];
        let missing = StartupSeeds::collect_missing_object_ids(&requested, &fetched);
        assert_eq!(missing, vec![id2]);
    }

    #[test]
    fn clap_parses_explicit_object_ids() {
        let parsed = SeedCli::try_parse_from(["seed-cli", "--objects", "0x5,0x6"])
            .expect("objects should parse");
        assert_eq!(
            parsed.seeds.objects,
            vec![parse_object_id("0x5"), parse_object_id("0x6")]
        );
    }
}
