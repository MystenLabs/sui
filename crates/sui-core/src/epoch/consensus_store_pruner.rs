// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_metrics::spawn_logged_monitored_task;
use narwhal_config::Epoch;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tokio::{
    sync::mpsc,
    time::{sleep, Instant},
};
use tracing::{error, info};
use typed_store::rocks::safe_drop_db;

pub struct ConsensusStorePruner {
    tx_remove: mpsc::Sender<Epoch>,
    _handle: tokio::task::JoinHandle<()>,
}

impl ConsensusStorePruner {
    pub fn new(base_path: PathBuf, epoch_retention: u64, epoch_prune_period: Duration) -> Self {
        let (tx_remove, mut rx_remove) = mpsc::channel(1);

        let _handle = spawn_logged_monitored_task!(async {
            info!("Starting consensus store pruner with epoch retention {epoch_retention} and prune period {epoch_prune_period:?}");

            let mut timeout = tokio::time::interval_at(
                Instant::now() + Duration::from_secs(60), // allow some time for the node to boot etc before attempting to prune
                epoch_prune_period,
            );
            let mut latest_epoch = 0;
            loop {
                tokio::select! {
                    _ = timeout.tick() => {
                        Self::prune_old_epoch_data(&base_path, latest_epoch, epoch_retention).await;
                    }
                    result = rx_remove.recv() => {
                        if result.is_none() {
                            info!("Closing consensus store pruner");
                            break;
                        }
                        latest_epoch = result.unwrap();
                        Self::prune_old_epoch_data(&base_path, latest_epoch, epoch_retention).await;
                    }
                }
            }
        });

        Self { tx_remove, _handle }
    }

    /// This method will remove all epoch data stores and directories that are older than the current epoch minus the epoch retention. The method ensures
    /// that always the `current_epoch` data is retained.
    pub async fn prune(&self, current_epoch: Epoch) {
        let result = self.tx_remove.send(current_epoch).await;
        if result.is_err() {
            error!(
                "Error sending message to data removal task for epoch {:?}",
                current_epoch,
            );
        }
    }

