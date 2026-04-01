// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use axum::Router;
use axum::routing::get;
use forking_data_store::stores::{
    CHECKPOINT_DIR, CHECKPOINT_FILE_EXTENSION, FORK_DIRECTORY_PREFIX,
};
use forking_data_store::{CheckpointStore, EpochData, EpochStore, SetupStore};
use rand::rngs::OsRng;
use tokio::sync::RwLock;
use tokio::sync::oneshot;
use tower_http::cors::CorsLayer;
use tracing::{info, warn};

use simulacrum::Simulacrum;
use simulacrum::store::SimulatorStore;
use simulacrum::store::in_mem_store::KeyStore;
use sui_rpc::proto::sui::rpc::v2::ledger_service_server::LedgerServiceServer;
use sui_rpc_api::grpc::Services as GrpcServices;
use sui_swarm_config::network_config::NetworkConfig;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::full_checkpoint_content::Checkpoint as CheckpointData;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::sui_system_state::{SuiSystemState, SuiSystemStateTrait};

use crate::grpc::ledger_service::ForkingLedgerService;

mod endpoints {
    pub const HEALTH: &str = "/health";
    pub const STATUS: &str = "/status";
}

/// Startup data resolved before the forked network is created.
pub struct StartupContext {
    /// The canonical short chain identifier for the source network.
    pub chain_id: String,
    /// The checkpoint used to initialize simulacrum.
    pub checkpoint: CheckpointData,
    /// Epoch metadata for the startup checkpoint epoch.
    pub epoch_data: EpochData,
    /// The original fork checkpoint for this fork session.
    pub fork_origin_checkpoint: u64,
}

/// Resolve an existing fork session to resume from a requested startup checkpoint.
///
/// This scans `<root>/<chain_id>/forked_at_<origin>/checkpoint/<requested>.binpb.zst` and
/// returns the unique matching `origin` if one exists.
pub fn resolve_resume_fork_checkpoint(
    store_root: &Path,
    startup_checkpoint: Option<u64>,
) -> Result<Option<u64>> {
    let Some(startup_checkpoint) = startup_checkpoint else {
        return Ok(None);
    };

    let checkpoint_file = format!("{startup_checkpoint}.{CHECKPOINT_FILE_EXTENSION}");
    let mut matches = Vec::new();

    if !store_root.exists() {
        return Ok(None);
    }

    for chain_dir in fs::read_dir(store_root)
        .with_context(|| format!("failed to read store root {}", store_root.display()))?
    {
        let chain_dir = chain_dir
            .with_context(|| format!("failed to iterate store root {}", store_root.display()))?;
        if !chain_dir.file_type()?.is_dir() {
            continue;
        }

        for fork_dir in fs::read_dir(chain_dir.path()).with_context(|| {
            format!(
                "failed to read chain directory {}",
                chain_dir.path().display()
            )
        })? {
            let fork_dir = fork_dir.with_context(|| {
                format!(
                    "failed to iterate chain directory {}",
                    chain_dir.path().display()
                )
            })?;
            if !fork_dir.file_type()?.is_dir() {
                continue;
            }

            let Some(fork_name) = fork_dir.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Some(origin) = fork_name
                .strip_prefix(FORK_DIRECTORY_PREFIX)
                .and_then(|value| value.parse::<u64>().ok())
            else {
                continue;
            };

            let checkpoint_path = fork_dir.path().join(CHECKPOINT_DIR).join(&checkpoint_file);
            if checkpoint_path.exists() {
                matches.push(origin);
            }
        }
    }

    match matches.as_slice() {
        [] => Ok(None),
        [origin] => Ok(Some(*origin)),
        origins => Err(anyhow!(
            "startup checkpoint {startup_checkpoint} exists in multiple fork sessions: {origins:?}"
        )),
    }
}

