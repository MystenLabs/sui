// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{path::Path, sync::Arc};

use anyhow::Context as _;
use tracing::info;

use sui_data_store::{
    FullCheckpointData, SetupStore as _,
    stores::{DataStore, FileSystemStore, InMemoryStore, ReadThroughStore, WriteThroughStore},
};
use sui_rpc_api::client::Client as RpcClient;
use sui_types::supported_protocol_versions::Chain;

use crate::{
    network::ForkNetwork,
    store::{DiskThenGraphqlObjects, HotObjects},
};

pub(super) struct DataStoreSetup {
    filesystem: Arc<FileSystemStore>,
    memory: Arc<InMemoryStore>,
    graphql: Arc<DataStore>,
    objects: Arc<HotObjects>,
    fullnode_endpoint: String,
}

impl DataStoreSetup {
    pub(super) fn filesystem(&self) -> &Arc<FileSystemStore> {
        &self.filesystem
    }

    pub(super) fn memory(&self) -> &Arc<InMemoryStore> {
        &self.memory
    }

    pub(super) fn graphql(&self) -> &Arc<DataStore> {
        &self.graphql
    }

    pub(super) fn objects(&self) -> &Arc<HotObjects> {
        &self.objects
    }

    pub(super) fn into_parts(self) -> (Arc<FileSystemStore>, Arc<HotObjects>) {
        (self.filesystem, self.objects)
    }

    pub(super) fn determine_startup_checkpoint(
        &self,
        checkpoint: Option<u64>,
        forked_at_checkpoint: u64,
    ) -> Result<u64, anyhow::Error> {
        Ok(checkpoint.unwrap_or(forked_at_checkpoint))
    }

    pub(super) async fn load_startup_checkpoint_data(
        &self,
        startup_checkpoint: u64,
    ) -> Result<FullCheckpointData, anyhow::Error> {
        self.fetch_checkpoint_from_grpc(startup_checkpoint).await
    }

    #[cfg(test)]
    async fn load_startup_checkpoint_data_with<F, Fut>(
        &self,
        startup_checkpoint: u64,
        fetcher: F,
    ) -> Result<FullCheckpointData, anyhow::Error>
    where
        F: FnOnce(u64) -> Fut,
        Fut: std::future::Future<Output = Result<FullCheckpointData, anyhow::Error>>,
    {
        fetcher(startup_checkpoint).await
    }

    pub(super) async fn fetch_checkpoint_from_grpc(
        &self,
        sequence: u64,
    ) -> Result<FullCheckpointData, anyhow::Error> {
        let mut client = RpcClient::new(self.fullnode_endpoint.clone()).with_context(|| {
            format!(
                "failed to construct fullnode gRPC client for {}",
                self.fullnode_endpoint
            )
        })?;
        client.get_full_checkpoint(sequence).await.with_context(|| {
            format!(
                "failed to fetch checkpoint {} from fullnode gRPC endpoint {}",
                sequence, self.fullnode_endpoint
            )
        })
    }
}

pub(super) fn build_data_store_setup(
    fork_network: &ForkNetwork,
    fullnode_endpoint: &str,
    at_checkpoint: u64,
    data_ingestion_path: &Path,
    version: &'static str,
) -> Result<DataStoreSetup, anyhow::Error> {
    let forking_path = format!(
        "forking/{}/forked_at_checkpoint_{}",
        fork_network.cache_path_component(),
        at_checkpoint
    );

    let node = fork_network.node();
    let fs_base_path = data_ingestion_path.join(forking_path);
    let filesystem = Arc::new(
        FileSystemStore::new_with_path(node.clone(), fs_base_path.clone())
            .context("failed to initialize file-system cache store")?,
    );
    let memory = Arc::new(InMemoryStore::new(node.clone()));
    let graphql = Arc::new(
        DataStore::new(node, version).context("failed to initialize GraphQL object source")?,
    );

    let disk_then_graphql_objects: Arc<DiskThenGraphqlObjects> =
        Arc::new(ReadThroughStore::new(filesystem.clone(), graphql.clone()));
    let objects: Arc<HotObjects> = Arc::new(WriteThroughStore::new(
        memory.clone(),
        disk_then_graphql_objects,
    ));

    info!("Fs base path {}", fs_base_path.display());
    match fork_network {
        ForkNetwork::Mainnet => {
            objects
                .setup(Some(Chain::Mainnet.as_str().to_string()))
                .context("failed to initialize local mainnet node mapping")?;
        }
        ForkNetwork::Testnet => {
            objects
                .setup(Some(Chain::Testnet.as_str().to_string()))
                .context("failed to initialize local testnet node mapping")?;
        }
        ForkNetwork::Devnet | ForkNetwork::Custom(_) => {
            let chain_id = objects
                .setup(None)
                .context("failed to initialize dynamic chain identifier mapping")?
                .with_context(|| {
                    format!(
                        "missing chain identifier while setting up {} data store",
                        fork_network.display_name()
                    )
                })?;
            info!(
                "Resolved dynamic chain identifier for {}: {}",
                fork_network.display_name(),
                chain_id
            );
        }
    }

    Ok(DataStoreSetup {
        filesystem,
        memory,
        graphql,
        objects,
        fullnode_endpoint: fullnode_endpoint.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;
    use tempfile::TempDir;

    use super::*;

    fn sample_checkpoint(sequence: u64) -> FullCheckpointData {
        TestCheckpointBuilder::new(sequence)
            .start_transaction(1)
            .create_owned_object(42)
            .finish_transaction()
            .build_checkpoint()
    }

    fn build_test_setup(tempdir: &TempDir) -> Result<DataStoreSetup, anyhow::Error> {
        build_data_store_setup(
            &ForkNetwork::Testnet,
            "http://127.0.0.1:9000",
            11,
            tempdir.path(),
            "test-version",
        )
    }

    #[test]
    fn determine_startup_checkpoint_uses_requested_or_default() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let setup = build_test_setup(&tempdir).expect("setup");

        assert_eq!(
            setup
                .determine_startup_checkpoint(Some(17), 11)
                .expect("requested checkpoint"),
            17
        );
        assert_eq!(
            setup
                .determine_startup_checkpoint(None, 11)
                .expect("default checkpoint"),
            11
        );
    }

    #[tokio::test]
    async fn load_startup_checkpoint_uses_fetcher() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let setup = build_test_setup(&tempdir).expect("setup");
        let checkpoint = sample_checkpoint(23);
        let fetch_calls = Arc::new(AtomicUsize::new(0));

        let loaded = setup
            .load_startup_checkpoint_data_with(23, {
                let fetch_calls = fetch_calls.clone();
                let checkpoint = checkpoint.clone();
                move |sequence| {
                    let fetch_calls = fetch_calls.clone();
                    let checkpoint = checkpoint.clone();
                    async move {
                        assert_eq!(sequence, 23);
                        fetch_calls.fetch_add(1, Ordering::Relaxed);
                        Ok(checkpoint)
                    }
                }
            })
            .await
            .expect("load checkpoint");

        assert_eq!(loaded.summary.sequence_number, 23);
        assert_eq!(fetch_calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn load_startup_checkpoint_propagates_fetch_error() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let setup = build_test_setup(&tempdir).expect("setup");

        let err = setup
            .load_startup_checkpoint_data_with(23, |_| async { anyhow::bail!("fetch failed") })
            .await
            .expect_err("fetch should fail");

        assert!(err.to_string().contains("fetch failed"));
    }
}
