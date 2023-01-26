// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::authority::authority_per_epoch_store::EPOCH_DB_PREFIX;
use itertools::Itertools;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use sui_config::node::AuthorityStorePruningConfig;
use tokio::sync::oneshot;
use tracing::log::{error, info};
use typed_store::rocks::safe_drop_db;

pub struct AuthorityPerEpochStorePruner {
    _cancel_handle: oneshot::Sender<()>,
}

impl AuthorityPerEpochStorePruner {
    pub fn new(parent_path: PathBuf, config: &AuthorityStorePruningConfig) -> Self {
        let (_cancel_handle, mut recv) = tokio::sync::oneshot::channel();
        let num_latest_epoch_dbs_to_retain = config.num_latest_epoch_dbs_to_retain;
        if num_latest_epoch_dbs_to_retain == 0 || num_latest_epoch_dbs_to_retain == usize::MAX {
            info!("Skipping pruning of epoch tables as we want to retain all versions");
            return Self { _cancel_handle };
        }
        let mut prune_interval =
            tokio::time::interval(Duration::from_secs(config.epoch_db_pruning_period_secs));
        tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    _ = prune_interval.tick() => {
                        info!("Starting pruning of epoch tables");
                        match Self::prune_old_directories(&parent_path, num_latest_epoch_dbs_to_retain) {
                            Ok(pruned_count) => info!("Finished pruning old epoch databases. Pruned {} dbs", pruned_count),
                            Err(err) => error!("Error while removing old epoch databases {:?}", err),
                        }
                    }
                    _ = &mut recv => break,
                }
            }
        });
        Self { _cancel_handle }
    }

    fn prune_old_directories(
        parent_path: &PathBuf,
        num_latest_epoch_dbs_to_retain: usize,
    ) -> Result<usize, anyhow::Error> {
        let mut candidates = vec![];
        let directories = fs::read_dir(parent_path)?.collect::<Result<Vec<_>, _>>()?;
        for directory in directories {
            let path = directory.path();
            if let Some(filename) = directory.file_name().to_str() {
                if let Ok(epoch) = filename.split_at(EPOCH_DB_PREFIX.len()).1.parse::<u64>() {
                    candidates.push((epoch, path));
                }
            }
        }
        let mut pruned = 0;
        let mut gc_results = vec![];
        if num_latest_epoch_dbs_to_retain < candidates.len() {
            let to_prune = candidates.len() - num_latest_epoch_dbs_to_retain;
            for (_, path) in candidates.into_iter().sorted().take(to_prune) {
                info!("Dropping epoch directory {:?}", path);
                pruned += 1;
                gc_results.push(safe_drop_db(path.join("recovery_log")));
                gc_results.push(safe_drop_db(path));
            }
        }
        gc_results.into_iter().collect::<Result<Vec<_>, _>>()?;
        Ok(pruned)
    }
}

#[cfg(test)]
mod tests {
    use crate::authority::authority_per_epoch_store_pruner::AuthorityPerEpochStorePruner;
    use std::fs;

    #[test]
    fn test_basic_epoch_pruner() {
        let parent_directory = tempfile::tempdir().unwrap().into_path();
        let directories: Vec<_> = vec!["epoch_0", "epoch_1", "epoch_3", "epoch_4"]
            .into_iter()
            .map(|name| parent_directory.join(name))
            .collect();
        for directory in &directories {
            fs::create_dir(directory).expect("failed to create directory");
        }

        let pruned =
            AuthorityPerEpochStorePruner::prune_old_directories(&parent_directory, 2).unwrap();
        assert_eq!(pruned, 2);
        assert_eq!(
            directories
                .into_iter()
                .map(|f| fs::metadata(f).is_ok())
                .collect::<Vec<_>>(),
            vec![false, false, true, true]
        );
    }
}