/// Start the forking server.
///
/// `store_factory` receives the resolved startup context together with the
/// generated network config and returns the simulator store implementation to use.
pub async fn start_server<S, B>(
    bootstrap_store: &B,
    startup_checkpoint: Option<u64>,
    fork_origin_checkpoint: Option<u64>,
    store_factory: impl FnOnce(&StartupContext, &NetworkConfig) -> Result<S>,
    host: &str,
    server_port: u16,
    shutdown_receiver: Option<oneshot::Receiver<()>>,
    ready_sender: Option<oneshot::Sender<SocketAddr>>,
) -> Result<()>
where
    S: SimulatorStore + Send + Sync + 'static,
    B: CheckpointStore + EpochStore + SetupStore,
{
    let chain_id = bootstrap_store
        .setup(None)?
        .ok_or_else(|| anyhow!("bootstrap store did not resolve a chain identifier"))?;
    let checkpoint = load_startup_checkpoint(bootstrap_store, startup_checkpoint)?;
    let epoch_data = load_startup_epoch_data(bootstrap_store, &checkpoint)?;
    let fork_origin_checkpoint =
        fork_origin_checkpoint.unwrap_or(checkpoint.summary.data().sequence_number);
    let verified_checkpoint = VerifiedCheckpoint::new_unchecked(checkpoint.summary.clone());

    let context = StartupContext {
        chain_id,
        checkpoint,
        epoch_data,
        fork_origin_checkpoint,
    };
    let config = build_network_config(context.epoch_data.protocol_version);
    let store = store_factory(&context, &config)?;
    let simulacrum = Arc::new(RwLock::new(setup_simulacrum(
        verified_checkpoint,
        &config,
        store,
    )?));
    let app = build_router(simulacrum);
    serve(app, host, server_port, shutdown_receiver, ready_sender).await
}

fn load_startup_checkpoint<B: CheckpointStore>(
    bootstrap_store: &B,
    startup_checkpoint: Option<u64>,
) -> Result<CheckpointData> {
    let checkpoint = match startup_checkpoint {
        Some(sequence_number) => bootstrap_store
            .get_checkpoint_by_sequence_number(sequence_number)?
            .ok_or_else(|| anyhow!("failed to fetch checkpoint {sequence_number}"))?,
        None => bootstrap_store
            .get_latest_checkpoint()?
            .ok_or_else(|| anyhow!("failed to fetch latest checkpoint"))?,
    };

    Ok(checkpoint)
}

fn load_startup_epoch_data<B: EpochStore>(
    bootstrap_store: &B,
    checkpoint: &CheckpointData,
) -> Result<EpochData> {
    let epoch = checkpoint.summary.data().epoch;
    bootstrap_store
        .epoch_info(epoch)?
        .ok_or_else(|| anyhow!("failed to fetch epoch data for epoch {epoch}"))
}

fn build_network_config(protocol_version: u64) -> NetworkConfig {
    ConfigBuilder::new_with_temp_dir()
        .rng(OsRng)
        .with_chain_start_timestamp_ms(0)
        .deterministic_committee_size(NonZeroUsize::MIN)
        .with_protocol_version(protocol_version.into())
        .build()
}

fn setup_simulacrum<S: SimulatorStore>(
    checkpoint: VerifiedCheckpoint,
    config: &NetworkConfig,
    store: S,
) -> Result<Simulacrum<OsRng, S>> {
    let rng = OsRng;
    let keystore = KeyStore::from_network_config(config);
    let system_state = fetch_system_state(config);

    println!(
        "Starting forking server from checkpoint {} with protocol version {}",
        checkpoint.sequence_number(),
        config.genesis.sui_system_object().protocol_version(),
    );

    Ok(Simulacrum::new_from_custom_state(
        keystore,
        checkpoint,
        system_state,
        config,
        store,
        rng,
    ))
}

/// Fetch the SuiSystemState from the genesis config. This is needed to initialize the Simulacrum.
///
/// Note this will be changed in future PRs to correctly fetch the SuiSystemState from the network.
fn fetch_system_state(config: &NetworkConfig) -> SuiSystemState {
    config.genesis.sui_system_object()
}

pub(crate) fn build_router<S>(simulacrum: Arc<RwLock<Simulacrum<OsRng, S>>>) -> Router
where
    S: SimulatorStore + Send + Sync + 'static,
{
    let grpc_router = GrpcServices::new()
        .add_service(LedgerServiceServer::new(ForkingLedgerService::new(
            simulacrum,
        )))
        .into_router();

    Router::new()
        .route(endpoints::HEALTH, get(health))
        .route(endpoints::STATUS, get(get_status))
        .merge(grpc_router)
        .layer(CorsLayer::permissive())
}

