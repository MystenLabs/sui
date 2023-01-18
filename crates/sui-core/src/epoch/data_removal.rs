// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use narwhal_config::Epoch;
use std::fs;
use std::path::PathBuf;
use tokio::sync::mpsc;

pub struct EpochDataRemover {
    base_path: PathBuf,
    tx_remove: mpsc::Sender<Epoch>,
    rx_remove: mpsc::Receiver<Epoch>,
}

impl EpochDataRemover {
    pub async fn new(base_path: PathBuf) -> Self {
        let (tx_remove, rx_remove) = mpsc::channel(1);
        Self {
            base_path,
            tx_remove,
            rx_remove,
        }
    }

    pub async fn run(&mut self) {
        while let Some(epoch) = self.rx_remove.recv().await {
            remove_old_epoch_data(self.base_path.clone(), epoch);
        }
    }

    pub async fn remove_old_data(
        self,
        latest_closed_epoch: Epoch,
    ) -> Result<(), mpsc::error::SendError<Epoch>> {
        self.tx_remove.send(latest_closed_epoch).await
    }
}

pub(crate) fn remove_old_epoch_data(storage_base_path: PathBuf, epoch: Epoch) {
    // Keep previous epoch data as a safety buffer and remove starting from epoch - 1
    let drop_boundary = epoch - 1;

    tracing::info!(
        "Starting Narwhal old epoch data removal for epoch {:?}",
        drop_boundary
    );

    // Get all the epoch stores in the base path directory
    let files = match fs::read_dir(storage_base_path) {
        Ok(f) => f,
        Err(e) => {
            tracing::error!("Narwhal Manager cannot read the files in the storage path directory for epoch cleanup: {:?}", e);
            return;
        }
    };

    // Look for any that are less than or equal to the drop boundary and drop
    for file_res in files {
        let f = match file_res {
            Ok(f) => f,
            Err(e) => {
                tracing::error!(
                    "Narwhal Manager error while cleaning up storage of previous epochs: {:?}",
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
                tracing::error!("Narwhal Manager could not parse file in storage path into epoch for cleanup: {:?}",e);
                continue;
            }
        };

        if file_epoch <= drop_boundary {
            if let Err(e) = fs::remove_dir(f.path()) {
                tracing::error!(
                    "Narwhal Manager could not remove old epoch storage directory: {:?}",
                    e
                );
            }
        }
    }

    tracing::info!(
        "Completed Narwhal old epoch data removal process for epoch {:?}",
        epoch
    );
}
