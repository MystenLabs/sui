// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Progress monitoring for analytics pipelines.
//!
//! This module provides traits and implementations for monitoring the progress
//! of data through analytics pipelines, including checking checkpoints in
//! external stores like Snowflake.

mod snowflake;

use std::fs;

use anyhow::{Result, anyhow};
use tracing::info;

use crate::config::IndexerConfig;
use crate::metrics::Metrics;

pub use snowflake::SnowflakeMaxCheckpointReader;

/// Trait for reading the maximum checkpoint from an external store.
#[async_trait::async_trait]
pub trait MaxCheckpointReader: Send + Sync + 'static {
    /// Returns the maximum checkpoint number in the store.
    async fn max_checkpoint(&self) -> Result<i64>;
}

fn load_password(path: &str) -> Result<String> {
    Ok(fs::read_to_string(path)?.trim().to_string())
}

/// Spawns background tasks to monitor Snowflake table checkpoints.
pub fn spawn_snowflake_monitors(
    config: &IndexerConfig,
    metrics: Metrics,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<Vec<tokio::task::JoinHandle<()>>> {
    let mut handles = Vec::new();

    for pipeline_config in config.pipeline_configs() {
        if !pipeline_config.report_sf_max_table_checkpoint {
            continue;
        }

        let sf_table_id = pipeline_config
            .sf_table_id
            .as_ref()
            .ok_or_else(|| {
                anyhow!(
                    "Missing sf_table_id for pipeline {}",
                    pipeline_config.pipeline
                )
            })?
            .clone();

        let sf_checkpoint_col_id = pipeline_config
            .sf_checkpoint_col_id
            .as_ref()
            .ok_or_else(|| {
                anyhow!(
                    "Missing sf_checkpoint_col_id for pipeline {}",
                    pipeline_config.pipeline
                )
            })?
            .clone();

        let account_identifier = config
            .sf_account_identifier
            .as_ref()
            .ok_or_else(|| anyhow!("Missing sf_account_identifier"))?
            .clone();

        let warehouse = config
            .sf_warehouse
            .as_ref()
            .ok_or_else(|| anyhow!("Missing sf_warehouse"))?
            .clone();

        let database = config
            .sf_database
            .as_ref()
            .ok_or_else(|| anyhow!("Missing sf_database"))?
            .clone();

        let schema = config
            .sf_schema
            .as_ref()
            .ok_or_else(|| anyhow!("Missing sf_schema"))?
            .clone();

        let username = config
            .sf_username
            .as_ref()
            .ok_or_else(|| anyhow!("Missing sf_username"))?
            .clone();

        let role = config
            .sf_role
            .as_ref()
            .ok_or_else(|| anyhow!("Missing sf_role"))?
            .clone();

        let password = load_password(
            config
                .sf_password_file
                .as_ref()
                .ok_or_else(|| anyhow!("Missing sf_password_file"))?,
        )?;

        let pipeline_name = pipeline_config.pipeline.to_string();
        let metrics = metrics.clone();
        let cancel = cancel.clone();

        let handle = tokio::spawn(async move {
            info!("Starting Snowflake monitor for pipeline: {}", pipeline_name);

            let reader = match SnowflakeMaxCheckpointReader::new(
                &account_identifier,
                &warehouse,
                &database,
                &schema,
                &username,
                &role,
                &password,
                &sf_table_id,
                &sf_checkpoint_col_id,
            )
            .await
            {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!(
                        "Failed to create Snowflake reader for {}: {}",
                        pipeline_name,
                        e
                    );
                    return;
                }
            };

            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        break;
                    }
                    _ = interval.tick() => {
                        match reader.max_checkpoint().await {
                            Ok(max_cp) => {
                                metrics
                                    .max_checkpoint_on_store
                                    .with_label_values(&[&pipeline_name])
                                    .set(max_cp);
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to query Snowflake max checkpoint for {}: {}",
                                    pipeline_name,
                                    e
                                );
                            }
                        }
                    }
                }
            }
        });

        handles.push(handle);
    }

    Ok(handles)
}
