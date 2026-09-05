// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context as _;
use anyhow::Result;
use anyhow::anyhow;
use prometheus::Registry;
use rand::rngs::OsRng;
use tokio::sync::RwLock;
use tokio::sync::oneshot;
use tracing::info;

use simulacrum::Simulacrum;
use simulacrum::store::in_mem_store::KeyStore;
use sui_futures::service::Service;
use sui_protocol_config::Chain;
use sui_protocol_config::ProtocolVersion;
use sui_rpc_api::RpcService;
use sui_rpc_api::ServerVersion;
use sui_rpc_api::subscription::SubscriptionService;
use sui_rpc_api::subscription::SubscriptionServiceHandle;
use sui_swarm_config::network_config::NetworkConfig;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::digests::ChainIdentifier;
use sui_types::digests::get_mainnet_chain_identifier;
use sui_types::digests::get_testnet_chain_identifier;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::storage::RpcStateReader;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::sui_system_state::SuiSystemStateTrait;

use crate::Node;
use crate::context::Context;
use crate::gql::CheckpointRead;
use crate::gql::GraphQLClient;
use crate::metadata::MetadataStore;
use crate::proto::forking::forking_service_server::ForkingServiceServer;
use crate::rpc::executor::ForkedTransactionExecutor;
use crate::rpc::forking_service::ForkingServiceImpl;
use crate::seed::SeedInput;
use crate::seed::ensure_seed_manifest_matches;
use crate::seed::load_seed_objects;
use crate::services::ServiceManager;
use crate::store::ForkStore;

/// Checkpoint selected for startup, plus whether existing local fork state was found.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedStartCheckpoint {
    pub(crate) checkpoint: Option<CheckpointSequenceNumber>,
    pub(crate) resuming: bool,
}

/// Resolve the fork checkpoint from local state when possible.
///
/// Explicit data directories can be inspected directly before the remote GraphQL client asks for
/// latest. Default roots cannot be inspected without a checkpoint because their path contains the
/// checkpoint.
pub(crate) fn resolve_start_checkpoint_from_local(
    node: &Node,
    requested_checkpoint: Option<CheckpointSequenceNumber>,
    data_dir: Option<&Path>,
) -> Result<ResolvedStartCheckpoint> {
    let local = match (data_dir, requested_checkpoint) {
        (Some(data_dir), _) => Some(MetadataStore::new_with_root(data_dir.to_path_buf())),
        (None, Some(checkpoint)) => Some(MetadataStore::new(node, checkpoint, None)?),
        (None, None) => None,
    };

    let Some(local) = local else {
        return Ok(ResolvedStartCheckpoint {
            checkpoint: requested_checkpoint,
            resuming: false,
        });
    };

    if local.seed_manifest_exists() {
        let manifest = local.read_seed_manifest()?;
        ensure_seed_manifest_matches(&manifest, &node.network_name(), requested_checkpoint)?;
        return Ok(ResolvedStartCheckpoint {
            checkpoint: Some(manifest.checkpoint),
            resuming: true,
        });
    }

    if let Some(checkpoint) = ServiceManager::existing_forked_checkpoint(
        local.root(),
        &node.network_name(),
        requested_checkpoint,
    )? {
        return Ok(ResolvedStartCheckpoint {
            checkpoint: Some(checkpoint),
            resuming: true,
        });
    }

    Ok(ResolvedStartCheckpoint {
        checkpoint: requested_checkpoint,
        resuming: false,
    })
}

