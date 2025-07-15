// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use config::{PipelineLayer, ServiceConfig};
use db::config::DbConfig;
use handlers::object_by_owner::ObjectByOwner;
use indexer::Indexer;
use prometheus::Registry;
use schema::Schema;
use sui_indexer_alt_framework::{
    ingestion::ClientArgs, pipeline::sequential::SequentialConfig, pipeline::CommitterConfig,
    IndexerArgs,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

mod config;
mod db;
mod handlers;
mod indexer;
pub(crate) mod schema;
mod store;

pub async fn start_service(
    path: impl AsRef<Path>,
    indexer_args: IndexerArgs,
    client_args: ClientArgs,
    config: ServiceConfig,
    registry: &Registry,
    cancel: CancellationToken,
) -> anyhow::Result<JoinHandle<()>> {
    let ServiceConfig {
        ingestion,
        consistency,
        rocksdb,
        committer,
        pipeline: PipelineLayer { object_by_owner },
    } = config;

    let committer = committer.finish(CommitterConfig::default());

    let mut indexer: Indexer<Schema> = Indexer::new(
        path,
        indexer_args,
        client_args,
        consistency,
        ingestion.into(),
        rocksdb,
        registry,
        cancel,
    )
    .await?;

    macro_rules! add_sequential {
        ($handler:expr, $config:expr) => {
            if let Some(layer) = $config {
                indexer
                    .sequential_pipeline(
                        $handler,
                        SequentialConfig {
                            committer: layer.finish(committer.clone()),
                            checkpoint_lag: 0,
                        },
                    )
                    .await?
            }
        };
    }

    add_sequential!(ObjectByOwner, object_by_owner);

    indexer.run().await
}
