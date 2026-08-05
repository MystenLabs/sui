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

use crate::CheckpointRead;
use crate::GraphQLClient;
use crate::Node;
use crate::context::Context;
use crate::metadata::MetadataStore;
use crate::proto::forking::forking_service_server::ForkingServiceServer;
use crate::rpc::executor::ForkedTransactionExecutor;
use crate::rpc::forking_service::ForkingServiceImpl;
use crate::seed::SeedInput;
use crate::seed::ensure_seed_manifest_matches;
use crate::seed::load_seed_objects;
use crate::services::ServiceManager;
use crate::store::ForkStore;

/// An initialized fork that is not yet serving: everything [`initialize`]
/// built, plus the startup facts the serving handle reports.
///
/// The seam between building fork state and serving it — production fills it
/// through [`initialize`], tests fill it by hand over synthetic genesis state.
pub(crate) struct ForkParts {
    pub(crate) context: Context,
    pub(crate) subscription_handle: SubscriptionServiceHandle,
    /// The embedded indexer's tasks; merged into the serving fork's [`Service`].
    pub(crate) indexer_service: Service,
    /// The resolved fork data directory.
    pub(crate) data_dir: PathBuf,
    pub(crate) network_name: String,
    pub(crate) forked_at_checkpoint: CheckpointSequenceNumber,
    /// The fork's local checkpoint tip at initialization: the fork point on a
    /// fresh fork, higher on a resumed one.
    pub(crate) starting_checkpoint: CheckpointSequenceNumber,
}

/// Checkpoint selected for startup, plus whether existing local fork state was found.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedStartCheckpoint {
    pub(crate) checkpoint: Option<CheckpointSequenceNumber>,
    pub(crate) resuming: bool,
}