/// Initialize a forked network by fetching the fork checkpoint from the remote endpoint when
/// needed, applying seed metadata, and starting a local Simulacrum instance from the highest
/// checkpoint already persisted locally. Also builds the checkpoint subscription broker and
/// embedded indexer service, which must both be passed to [`run`].
///
/// `data_dir` is the root folder where the fork state is persisted. If `None`, a default path is
/// used. See the `[MetadataStore]` docs for details.
pub async fn initialize(
    node: Node,
    forked_at_checkpoint: CheckpointSequenceNumber,
    version: &str,
    data_dir: Option<PathBuf>,
    seed_input: SeedInput,
) -> Result<(Context, SubscriptionServiceHandle, Service)> {
    // 1. Prepare metadata and GraphQL, then open the RPC store before constructing ForkStore.
    let gql = GraphQLClient::new(node.clone(), version)?;
    let chain_identifier = gql.chain();
    let local = MetadataStore::new(&node, forked_at_checkpoint, data_dir)?;
    let network_name = node.network_name();
    crate::seed::ensure_seed_policy(&local, &seed_input)?;

    // 2. Fetch the startup checkpoint, open the RPC store using its chain identity,
    //    then construct the ForkStore with the RPC store already attached.
    let (checkpoint, checkpoint_contents) = gql
        .get_checkpoint(Some(forked_at_checkpoint))?
        .ok_or_else(|| anyhow!("checkpoint {} not found", forked_at_checkpoint))?;
    let rpc_chain_identifier = fork_chain_identifier(chain_identifier, &checkpoint);
    let mut services = ServiceManager::open(
        local.root(),
        network_name.clone(),
        forked_at_checkpoint,
        rpc_chain_identifier,
    )?;
    let store = ForkStore::from_parts(forked_at_checkpoint, gql, local, services.local_store());
    store.save_checkpoint(&checkpoint, &checkpoint_contents)?;
    let seed_manifest =
        crate::seed::prepare_seed_manifest(&store, network_name, &seed_input).await?;

    // Seeding is eager and one-shot: the enumerations behind the manifest are
    // pinned at the fork checkpoint and cannot be re-run once the fork has
    // diverged, so the whole seed set is loaded here, before anything executes,
    // and never again. On a resumed fork the load is a no-op.
    load_seed_objects(&store, &seed_manifest)?;

    // Resume support: the Simulacrum must build its next checkpoint on the
    // fork's own local tip. On a fresh fork that tip is the fork-point
    // checkpoint persisted just above; on a resumed fork that has advanced,
    // it is the highest locally sealed checkpoint — re-seeding from the fork
    // point would rebuild an already-persisted sequence number with different
    // contents and fail the seal.
    let base_checkpoint = resume_base_checkpoint(&store)?;

    // 3. Read system state — fetches object 0x5 + dynamic fields from remote via ObjectStore.
    let system_state = sui_types::sui_system_state::get_sui_system_state(&store).map_err(|e| {
        anyhow!(
            "failed to read system state at checkpoint {}: {}",
            forked_at_checkpoint,
            e
        )
    })?;

    let protocol_version = system_state.protocol_version();

    // 4. Build NetworkConfig with local test validators.
    let mut rng = OsRng;
    let config = ConfigBuilder::new_with_temp_dir()
        .rng(&mut rng)
        .with_chain_start_timestamp_ms(base_checkpoint.timestamp_ms)
        .deterministic_committee_size(NonZeroUsize::MIN)
        .with_protocol_version(ProtocolVersion::new(protocol_version))
        .with_chain_override(node.chain())
        .build();

    // 5. Override validators in system state with local keys from config.
    let system_state = override_validators(system_state, &config)?;

    // 6. Build KeyStore.
    let keystore = KeyStore::from_network_config(&config);

    // 7. Create Simulacrum from custom state.
    let simulacrum = Simulacrum::new_from_custom_state(
        keystore,
        base_checkpoint,
        system_state,
        rpc_chain_identifier,
        &config,
        store,
        rng,
    );

    // 8. Build the checkpoint subscription broker. The indexer's broadcast pipeline owns the
    //    sender, and `RpcService` receives the handle in `run` so subscribers can register.
    let registry = Registry::new();
    let (checkpoint_sender, subscription_handle) =
        SubscriptionService::build(&registry, None, None, None, None);

    let simulacrum = Arc::new(RwLock::new(simulacrum));
    let indexer_service = services
        .start_indexer(simulacrum.clone(), checkpoint_sender, &registry)
        .await?;
    let context = Context::new(simulacrum, services);

    Ok((context, subscription_handle, indexer_service))
}

fn fork_chain_identifier(chain: Chain, checkpoint: &VerifiedCheckpoint) -> ChainIdentifier {
    match chain {
        Chain::Mainnet => get_mainnet_chain_identifier(),
        Chain::Testnet => get_testnet_chain_identifier(),
        Chain::Unknown => ChainIdentifier::from(*checkpoint.digest()),
    }
}

/// Return the checkpoint the Simulacrum should build on, which is the highest locally persisted
/// checkpoint. On a fresh fork this is the fork-point checkpoint, persisted before this is called.
/// On a resumed fork it is the local tip, so the next sealed checkpoint gets the correct sequence
/// number and previous-digest chain instead of colliding with an already-persisted one.
pub(crate) fn resume_base_checkpoint(store: &ForkStore) -> Result<VerifiedCheckpoint> {
    store
        .get_highest_verified_checkpoint()?
        .ok_or_else(|| anyhow!("no local checkpoint available to resume from"))
}

