// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use backoff::future::retry;
use backoff::ExponentialBackoff;
use sui_sdk::SuiClient;
use tracing::{error, info, warn};

use crate::processors::address_processor::AddressProcessor;
use crate::processors::object_processor::ObjectProcessor;
use crate::processors::package_processor::PackageProcessor;

pub struct ProcessorOrchestrator {
    pub rpc_client: SuiClient,
    pub db_url: String,
}

impl ProcessorOrchestrator {
    pub fn new(rpc_client: SuiClient, db_url: String) -> Self {
        Self { rpc_client, db_url }
    }

    pub async fn run_forever(&mut self) {
        info!("Processor orchestrator started...");
        let address_processor = AddressProcessor::new(self.db_url.clone());
        let object_processor = ObjectProcessor::new(self.db_url.clone());
        let package_processor = PackageProcessor::new(self.rpc_client.clone(), self.db_url.clone());

        tokio::task::spawn(async move {
            let addr_result = retry(ExponentialBackoff::default(), || async {
                let addr_processor_exec_res = address_processor.start().await;
                if let Err(e) = addr_processor_exec_res.clone() {
                    warn!(
                        "Indexer address processor failed with error: {:?}, retrying...",
                        e
                    );
                }
                Ok(addr_processor_exec_res?)
            })
            .await;
            if let Err(e) = addr_result {
                error!(
                    "Indexer address processor failed after retrials with error {:?}",
                    e
                );
            }
        });
        tokio::task::spawn(async move {
            let obj_result = retry(ExponentialBackoff::default(), || async {
                let obj_processor_exec_res = object_processor.start().await;
                if let Err(e) = obj_processor_exec_res.clone() {
                    warn!(
                        "Indexer object processor failed with error: {:?}, retrying...",
                        e
                    );
                }
                Ok(obj_processor_exec_res?)
            })
            .await;
            if let Err(e) = obj_result {
                error!(
                    "Indexer object processor failed after retrials with error {:?}",
                    e
                );
            }
        });
        tokio::task::spawn(async move {
            let pkg_result = retry(ExponentialBackoff::default(), || async {
                let pkg_processor_exec_res = package_processor.start().await;
                if let Err(e) = pkg_processor_exec_res.clone() {
                    warn!(
                        "Indexer package processor failed with error: {:?}, retrying...",
                        e
                    );
                }
                Ok(pkg_processor_exec_res?)
            })
            .await;
            if let Err(e) = pkg_result {
                error!(
                    "Indexer package processor failed after retrials with error {:?}",
                    e
                );
            }
        });
    }
}