/// Resolve the checkpoint the fork starts from and whether it resumes existing
/// local state.
///
/// Local knowledge wins: a fork already present in an inspectable data
/// directory is resumed at its recorded checkpoint (a requested checkpoint or
/// network that disagrees with it is an error). Otherwise the requested
/// checkpoint is used, and with no request either, the network's latest
/// checkpoint is fetched from the remote.
pub(crate) async fn resolve_fork_point(
    node: &Node,
    requested_checkpoint: Option<CheckpointSequenceNumber>,
    data_dir: Option<&Path>,
    version: &str,
) -> Result<(CheckpointSequenceNumber, bool)> {
    let resolved = resolve_start_checkpoint_from_local(node, requested_checkpoint, data_dir)?;
    let checkpoint = match resolved.checkpoint {
        Some(checkpoint) => checkpoint,
        None => GraphQLClient::new(node.clone(), version)?
            .get_latest_checkpoint_sequence_number()
            .await?
            .with_context(|| {
                format!(
                    "failed to get latest checkpoint for {}",
                    node.network_name()
                )
            })?,
    };
    Ok((checkpoint, resolved.resuming))
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

/// Initialize a forked network at `forked_at_checkpoint`: open or create the data directory, apply
/// seed metadata, start a local Simulacrum instance from the highest checkpoint already persisted
/// locally, and start the embedded indexer. Also builds the checkpoint subscription broker, whose
/// handle is carried in the returned parts so [`serve`] can expose the streaming RPC.
///
/// Metrics for the subscription broker and the embedded indexer are registered into `registry`.
/// `data_dir` is the root folder where the fork state is persisted. If `None`, a default path is
/// used. See the [`MetadataStore`] docs for details.
pub(crate) async fn initialize(
    node: Node,
    forked_at_checkpoint: CheckpointSequenceNumber,
    version: &str,
    data_dir: Option<PathBuf>,
    seed_input: SeedInput,
    registry: &Registry,
) -> Result<ForkParts> {
    // 1. Prepare metadata and GraphQL, then open the RPC store before constructing ForkStore.
    let gql = GraphQLClient::new(node.clone(), version)?;
    let chain_identifier = gql.chain();
    let local = MetadataStore::new(&node, forked_at_checkpoint, data_dir)?;
    let data_dir = local.root().to_path_buf();
    let network_name = node.network_name();
    crate::seed::ensure_seed_policy(&local, &seed_input)?;

    // 2. Fetch the startup checkpoint, open the RPC store using its chain identity,
    //    then construct the ForkStore with the RPC store already attached.
    let (checkpoint, checkpoint_contents) = gql
        .get_checkpoint(Some(forked_at_checkpoint))?
        .ok_or_else(|| anyhow!("checkpoint {} not found", forked_at_checkpoint))?;
    let rpc_chain_identifier = fork_chain_identifier(chain_identifier, &checkpoint);
    let services = ServiceManager::open(
        local.root(),
        network_name.clone(),
        forked_at_checkpoint,
        rpc_chain_identifier,
    )?;
    let store = ForkStore::from_parts(forked_at_checkpoint, gql, local, services.local_store());
    store.save_checkpoint(&checkpoint, &checkpoint_contents)?;
    let seed_manifest =
        crate::seed::prepare_seed_manifest(&store, network_name.clone(), &seed_input).await?;

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
    let starting_checkpoint = base_checkpoint.data().sequence_number;
    let simulacrum = Simulacrum::new_from_custom_state(
        keystore,
        base_checkpoint,
        system_state,
        &config,
        store,
        rng,
    );

    // 8. Build the checkpoint subscription broker. The sender is owned by
    //    `Context` (producers push here on `advance_checkpoint`); the handle
    //    is wired into `RpcService` in `serve` so subscribers can register.
    let (checkpoint_sender, subscription_handle) =
        SubscriptionService::build(registry, None, None, None, None);

    let (context, indexer_service) =
        Context::new(simulacrum, services, checkpoint_sender, registry).await?;

    Ok(ForkParts {
        context,
        subscription_handle,
        indexer_service,
        data_dir,
        network_name,
        forked_at_checkpoint,
        starting_checkpoint,
    })
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

/// Bind `listen_address` and serve the fork's gRPC surface over it, returning the bound address
/// and the server as a [`Service`].
///
/// The fork binds its own listener rather than using the `sui-rpc-api` serving entry point, so
/// that a requested port 0 resolves to a real ephemeral port the caller can report, and a bind
/// failure is an error here rather than a panic in a background task. The returned [`Service`]
/// carries a shutdown signal that drains the server gracefully. It does not include the embedded
/// indexer, which the caller merges in to tie both lifetimes together.
pub(crate) async fn serve(
    context: Arc<Context>,
    subscription_handle: SubscriptionServiceHandle,
    listen_address: SocketAddr,
    version: &'static str,
    registry: &Registry,
) -> Result<(SocketAddr, Service)> {
    let store = {
        let sim = context.simulacrum().read().await;
        sim.store().clone()
    };
    let reader: Arc<dyn RpcStateReader> = Arc::new(store);

    // Serve through `sui-rpc-api`'s `RpcService` directly (the fork does not
    // depend on `sui-rpc-node`). The `ForkStore` itself is the
    // `RpcStateReader`, with the fork admin service and executor attached.
    let mut service = RpcService::new(reader);
    service.with_server_version(ServerVersion::new("sui-fork", version));
    service.with_metrics(registry);
    service.with_subscription_service(subscription_handle);
    service.with_executor(Arc::new(ForkedTransactionExecutor::new(context.clone())));
    service.with_custom_service(ForkingServiceServer::new(ForkingServiceImpl::new(context)));
    service.with_file_descriptor_set(crate::proto::FILE_DESCRIPTOR_SET);

    let listener = tokio::net::TcpListener::bind(listen_address)
        .await
        .with_context(|| format!("failed to bind fork RPC server to {listen_address}"))?;
    let rpc_address = listener
        .local_addr()
        .context("failed to read the fork RPC server's bound address")?;
    let router = service.into_router().await;

    info!("starting sui-rpc-api server on {rpc_address}");
    let (stx, srx) = oneshot::channel::<()>();
    let service = Service::new()
        .with_shutdown_signal(async move {
            let _ = stx.send(());
        })
        .spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = srx.await;
                    info!("shutdown received, stopping fork RPC server");
                })
                .await
                .context("fork RPC server failed")
        });

    Ok((rpc_address, service))
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