/// Bind the fork's RPC listener and return bind failures to the caller.
///
/// Binding before initialization reports address conflicts synchronously and preserves the selected
/// address when the caller requests an ephemeral port.
pub(crate) async fn bind(rpc_addr: SocketAddr) -> Result<tokio::net::TcpListener> {
    tokio::net::TcpListener::bind(rpc_addr)
        .await
        .with_context(|| format!("failed to bind fork RPC server to {rpc_addr}"))
}

/// Serve a fork over an already-bound listener and return its managed RPC service.
///
/// The returned service stops accepting connections and drains in-flight requests during graceful
/// shutdown. The RPC task retains the context, while the caller must keep the indexer alive until
/// draining completes. Returns an error if the listener's bound address cannot be read.
pub(crate) async fn serve(
    context: Arc<Context>,
    subscription_handle: SubscriptionServiceHandle,
    listener: tokio::net::TcpListener,
    version: &'static str,
) -> Result<(SocketAddr, Service)> {
    let store = {
        let sim = context.simulacrum().read().await;
        sim.store().clone()
    };
    let reader: Arc<dyn RpcStateReader> = Arc::new(store);

    let mut rpc = RpcService::new(reader);
    rpc.with_server_version(ServerVersion::new("sui-fork", version));
    rpc.with_subscription_service(subscription_handle);
    rpc.with_executor(Arc::new(ForkedTransactionExecutor::new(context.clone())));
    rpc.with_custom_service(ForkingServiceServer::new(ForkingServiceImpl::new(context)));
    rpc.with_file_descriptor_set(crate::proto::FILE_DESCRIPTOR_SET);

    let rpc_addr = listener
        .local_addr()
        .context("failed to read the fork RPC server's bound address")?;
    let router = rpc.into_router().await;
    let (shutdown_sender, shutdown_receiver) = oneshot::channel::<()>();

    info!("starting sui-rpc-api server on {rpc_addr}");
    let service = Service::new()
        .with_shutdown_signal(async move {
            let _ = shutdown_sender.send(());
        })
        .spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_receiver.await;
                    info!("shutdown received, stopping fork RPC server");
                })
                .await
                .context("fork RPC server failed")
        });

    Ok((rpc_addr, service))
}

/// Run the forked network until it receives a process shutdown signal or a service stops.
///
/// Bind `rpc_addr` before starting the RPC service. Process shutdown drains RPC requests before
/// stopping the indexer because accepted requests may need the indexer to publish their
/// checkpoints. Returns an error if binding fails or either service stops unexpectedly.
pub async fn run(
    context: Context,
    subscription_handle: SubscriptionServiceHandle,
    indexer_service: Service,
    rpc_addr: SocketAddr,
    version: &'static str,
) -> Result<()> {
    let listener = bind(rpc_addr).await?;
    run_with_listener(
        context,
        subscription_handle,
        indexer_service,
        listener,
        version,
    )
    .await
}

/// Run the forked network over an already-bound listener.
///
/// Process shutdown drains RPC requests before stopping the indexer because accepted requests may
/// need the indexer to publish their checkpoints. Returns an error if either service stops
/// unexpectedly.
pub(crate) async fn run_with_listener(
    context: Context,
    subscription_handle: SubscriptionServiceHandle,
    mut indexer_service: Service,
    listener: tokio::net::TcpListener,
    version: &'static str,
) -> Result<()> {
    let (_, mut rpc_service) =
        serve(Arc::new(context), subscription_handle, listener, version).await?;

    enum Exit {
        Shutdown,
        Rpc(Result<()>),
        Indexer(Result<()>),
    }

    info!("forked network running, waiting for shutdown signal (Ctrl+C)");
    let exit = tokio::select! {
        result = tokio::signal::ctrl_c() => {
            result?;
            Exit::Shutdown
        }
        result = rpc_service.join() => Exit::Rpc(result),
        result = indexer_service.join() => Exit::Indexer(result),
    };

    match exit {
        Exit::Shutdown => {
            info!("shutdown signal received, stopping forked network");
            let rpc_shutdown = rpc_service.shutdown().await;
            let indexer_shutdown = indexer_service.shutdown().await;
            rpc_shutdown?;
            indexer_shutdown?;
            Ok(())
        }
        Exit::Rpc(Ok(())) => Err(anyhow!("rpc server stopped unexpectedly")),
        Exit::Rpc(Err(error)) => Err(error.context("rpc server failed")),
        Exit::Indexer(Ok(())) => Err(anyhow!("embedded rpc-store indexer stopped unexpectedly")),
        Exit::Indexer(Err(error)) => Err(error.context("embedded rpc-store indexer failed")),
    }
}

