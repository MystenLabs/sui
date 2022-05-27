// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use futures::future::join_all;
use sui_config::NetworkConfig;
use sui_node::SuiNode;
use tracing::{error, info};

pub struct SuiNetwork {
    pub spawned_authorities: Vec<SuiNode>,
}

impl SuiNetwork {
    pub async fn start(config: &NetworkConfig) -> Result<Self, anyhow::Error> {
        if config.validator_configs().is_empty() {
            return Err(anyhow!(
                "No authority configured for the network, please run genesis."
            ));
        }

        info!(
            "Starting network with {} authorities",
            config.validator_configs().len()
        );

        let mut spawned_authorities = Vec::new();
        for validator in config.validator_configs() {
            let server = SuiNode::start(validator).await?;
            spawned_authorities.push(server);
        }
        info!("Started {} authorities", spawned_authorities.len());

        Ok(Self {
            spawned_authorities,
        })
    }

    pub async fn wait_for_completion(self) -> Result<(), anyhow::Error> {
        let mut handles = Vec::new();
        for spawned_server in self.spawned_authorities {
            handles.push(async move {
                if let Err(err) = spawned_server.wait().await {
                    error!("Server ended with an error: {err}");
                }
            });
        }
        join_all(handles).await;
        info!("All servers stopped.");
        Ok(())
    }
}
