// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
#[path = "../unit_tests/epoch_data_tests.rs"]
pub mod epoch_data_tests;

use mysten_metrics::spawn_monitored_task;
use narwhal_config::Epoch;
use std::fs;
use std::path::PathBuf;
use tokio::sync::mpsc;

pub struct EpochDataRemover {
    base_path: PathBuf,
    tx_remove: mpsc::Sender<Epoch>,
}

impl EpochDataRemover {
    pub fn new(base_path: PathBuf) -> Self {
        let (tx_remove, _rx_remove) = mpsc::channel(1);
        Self {
            base_path,
            tx_remove,
        }
    }

    pub async fn run(&mut self) {
        let (tx_remove, mut rx_remove) = mpsc::channel(1);
        self.tx_remove = tx_remove;
        let base_path = self.base_path.clone();
        spawn_monitored_task!(async {
            tracing::info!("Starting Epoch Data Remover");
            loop {
                match rx_remove.recv().await {
                    Some(epoch) => {
                        remove_old_epoch_data(base_path.clone(), epoch);
                    }
                    None => {
                        tracing::info!("Closing Epoch Data Remover");
                        break;
                    }
                }
            }
        });
    }

    pub async fn remove_old_data(&self, latest_closed_epoch: Epoch) {
        let result = self.tx_remove.send(latest_closed_epoch).await;
        if result.is_err() {
            tracing::error!(
                "Error sending message to data removal task for epoch {:?}",
                latest_closed_epoch,
            );
        }
    }
}

pub(crate) fn remove_old_epoch_data(storage_base_path: PathBuf, epoch: Epoch) {
    if epoch < 1 {
        return;
    }

    // Keep previous epoch data as a safety buffer and remove starting from epoch - 1
    let drop_boundary = epoch - 1;

    tracing::info!(
        "Starting old epoch data removal for epoch {:?}",
        drop_boundary
    );

    // Get all the epoch stores in the base path directory
    let files = match fs::read_dir(storage_base_path) {
        Ok(f) => f,
        Err(e) => {
            tracing::error!("Data Remover cannot read the files in the storage path directory for epoch cleanup: {:?}", e);
            return;
        }
    };

    // Look for any that are less than or equal to the drop boundary and drop
    for file_res in files {
        let f = match file_res {
            Ok(f) => f,
            Err(e) => {
                tracing::error!(
                    "Data Remover error while cleaning up storage of previous epochs: {:?}",
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
                tracing::error!("Data Remover could not parse file in storage path into epoch for cleanup: {:?}",e);
                continue;
            }
        };

        if file_epoch <= drop_boundary {
            if let Err(e) = fs::remove_dir_all(f.path()) {
                tracing::error!(
                    "Data Remover could not remove old epoch storage directory: {:?}",
                    e
                );
            }
        }
    }

    tracing::info!(
        "Completed old epoch data removal process for epoch {:?}",
        epoch
    );
}