/// Replace the validator set in the system state with local validators from the NetworkConfig
/// genesis, so the simulacrum can sign checkpoints with locally available keys.
fn override_validators(
    system_state: SuiSystemState,
    config: &NetworkConfig,
) -> Result<SuiSystemState> {
    let genesis_validators = match config.genesis.sui_system_object() {
        SuiSystemState::V1(inner) => inner.validators,
        SuiSystemState::V2(inner) => inner.validators,
        #[cfg(msim)]
        _ => anyhow::bail!("unsupported genesis system state variant"),
    };

    match system_state {
        SuiSystemState::V1(mut inner) => {
            inner.validators = genesis_validators;
            Ok(SuiSystemState::V1(inner))
        }
        SuiSystemState::V2(mut inner) => {
            inner.validators = genesis_validators;
            Ok(SuiSystemState::V2(inner))
        }
        #[cfg(msim)]
        _ => anyhow::bail!("unsupported system state variant"),
    }
}

#[cfg(test)]
mod tests {
    use sui_types::digests::get_mainnet_chain_identifier;

    use crate::seed::SeedManifest;

    use super::*;

    fn write_manifest(root: &Path, network: &str, checkpoint: CheckpointSequenceNumber) {
        MetadataStore::new_with_root(root.to_path_buf())
            .write_seed_manifest(&SeedManifest {
                network: network.to_owned(),
                checkpoint,
                addresses: Vec::new(),
                entries: Vec::new(),
            })
            .expect("seed manifest should write");
    }

    #[tokio::test]
    async fn bind_reports_address_conflicts() {
        let listener = bind("127.0.0.1:0".parse().expect("valid address"))
            .await
            .expect("ephemeral listener should bind");
        let rpc_addr = listener
            .local_addr()
            .expect("listener should have an address");

        let error = bind(rpc_addr)
            .await
            .expect_err("a second listener must not bind the same address");

        assert!(
            error.to_string().contains("failed to bind fork RPC server"),
            "unexpected error: {error:#}",
        );
    }

    #[test]
    fn resolve_start_checkpoint_uses_manifest_when_checkpoint_is_omitted() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_manifest(temp.path(), "mainnet", 42);

        let resolved = resolve_start_checkpoint_from_local(&Node::Mainnet, None, Some(temp.path()))
            .expect("checkpoint resolution should not fail");

        assert_eq!(
            resolved,
            ResolvedStartCheckpoint {
                checkpoint: Some(42),
                resuming: true,
            }
        );
    }

    #[test]
    fn resolve_start_checkpoint_rejects_requested_checkpoint_mismatch() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_manifest(temp.path(), "mainnet", 42);

        let err = resolve_start_checkpoint_from_local(&Node::Mainnet, Some(43), Some(temp.path()))
            .expect_err("checkpoint mismatch should fail");

        assert!(
            err.to_string()
                .contains("does not match requested checkpoint")
        );
    }

    #[test]
    fn resolve_start_checkpoint_rejects_network_mismatch() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_manifest(temp.path(), "testnet", 42);

        let err = resolve_start_checkpoint_from_local(&Node::Mainnet, None, Some(temp.path()))
            .expect_err("network mismatch should fail");

        assert!(err.to_string().contains("does not match requested network"));
    }

    #[test]
    fn resolve_start_checkpoint_uses_persisted_metadata() {
        let temp = tempfile::tempdir().expect("tempdir");
        ServiceManager::open(
            temp.path(),
            "mainnet".to_owned(),
            17,
            get_mainnet_chain_identifier(),
        )
        .expect("fork metadata should write");

        let resolved = resolve_start_checkpoint_from_local(&Node::Mainnet, None, Some(temp.path()))
            .expect("checkpoint resolution should not fail");

        assert_eq!(
            resolved,
            ResolvedStartCheckpoint {
                checkpoint: Some(17),
                resuming: true,
            }
        );
    }

    #[test]
    fn resolve_start_checkpoint_returns_none_for_fresh_start() {
        let temp = tempfile::tempdir().expect("tempdir");

        let resolved = resolve_start_checkpoint_from_local(&Node::Mainnet, None, Some(temp.path()))
            .expect("checkpoint resolution should not fail");

        assert_eq!(
            resolved,
            ResolvedStartCheckpoint {
                checkpoint: None,
                resuming: false,
            }
        );
    }
}