async fn serve(
    app: Router,
    host: &str,
    server_port: u16,
    shutdown_receiver: Option<oneshot::Receiver<()>>,
    ready_sender: Option<oneshot::Sender<SocketAddr>>,
) -> Result<()> {
    let addr: SocketAddr = format!("{host}:{server_port}").parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let local_addr = listener.local_addr()?;

    println!("Forking server listening on {local_addr}");

    if let Some(ready_sender) = ready_sender {
        let _ = ready_sender.send(local_addr);
    }

    let shutdown_signal = async move {
        match shutdown_receiver {
            Some(receiver) => {
                let ctrl_c = async {
                    match tokio::signal::ctrl_c().await {
                        Ok(()) => {
                            info!("\nReceived CTRL+C, shutting down gracefully...");
                        }
                        Err(err) => {
                            warn!("Failed to install CTRL+C signal handler: {err}");
                            std::future::pending::<()>().await;
                        }
                    }
                };

                tokio::select! {
                    _ = receiver => {
                        info!("received programmatic shutdown signal");
                    }
                    _ = ctrl_c => {
                    }
                }
            }
            None => {
                if let Err(err) = tokio::signal::ctrl_c().await {
                    warn!("Failed to install CTRL+C signal handler: {err}");
                    return;
                }
                info!("\nReceived CTRL+C, shutting down gracefully...");
            }
        }
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await?;

    info!("Server shutdown complete");

    Ok(())
}

async fn health() -> &'static str {
    "OK"
}

