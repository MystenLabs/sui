// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use consensus_config::Epoch;
use mysten_metrics::spawn_logged_monitored_task;
use prometheus::{
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry, IntCounter, IntCounterVec, IntGauge, Registry,
};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tokio::{
    sync::mpsc,
    time::{sleep, Instant},
};
use tracing::{error, info};
use typed_store::rocks::safe_drop_db;

struct Metrics {
    last_pruned_consensus_db_epoch: IntGauge,
    successfully_pruned_consensus_dbs: IntCounter,
    error_pruning_consensus_dbs: IntCounterVec,
}

impl Metrics {
    fn new(registry: &Registry) -> Self {
        Self {
            last_pruned_consensus_db_epoch: register_int_gauge_with_registry!(
                "last_pruned_consensus_db_epoch",
                "The last epoch for which the consensus store was pruned",
                registry
            )
            .unwrap(),
            successfully_pruned_consensus_dbs: register_int_counter_with_registry!(
                "successfully_pruned_consensus_dbs",
                "The number of consensus dbs successfully pruned",
                registry
            )
            .unwrap(),
            error_pruning_consensus_dbs: register_int_counter_vec_with_registry!(
                "error_pruning_consensus_dbs",
                "The number of errors encountered while pruning consensus dbs",
                &["mode"],
                registry
            )
            .unwrap(),
        }
    }
}

pub struct ConsensusStorePruner {
    tx_remove: mpsc::Sender<Epoch>,
    _handle: tokio::task::JoinHandle<()>,
}

impl ConsensusStorePruner {
    pub fn new(
        base_path: PathBuf,
        epoch_retention: u64,
        epoch_prune_period: Duration,
        registry: &Registry,
    ) -> Self {
        let (tx_remove, mut rx_remove) = mpsc::channel(1);
        let metrics = Metrics::new(registry);

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
                        Self::prune_old_epoch_data(&base_path, latest_epoch, epoch_retention, &metrics).await;
                    }
                    result = rx_remove.recv() => {
                        if result.is_none() {
                            info!("Closing consensus store pruner");
                            break;
                        }
                        latest_epoch = result.unwrap();
                        Self::prune_old_epoch_data(&base_path, latest_epoch, epoch_retention, &metrics).await;
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
        metrics: &Metrics,
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
                        "Could not prune old consensus storage \"{:?}\" directory with safe approach. Will fallback to force delete: {:?}",
                        f.path(),
                        e
                    );

                    metrics
                        .error_pruning_consensus_dbs
                        .with_label_values(&["safe"])
                        .inc();

                    const WAIT_BEFORE_FORCE_DELETE: Duration = Duration::from_secs(5);
                    sleep(WAIT_BEFORE_FORCE_DELETE).await;

                    if let Err(err) = fs::remove_dir_all(f.path()) {
                        error!(
                            "Could not prune old consensus storage \"{:?}\" directory with force delete: {:?}",
                            f.path(),
                            err
                        );
                        metrics
                            .error_pruning_consensus_dbs
                            .with_label_values(&["force"])
                            .inc();
                    } else {
                        info!(
                            "Successfully pruned consensus epoch storage directory with force delete: {:?}",
                            f.path()
                        );
                        let last_epoch = metrics.last_pruned_consensus_db_epoch.get();
                        metrics
                            .last_pruned_consensus_db_epoch
                            .set(last_epoch.max(file_epoch as i64));
                        metrics.successfully_pruned_consensus_dbs.inc();
                    }
                } else {
                    info!(
                        "Successfully pruned consensus epoch storage directory: {:?}",
                        f.path()
                    );
                    let last_epoch = metrics.last_pruned_consensus_db_epoch.get();
                    metrics
                        .last_pruned_consensus_db_epoch
                        .set(last_epoch.max(file_epoch as i64));
                    metrics.successfully_pruned_consensus_dbs.inc();
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
    use crate::epoch::consensus_store_pruner::{ConsensusStorePruner, Metrics};
    use prometheus::Registry;
    use std::fs;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_remove_old_epoch_data() {
        telemetry_subscribers::init_for_testing();
        let metrics = Metrics::new(&Registry::new());

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
                &metrics,
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
                &metrics,
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
                &metrics,
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

        let pruner = ConsensusStorePruner::new(
            base_directory.clone(),
            epoch_retention,
            epoch_prune_period,
            &Registry::new(),
        );

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
