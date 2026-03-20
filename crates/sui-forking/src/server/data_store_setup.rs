// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{path::Path, sync::Arc};

use anyhow::Context as _;
use tracing::info;

use sui_data_store::{
    CheckpointStore as _, CheckpointStoreWriter as _, FullCheckpointData, SetupStore as _,
    stores::{DataStore, FileSystemStore, InMemoryStore, ReadThroughStore, WriteThroughStore},
};
use sui_rpc_api::client::Client as RpcClient;
use sui_types::supported_protocol_versions::Chain;

use crate::{
    network::ForkNetwork,
    store::{DiskThenGraphqlObjects, ForkDataStore, HotMemFs, HotObjects},
};

pub(super) struct DataStoreSetup {
    filesystem: Arc<FileSystemStore>,
    store: ForkDataStore,
    fullnode_endpoint: String,
}

impl DataStoreSetup {
    pub(super) fn filesystem(&self) -> &Arc<FileSystemStore> {
        &self.filesystem
    }

    pub(super) fn store(&self) -> &ForkDataStore {
        &self.store
    }

    pub(super) fn into_parts(self) -> (Arc<FileSystemStore>, ForkDataStore) {
        (self.filesystem, self.store)
    }

    pub(super) fn determine_startup_checkpoint(
        &self,
        checkpoint: Option<u64>,
        forked_at_checkpoint: u64,
    ) -> Result<u64, anyhow::Error> {
        let Some(requested_checkpoint) = checkpoint else {
            return Ok(forked_at_checkpoint);
        };

        let local_latest = self
            .store
            .get_latest_checkpoint()
            .context("failed to inspect local checkpoint cache")?;

        match local_latest {
            None => Ok(requested_checkpoint),
            Some(checkpoint_data) => {
                let local_latest_sequence = checkpoint_data.summary.sequence_number;
                if local_latest_sequence < requested_checkpoint {
                    anyhow::bail!(
                        "local fork cache for checkpoint {} is stale: latest local checkpoint is {}",
                        requested_checkpoint,
                        local_latest_sequence
                    );
                }
                Ok(local_latest_sequence)
            }
        }
    }

    pub(super) async fn load_startup_checkpoint_data(
        &self,
        startup_checkpoint: u64,
    ) -> Result<FullCheckpointData, anyhow::Error> {
        match self
            .store
            .get_checkpoint_by_sequence_number(startup_checkpoint)
            .context("failed to read startup checkpoint from local checkpoint store")?
        {
            Some(checkpoint) => Ok(checkpoint),
            None => {
                let checkpoint = self.fetch_checkpoint_from_grpc(startup_checkpoint).await?;
                self.store.write_checkpoint(&checkpoint).with_context(|| {
                    format!(
                        "failed to persist startup checkpoint {} into local stores",
                        startup_checkpoint
                    )
                })?;
                Ok(checkpoint)
            }
        }
    }