async fn get_status() -> &'static str {
    "OK"
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::ServiceStore;
    use forking_data_store::{
        CheckpointStore, CheckpointStoreWriter, EpochData, EpochStore, EpochStoreWriter,
        MockCheckpointStore, MockEpochStore, Node, SetupStore,
        stores::{
            CHECKPOINT_DIR, CHECKPOINT_FILE_EXTENSION, DATA_STORE_DIR, FileSystemStore,
            ForkingStore, ReadThroughStore,
        },
    };
    use mockall::predicate::eq;
    use sui_types::committee::ProtocolVersion;
    use sui_types::digests::MAINNET_CHAIN_IDENTIFIER_BASE58;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    type BootstrapStore = ForkingStore<
        FileSystemStore,
        ReadThroughStore<FileSystemStore, ReadThroughStore<FileSystemStore, TestEpochStore>>,
        FileSystemStore,
        ReadThroughStore<FileSystemStore, ReadThroughStore<FileSystemStore, TestCheckpointStore>>,
    >;
    type RuntimeStore =
        ForkingStore<FileSystemStore, FileSystemStore, FileSystemStore, FileSystemStore>;

    struct TestCheckpointStore {
        chain_id: String,
        inner: MockCheckpointStore,
    }

    impl TestCheckpointStore {
        fn new(chain_id: String, inner: MockCheckpointStore) -> Self {
            Self { chain_id, inner }
        }
    }

    impl CheckpointStore for TestCheckpointStore {
        fn get_checkpoint_by_sequence_number(
            &self,
            sequence: u64,
        ) -> Result<Option<CheckpointData>> {
            self.inner.get_checkpoint_by_sequence_number(sequence)
        }

        fn get_latest_checkpoint(&self) -> Result<Option<CheckpointData>> {
            self.inner.get_latest_checkpoint()
        }

        fn get_sequence_by_checkpoint_digest(
            &self,
            digest: &sui_types::digests::CheckpointDigest,
        ) -> Result<Option<u64>> {
            self.inner.get_sequence_by_checkpoint_digest(digest)
        }

        fn get_sequence_by_contents_digest(
            &self,
            digest: &sui_types::digests::CheckpointContentsDigest,
        ) -> Result<Option<u64>> {
            self.inner.get_sequence_by_contents_digest(digest)
        }
    }

    impl SetupStore for TestCheckpointStore {
        fn setup(&self, chain_id: Option<String>) -> Result<Option<String>> {
            Ok(chain_id.or_else(|| Some(self.chain_id.clone())))
        }
    }

    struct TestEpochStore {
        chain_id: String,
        inner: MockEpochStore,
    }

    impl TestEpochStore {
        fn new(chain_id: String, inner: MockEpochStore) -> Self {
            Self { chain_id, inner }
        }
    }

    impl EpochStore for TestEpochStore {
        fn epoch_info(&self, epoch: u64) -> Result<Option<EpochData>> {
            self.inner.epoch_info(epoch)
        }

        fn protocol_config(
            &self,
            epoch: u64,
        ) -> Result<Option<sui_types::supported_protocol_versions::ProtocolConfig>> {
            self.inner.protocol_config(epoch)
        }
    }

    impl SetupStore for TestEpochStore {
        fn setup(&self, chain_id: Option<String>) -> Result<Option<String>> {
            Ok(chain_id.or_else(|| Some(self.chain_id.clone())))
        }
    }

    fn test_checkpoint(sequence: u64, epoch: u64) -> CheckpointData {
        TestCheckpointBuilder::new(sequence)
            .with_epoch(epoch)
            .build_checkpoint()
    }

    fn test_epoch_data(epoch: u64, protocol_version: u64) -> EpochData {
        EpochData {
            epoch_id: epoch,
            protocol_version,
            rgp: 1,
            start_timestamp: 0,
        }
    }

    fn checkpoint_file_path(
        store_root: &Path,
        chain_id: &str,
        fork_origin_checkpoint: u64,
        sequence: u64,
    ) -> PathBuf {
        store_root
            .join(chain_id)
            .join(FileSystemStore::fork_directory_name(fork_origin_checkpoint))
            .join(CHECKPOINT_DIR)
            .join(format!("{sequence}.{CHECKPOINT_FILE_EXTENSION}"))
    }

    fn filesystem_store(root: &Path, fork_origin_checkpoint: Option<u64>) -> FileSystemStore {
        match fork_origin_checkpoint {
            Some(fork_origin_checkpoint) => FileSystemStore::new_with_path_for_fork(
                Node::Mainnet,
                root.to_path_buf(),
                fork_origin_checkpoint,
            )
            .unwrap(),
            None => FileSystemStore::new_with_path(Node::Mainnet, root.to_path_buf()).unwrap(),
        }
    }

    fn bootstrap_store(
        root: &Path,
        startup_checkpoint: Option<u64>,
        resume_fork_origin: Option<u64>,
        checkpoint_store: TestCheckpointStore,
        epoch_store: TestEpochStore,
    ) -> BootstrapStore {
        let bootstrap_fork_origin =
            startup_checkpoint.map(|sequence| resume_fork_origin.unwrap_or(sequence));
        ForkingStore::new(
            filesystem_store(root, None),
            ReadThroughStore::new(
                filesystem_store(root, bootstrap_fork_origin),
                ReadThroughStore::new(filesystem_store(root, None), epoch_store),
            ),
            filesystem_store(root, None),
            ReadThroughStore::new(
                filesystem_store(root, bootstrap_fork_origin),
                ReadThroughStore::new(filesystem_store(root, None), checkpoint_store),
            ),
        )
    }

    fn filesystem_runtime_store(root: &Path, startup: &StartupContext) -> RuntimeStore {
        let transactions = filesystem_store(root, Some(startup.fork_origin_checkpoint));
        transactions.setup(Some(startup.chain_id.clone())).unwrap();
        let epochs = filesystem_store(root, Some(startup.fork_origin_checkpoint));
        epochs.setup(Some(startup.chain_id.clone())).unwrap();
        epochs
            .write_epoch_info(startup.epoch_data.epoch_id, startup.epoch_data.clone())
            .unwrap();
        let objects = filesystem_store(root, Some(startup.fork_origin_checkpoint));
        objects.setup(Some(startup.chain_id.clone())).unwrap();
        let checkpoints = filesystem_store(root, Some(startup.fork_origin_checkpoint));
        checkpoints.setup(Some(startup.chain_id.clone())).unwrap();
        checkpoints.write_checkpoint(&startup.checkpoint).unwrap();
        ForkingStore::new(transactions, epochs, objects, checkpoints)
    }

    fn service_store(
        root: &Path,
        startup: &StartupContext,
    ) -> ServiceStore<RuntimeStore, RuntimeStore> {
        ServiceStore::new(
            startup.fork_origin_checkpoint,
            filesystem_runtime_store(root, startup),
            filesystem_runtime_store(root, startup),
        )
    }

    async fn spawn_server<B>(
        bootstrap_store: B,
        startup_checkpoint: Option<u64>,
        resume_fork_origin: Option<u64>,
        store_factory: impl FnOnce(
            &StartupContext,
            &NetworkConfig,
        ) -> Result<ServiceStore<RuntimeStore, RuntimeStore>>
        + Send
        + 'static,
    ) -> (SocketAddr, oneshot::Sender<()>, tokio::task::JoinHandle<()>)
    where
        B: CheckpointStore + EpochStore + SetupStore + Sync + Send + 'static,
    {
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let (ready_tx, ready_rx) = oneshot::channel::<SocketAddr>();

        let server = tokio::spawn(async move {
            start_server(
                &bootstrap_store,
                startup_checkpoint,
                resume_fork_origin,
                store_factory,
                "127.0.0.1",
                0,
                Some(shutdown_rx),
                Some(ready_tx),
            )
            .await
            .unwrap();
        });

        let addr = ready_rx.await.expect("server should signal ready");
        (addr, shutdown_tx, server)
    }

    #[tokio::test]
    async fn test_server_health_and_status() {
        let tempdir = tempfile::tempdir().unwrap();
        let store_root = tempdir.path().join(DATA_STORE_DIR);
        let checkpoint = test_checkpoint(7, 3);
        let epoch_data = test_epoch_data(3, ProtocolVersion::MAX.as_u64());
        let chain_id = MAINNET_CHAIN_IDENTIFIER_BASE58.to_string();

        let mut checkpoint_store = MockCheckpointStore::new();
        checkpoint_store
            .expect_get_checkpoint_by_sequence_number()
            .with(eq(7))
            .times(1)
            .return_once(move |_| Ok(Some(checkpoint)));
        checkpoint_store
            .expect_get_latest_checkpoint()
            .times(0)
            .returning(|| Ok(None));
        checkpoint_store
            .expect_get_sequence_by_checkpoint_digest()
            .times(0)
            .returning(|_| Ok(None));
        checkpoint_store
            .expect_get_sequence_by_contents_digest()
            .times(0)
            .returning(|_| Ok(None));

        let mut epoch_store = MockEpochStore::new();
        epoch_store
            .expect_epoch_info()
            .with(eq(3))
            .times(1)
            .return_once(move |_| Ok(Some(epoch_data)));
        epoch_store
            .expect_protocol_config()
            .times(0)
            .returning(|_| Ok(None));

        let bootstrap_store = bootstrap_store(
            &store_root,
            Some(7),
            None,
            TestCheckpointStore::new(chain_id.clone(), checkpoint_store),
            TestEpochStore::new(chain_id.clone(), epoch_store),
        );
        let (addr, shutdown_tx, server) = spawn_server(bootstrap_store, Some(7), None, {
            let store_root = store_root.clone();
            move |startup, _config| Ok(service_store(&store_root, startup))
        })
        .await;

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://{addr}/health"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), "OK");

        let resp = client
            .get(format!("http://{addr}/status"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), "OK");

        shutdown_tx.send(()).unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn test_startup_downloads_checkpoint_and_persists_it_to_disk() {
        let tempdir = tempfile::tempdir().unwrap();
        let store_root = tempdir.path().join(DATA_STORE_DIR);
        let chain_id = MAINNET_CHAIN_IDENTIFIER_BASE58.to_string();
        let short_chain_id = "35834a8a";
        let checkpoint = test_checkpoint(7, 3);
        let epoch_data = test_epoch_data(3, ProtocolVersion::MAX.as_u64());

        let mut checkpoint_store = MockCheckpointStore::new();
        checkpoint_store
            .expect_get_checkpoint_by_sequence_number()
            .with(eq(7))
            .times(1)
            .return_once({
                let checkpoint = checkpoint.clone();
                move |_| Ok(Some(checkpoint))
            });
        checkpoint_store
            .expect_get_latest_checkpoint()
            .times(0)
            .returning(|| Ok(None));
        checkpoint_store
            .expect_get_sequence_by_checkpoint_digest()
            .times(0)
            .returning(|_| Ok(None));
        checkpoint_store
            .expect_get_sequence_by_contents_digest()
            .times(0)
            .returning(|_| Ok(None));

        let mut epoch_store = MockEpochStore::new();
        epoch_store
            .expect_epoch_info()
            .with(eq(3))
            .times(1)
            .return_once(move |_| Ok(Some(epoch_data)));
        epoch_store
            .expect_protocol_config()
            .times(0)
            .returning(|_| Ok(None));

        let bootstrap_store = bootstrap_store(
            &store_root,
            Some(7),
            None,
            TestCheckpointStore::new(chain_id.clone(), checkpoint_store),
            TestEpochStore::new(chain_id.clone(), epoch_store),
        );
        let (addr, shutdown_tx, server) = spawn_server(bootstrap_store, Some(7), None, {
            let store_root = store_root.clone();
            move |startup, _config| Ok(service_store(&store_root, startup))
        })
        .await;
        shutdown_tx.send(()).unwrap();
        server.await.unwrap();

        let client = reqwest::Client::new();
        let resp = client.get(format!("http://{addr}/health")).send().await;
        assert!(
            resp.is_err(),
            "server should be shut down before verification"
        );

        let checkpoint_path = checkpoint_file_path(&store_root, short_chain_id, 7, 7);
        assert!(checkpoint_path.exists(), "checkpoint file should be saved");

        let filesystem_store =
            FileSystemStore::new_with_path_for_fork(Node::Mainnet, store_root, 7).unwrap();
        assert_eq!(
            filesystem_store.setup(None).unwrap(),
            Some(short_chain_id.to_string())
        );
        assert!(
            filesystem_store
                .get_checkpoint_by_sequence_number(7)
                .unwrap()
                .is_some()
        );
        assert!(filesystem_store.epoch_info(3).unwrap().is_some());
    }

    #[tokio::test]
    async fn test_restart_uses_filesystem_cache_without_remote() {
        let tempdir = tempfile::tempdir().unwrap();
        let store_root = tempdir.path().join(DATA_STORE_DIR);
        let chain_id = MAINNET_CHAIN_IDENTIFIER_BASE58.to_string();
        let short_chain_id = "35834a8a";
        let checkpoint = test_checkpoint(7, 3);
        let epoch_data = test_epoch_data(3, ProtocolVersion::MAX.as_u64());

        let mut checkpoint_store = MockCheckpointStore::new();
        checkpoint_store
            .expect_get_checkpoint_by_sequence_number()
            .with(eq(7))
            .times(1)
            .return_once({
                let checkpoint = checkpoint.clone();
                move |_| Ok(Some(checkpoint))
            });
        checkpoint_store
            .expect_get_latest_checkpoint()
            .times(0)
            .returning(|| Ok(None));
        checkpoint_store
            .expect_get_sequence_by_checkpoint_digest()
            .times(0)
            .returning(|_| Ok(None));
        checkpoint_store
            .expect_get_sequence_by_contents_digest()
            .times(0)
            .returning(|_| Ok(None));

        let mut epoch_store = MockEpochStore::new();
        epoch_store
            .expect_epoch_info()
            .with(eq(3))
            .times(1)
            .return_once(move |_| Ok(Some(epoch_data)));
        epoch_store
            .expect_protocol_config()
            .times(0)
            .returning(|_| Ok(None));

        let first_boot_store = bootstrap_store(
            &store_root,
            Some(7),
            None,
            TestCheckpointStore::new(chain_id.clone(), checkpoint_store),
            TestEpochStore::new(chain_id.clone(), epoch_store),
        );
        let (_, shutdown_tx, server) = spawn_server(first_boot_store, Some(7), None, {
            let store_root = store_root.clone();
            move |startup, _config| Ok(service_store(&store_root, startup))
        })
        .await;
        shutdown_tx.send(()).unwrap();
        server.await.unwrap();

        let resume_fork_origin = resolve_resume_fork_checkpoint(&store_root, Some(7)).unwrap();
        let second_boot_store = ForkingStore::new(
            filesystem_store(&store_root, resume_fork_origin),
            filesystem_store(&store_root, resume_fork_origin),
            filesystem_store(&store_root, resume_fork_origin),
            filesystem_store(&store_root, resume_fork_origin),
        );

        let (addr, shutdown_tx, server) =
            spawn_server(second_boot_store, Some(7), resume_fork_origin, {
                let store_root = store_root.clone();
                move |startup, _config| Ok(service_store(&store_root, startup))
            })
            .await;

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://{addr}/health"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        shutdown_tx.send(()).unwrap();
        server.await.unwrap();

        let filesystem_store =
            FileSystemStore::new_with_path_for_fork(Node::Mainnet, store_root, 7).unwrap();
        assert_eq!(
            filesystem_store.setup(None).unwrap(),
            Some(short_chain_id.to_string())
        );
        assert!(
            filesystem_store
                .get_checkpoint_by_sequence_number(7)
                .unwrap()
                .is_some()
        );
        assert!(filesystem_store.epoch_info(3).unwrap().is_some());
    }

    #[tokio::test]
    async fn test_restart_from_local_checkpoint_uses_original_fork_directory() {
        let tempdir = tempfile::tempdir().unwrap();
        let store_root = tempdir.path().join(DATA_STORE_DIR);
        let chain_id = MAINNET_CHAIN_IDENTIFIER_BASE58.to_string();
        let short_chain_id = "35834a8a";
        let checkpoint = test_checkpoint(7, 3);
        let local_checkpoint = test_checkpoint(9, 3);
        let epoch_data = test_epoch_data(3, ProtocolVersion::MAX.as_u64());

        let mut checkpoint_store = MockCheckpointStore::new();
        checkpoint_store
            .expect_get_checkpoint_by_sequence_number()
            .with(eq(7))
            .times(1)
            .return_once({
                let checkpoint = checkpoint.clone();
                move |_| Ok(Some(checkpoint))
            });
        checkpoint_store
            .expect_get_latest_checkpoint()
            .times(0)
            .returning(|| Ok(None));
        checkpoint_store
            .expect_get_sequence_by_checkpoint_digest()
            .times(0)
            .returning(|_| Ok(None));
        checkpoint_store
            .expect_get_sequence_by_contents_digest()
            .times(0)
            .returning(|_| Ok(None));

        let mut epoch_store = MockEpochStore::new();
        epoch_store
            .expect_epoch_info()
            .with(eq(3))
            .times(1)
            .return_once(move |_| Ok(Some(epoch_data)));
        epoch_store
            .expect_protocol_config()
            .times(0)
            .returning(|_| Ok(None));

        let first_boot_store = bootstrap_store(
            &store_root,
            Some(7),
            None,
            TestCheckpointStore::new(chain_id.clone(), checkpoint_store),
            TestEpochStore::new(chain_id.clone(), epoch_store),
        );
        let (_, shutdown_tx, server) = spawn_server(first_boot_store, Some(7), None, {
            let store_root = store_root.clone();
            move |startup, _config| Ok(service_store(&store_root, startup))
        })
        .await;
        shutdown_tx.send(()).unwrap();
        server.await.unwrap();

        let local_store =
            FileSystemStore::new_with_path_for_fork(Node::Mainnet, store_root.clone(), 7).unwrap();
        assert_eq!(
            local_store.setup(None).unwrap(),
            Some(short_chain_id.to_string())
        );
        local_store.write_checkpoint(&local_checkpoint).unwrap();

        let resume_fork_origin = resolve_resume_fork_checkpoint(&store_root, Some(9)).unwrap();
        assert_eq!(resume_fork_origin, Some(7));

        let second_boot_store = ForkingStore::new(
            filesystem_store(&store_root, resume_fork_origin),
            filesystem_store(&store_root, resume_fork_origin),
            filesystem_store(&store_root, resume_fork_origin),
            filesystem_store(&store_root, resume_fork_origin),
        );

        let (addr, shutdown_tx, server) =
            spawn_server(second_boot_store, Some(9), resume_fork_origin, {
                let store_root = store_root.clone();
                move |startup, _config| {
                    assert_eq!(startup.fork_origin_checkpoint, 7);
                    assert_eq!(startup.checkpoint.summary.data().sequence_number, 9);
                    Ok(service_store(&store_root, startup))
                }
            })
            .await;

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://{addr}/health"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        shutdown_tx.send(()).unwrap();
        server.await.unwrap();

        let resumed_checkpoint_path = checkpoint_file_path(&store_root, short_chain_id, 7, 9);
        assert!(resumed_checkpoint_path.exists());
    }
}