    async fn prune_old_epoch_data(
        storage_base_path: &PathBuf,
        current_epoch: Epoch,
        epoch_retention: u64,
    ) {
        let drop_boundary = current_epoch.saturating_sub(epoch_retention);

        info!(
            "Consensus store prunning for current epoch {}. Will remove epochs < {:?}",
            current_epoch, drop_boundary
        );

        // Get all the epoch stores in the base path directory
        let files = match fs::read_dir(storage_base_path) {
            Ok(f) => f,
            Err(e) => {
                error!(
                    "Can not read the files in the storage path directory for epoch cleanup: {:?}",
                    e
                );
                return;
            }
        };

        // Look for any that are less than the drop boundary and drop
        for file_res in files {
            let f = match file_res {
                Ok(f) => f,
                Err(e) => {
                    error!(
                        "Error while cleaning up storage of previous epochs: {:?}",
                        e
                    );
                    continue;
                }
            };

            let name = f.file_name();
            let file_epoch_string = match name.to_str() {
                Some(f) => f,
                None => continue,
            };

            let file_epoch = match file_epoch_string.to_owned().parse::<u64>() {
                Ok(f) => f,
                Err(e) => {
                    error!(
                        "Could not parse file \"{file_epoch_string}\" in storage path into epoch for cleanup: {:?}",
                        e
                    );
                    continue;
                }
            };

            if file_epoch < drop_boundary {
                if let Err(e) = safe_drop_db(f.path()) {
                    error!(
                        "Could not prune old epoch storage \"{:?}\" directory with safe approach. Will fallback to force delete: {:?}",
                        f.path(),
                        e
                    );

                    const WAIT_BEFORE_FORCE_DELETE: Duration = Duration::from_secs(5);
                    sleep(WAIT_BEFORE_FORCE_DELETE).await;

                    if let Err(err) = fs::remove_dir_all(f.path()) {
                        error!(
                            "Could not prune old epoch storage \"{:?}\" directory with force delete: {:?}",
                            f.path(),
                            err
                        );
                    } else {
                        info!(
                            "Successfully pruned old epoch storage directory with force delete: {:?}",
                            f.path()
                        );
                    }
                } else {
                    info!(
                        "Successfully pruned old epoch storage directory: {:?}",
                        f.path()
                    );
                }
            }
        }

        info!(
            "Completed old epoch data removal process for epoch {:?}",
            current_epoch
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::epoch::consensus_store_pruner::ConsensusStorePruner;
    use std::fs;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_remove_old_epoch_data() {
        telemetry_subscribers::init_for_testing();

        {
            // Epoch 0 should not be removed when it's current epoch.
            let epoch_retention = 0;
            let current_epoch = 0;

            let base_directory = tempfile::tempdir().unwrap().into_path();

            create_epoch_directories(&base_directory, vec!["0", "other"]);

            ConsensusStorePruner::prune_old_epoch_data(
                &base_directory,
                current_epoch,
                epoch_retention,
            )
            .await;

            let epochs_left = read_epoch_directories(&base_directory);

            assert_eq!(epochs_left.len(), 1);
            assert_eq!(epochs_left[0], 0);
        }

        {
            // Every directory should be retained only for 1 epoch. We expect any epoch directories < 99 to be removed.
            let epoch_retention = 1;
            let current_epoch = 100;

            let base_directory = tempfile::tempdir().unwrap().into_path();

            create_epoch_directories(&base_directory, vec!["97", "98", "99", "100", "other"]);

            ConsensusStorePruner::prune_old_epoch_data(
                &base_directory,
                current_epoch,
                epoch_retention,
            )
            .await;

            let epochs_left = read_epoch_directories(&base_directory);

            assert_eq!(epochs_left.len(), 2);
            assert_eq!(epochs_left[0], 99);
            assert_eq!(epochs_left[1], 100);
        }

        {
            // Every directory should be retained only for 0 epochs. That means only the current epoch directory should be retained and everything else
            // deleted.
            let epoch_retention = 0;
            let current_epoch = 100;

            let base_directory = tempfile::tempdir().unwrap().into_path();

            create_epoch_directories(&base_directory, vec!["97", "98", "99", "100", "other"]);

            ConsensusStorePruner::prune_old_epoch_data(
                &base_directory,
                current_epoch,
                epoch_retention,
            )
            .await;

            let epochs_left = read_epoch_directories(&base_directory);

            assert_eq!(epochs_left.len(), 1);
            assert_eq!(epochs_left[0], 100);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_consensus_store_pruner() {
        let epoch_retention = 1;
        let epoch_prune_period = std::time::Duration::from_millis(500);

        let base_directory = tempfile::tempdir().unwrap().into_path();

        // We create some directories up to epoch 100
        create_epoch_directories(&base_directory, vec!["97", "98", "99", "100", "other"]);

        let pruner =
            ConsensusStorePruner::new(base_directory.clone(), epoch_retention, epoch_prune_period);

        // We let the pruner run for a couple of times to prune the old directories. Since the default epoch of 0 is used no dirs should be pruned.
        sleep(3 * epoch_prune_period).await;

        // We expect the directories to be the same as before
        let epoch_dirs = read_epoch_directories(&base_directory);
        assert_eq!(epoch_dirs.len(), 4);

        // Then we update the epoch and instruct to prune for current epoch = 100
        pruner.prune(100).await;

        // We let the pruner run and check again the directories - no directories of epoch < 99 should be left
        sleep(2 * epoch_prune_period).await;

        let epoch_dirs = read_epoch_directories(&base_directory);
        assert_eq!(epoch_dirs.len(), 2);
        assert_eq!(epoch_dirs[0], 99);
        assert_eq!(epoch_dirs[1], 100);
    }

    fn create_epoch_directories(base_directory: &std::path::Path, epochs: Vec<&str>) {
        for epoch in epochs {
            let mut path = base_directory.to_path_buf();
            path.push(epoch);
            fs::create_dir(path).unwrap();
        }
    }

    fn read_epoch_directories(base_directory: &std::path::Path) -> Vec<u64> {
        let files = fs::read_dir(base_directory).unwrap();

        let mut epochs = Vec::new();
        for file_res in files {
            let file_epoch_string = file_res.unwrap().file_name().to_str().unwrap().to_owned();
            if let Ok(file_epoch) = file_epoch_string.parse::<u64>() {
                epochs.push(file_epoch);
            }
        }

        epochs.sort();
        epochs
    }
}