    #[cfg(test)]
    async fn load_startup_checkpoint_data_with<F, Fut>(
        &self,
        startup_checkpoint: u64,
        fetcher: F,
    ) -> Result<FullCheckpointData, anyhow::Error>
    where
        F: FnOnce(u64) -> Fut,
        Fut: Future<Output = Result<FullCheckpointData, anyhow::Error>>,
    {
        match self
            .store
            .get_checkpoint_by_sequence_number(startup_checkpoint)
            .context("failed to read startup checkpoint from local checkpoint store")?
        {
            Some(checkpoint) => Ok(checkpoint),
            None => {
                let checkpoint = fetcher(startup_checkpoint).await?;
                self.store.write_checkpoint(&checkpoint).with_context(|| {
                    format!(
                        "failed to persist startup checkpoint {} into local stores",
                        startup_checkpoint
                    )
                })?;
                Ok(checkpoint)
            }
        }
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
    let graphql =
        Arc::new(DataStore::new(node, version).context("failed to initialize GraphQL data store")?);

    let hot_mem_fs: Arc<HotMemFs> =
        Arc::new(WriteThroughStore::new(memory.clone(), filesystem.clone()));
    let disk_then_graphql_objects: Arc<DiskThenGraphqlObjects> =
        Arc::new(ReadThroughStore::new(filesystem.clone(), graphql));
    let hot_objects: Arc<HotObjects> =
        Arc::new(WriteThroughStore::new(memory, disk_then_graphql_objects));

    info!("Fs base path {:?}", fs_base_path.display());
    match fork_network {
        ForkNetwork::Mainnet => {
            hot_mem_fs
                .setup(Some(Chain::Mainnet.as_str().to_string()))
                .context("failed to initialize local mainnet node mapping")?;
        }
        ForkNetwork::Testnet => {
            hot_mem_fs
                .setup(Some(Chain::Testnet.as_str().to_string()))
                .context("failed to initialize local testnet node mapping")?;
        }
        ForkNetwork::Devnet | ForkNetwork::Custom(_) => {
            let chain_id = hot_objects
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

    let store = ForkDataStore::new(
        hot_mem_fs.clone(),
        hot_mem_fs.clone(),
        hot_objects,
        hot_mem_fs,
    );

    Ok(DataStoreSetup {
        filesystem,
        store,
        fullnode_endpoint: fullnode_endpoint.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use sui_data_store::{CheckpointStore as _, CheckpointStoreWriter as _};
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
    fn determine_startup_checkpoint_uses_requested_when_local_cache_is_empty() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let setup = build_test_setup(&tempdir).expect("setup");

        assert_eq!(
            setup
                .determine_startup_checkpoint(Some(17), 11)
                .expect("startup checkpoint"),
            17
        );
        assert_eq!(
            setup
                .determine_startup_checkpoint(None, 11)
                .expect("default checkpoint"),
            11
        );
    }

    #[test]
    fn determine_startup_checkpoint_resumes_from_local_latest() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let setup = build_test_setup(&tempdir).expect("setup");
        let checkpoint = sample_checkpoint(19);

        setup
            .store()
            .write_checkpoint(&checkpoint)
            .expect("write checkpoint");

        assert_eq!(
            setup
                .determine_startup_checkpoint(Some(11), 11)
                .expect("resumed checkpoint"),
            19
        );
    }

    #[test]
    fn determine_startup_checkpoint_rejects_stale_local_cache() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let setup = build_test_setup(&tempdir).expect("setup");
        let checkpoint = sample_checkpoint(13);

        setup
            .store()
            .write_checkpoint(&checkpoint)
            .expect("write checkpoint");

        let err = setup
            .determine_startup_checkpoint(Some(17), 11)
            .expect_err("stale cache should fail");
        assert!(
            err.to_string()
                .contains("local fork cache for checkpoint 17 is stale")
        );
    }

    #[tokio::test]
    async fn load_startup_checkpoint_uses_local_cache_without_fetching() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let setup = build_test_setup(&tempdir).expect("setup");
        let checkpoint = sample_checkpoint(21);
        let fetch_calls = Arc::new(AtomicUsize::new(0));

        setup
            .store()
            .write_checkpoint(&checkpoint)
            .expect("write checkpoint");

        let loaded = setup
            .load_startup_checkpoint_data_with(21, {
                let fetch_calls = fetch_calls.clone();
                move |_| {
                    let fetch_calls = fetch_calls.clone();
                    async move {
                        fetch_calls.fetch_add(1, Ordering::Relaxed);
                        panic!("fetcher should not be called for local checkpoint hits");
                    }
                }
            })
            .await
            .expect("load checkpoint");

        assert_eq!(loaded.summary.sequence_number, 21);
        assert_eq!(fetch_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn load_startup_checkpoint_fetches_and_persists_missing_checkpoint() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let setup = build_test_setup(&tempdir).expect("setup");
        let checkpoint = sample_checkpoint(23);
        let fetch_calls = Arc::new(AtomicUsize::new(0));

        let loaded = setup
            .load_startup_checkpoint_data_with(23, {
                let checkpoint = checkpoint.clone();
                let fetch_calls = fetch_calls.clone();
                move |sequence| {
                    let checkpoint = checkpoint.clone();
                    let fetch_calls = fetch_calls.clone();
                    async move {
                        fetch_calls.fetch_add(1, Ordering::Relaxed);
                        assert_eq!(sequence, 23);
                        Ok(checkpoint)
                    }
                }
            })
            .await
            .expect("load checkpoint");

        assert_eq!(loaded.summary.sequence_number, 23);
        assert_eq!(fetch_calls.load(Ordering::Relaxed), 1);

        let cached = setup
            .store()
            .get_checkpoint_by_sequence_number(23)
            .expect("checkpoint read")
            .expect("checkpoint persisted");
        assert_eq!(cached.summary.sequence_number, 23);

        let loaded_again = setup
            .load_startup_checkpoint_data_with(23, |_| async {
                panic!("persisted checkpoint should not be fetched twice");
            })
            .await
            .expect("load checkpoint again");
        assert_eq!(loaded_again.summary.sequence_number, 23);
    }
}
