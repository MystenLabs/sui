// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use anyhow::anyhow;
use prometheus::Registry;
use rand::rngs::OsRng;
use tracing::info;

use simulacrum::Simulacrum;
use simulacrum::store::in_mem_store::KeyStore;
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
use crate::rpc::reader::RpcReader;
use crate::seed::SeedInput;
use crate::seed::ensure_seed_manifest_matches;
use crate::seed::save_seed_manifest_objects;
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
/// Explicit data directories can be inspected directly before the remote
/// GraphQL client asks for latest. Default roots cannot be inspected without a
/// checkpoint because their path contains the checkpoint.
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

/// Initialize a forked network by fetching the fork checkpoint from the remote
/// endpoint when needed, applying seed metadata, and starting a local Simulacrum
/// instance from the highest checkpoint already persisted locally. Also builds
/// the checkpoint subscription broker; the returned handle must be passed to
/// [`run`] so the gRPC server exposes the streaming RPC.
///
/// `data_dir` is the root folder where the fork state is persisted. If `None`, a default path is
/// used. See the `[MetadataStore]` docs for details.
pub async fn initialize(
    node: Node,
    forked_at_checkpoint: CheckpointSequenceNumber,
    version: &str,
    data_dir: Option<PathBuf>,
    seed_input: SeedInput,
) -> Result<(Context, SubscriptionServiceHandle)> {
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
    let services = ServiceManager::open(
        local.root(),
        network_name.clone(),
        forked_at_checkpoint,
        rpc_chain_identifier,
    )?;
    let store = ForkStore::from_parts(forked_at_checkpoint, gql, local, services.local_store());
    store.save_checkpoint(&checkpoint, &checkpoint_contents)?;
    let seed_manifest =
        crate::seed::prepare_seed_manifest(&store, network_name, &seed_input).await?;
    save_seed_manifest_objects(&store, &seed_manifest)?;

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
        &config,
        store,
        rng,
    );

    // 8. Build the checkpoint subscription broker. The sender is owned by
    //    `Context` (producers push here on `advance_checkpoint`); the handle
    //    is wired into `RpcService` in `run` so subscribers can register.
    let registry = Registry::new();
    let (checkpoint_sender, subscription_handle) =
        SubscriptionService::build(&registry, None, None, None, None);

    let context =
        Context::new_with_services(simulacrum, services, checkpoint_sender, &registry).await?;

    Ok((context, subscription_handle))
}

fn fork_chain_identifier(chain: Chain, checkpoint: &VerifiedCheckpoint) -> ChainIdentifier {
    match chain {
        Chain::Mainnet => get_mainnet_chain_identifier(),
        Chain::Testnet => get_testnet_chain_identifier(),
        Chain::Unknown => ChainIdentifier::from(*checkpoint.digest()),
    }
}

/// Return the checkpoint the Simulacrum should build on: the highest locally
/// persisted checkpoint. On a fresh fork this is the fork-point checkpoint
/// (persisted before this is called); on a resumed fork it is the local tip,
/// so the next sealed checkpoint gets the correct sequence number and
/// previous-digest chain instead of colliding with an already-persisted one.
pub(crate) fn resume_base_checkpoint(store: &ForkStore) -> Result<VerifiedCheckpoint> {
    store
        .get_highest_verified_checkpoint()?
        .ok_or_else(|| anyhow!("no local checkpoint available to resume from"))
}

/// Run the forked network. Spawns the `sui-rpc-api` `RpcService` bound to
/// `rpc_addr`, backed by `RpcReader`, then blocks on Ctrl+C.
pub async fn run(
    context: Context,
    subscription_handle: SubscriptionServiceHandle,
    rpc_addr: SocketAddr,
    version: &'static str,
) -> Result<()> {
    let store = {
        let sim = context.simulacrum().read().await;
        sim.store().clone()
    };
    let reader: Arc<dyn RpcStateReader> = Arc::new(RpcReader::new(store));

    // Serve through `sui-rpc-api`'s `RpcService` directly (the fork does not
    // depend on `sui-rpc-node`). The rpc-store-backed `RpcReader` is the
    // `RpcStateReader`, with the fork admin service and executor attached.
    let mut service = RpcService::new(reader);
    service.with_server_version(ServerVersion::new("sui-fork", version));
    service.with_subscription_service(subscription_handle);
    let context = Arc::new(context);
    service.with_executor(Arc::new(ForkedTransactionExecutor::new(context.clone())));
    service.with_custom_service(ForkingServiceServer::new(ForkingServiceImpl::new(
        context.clone(),
    )));
    service.with_file_descriptor_set(crate::proto::FILE_DESCRIPTOR_SET);

    info!("starting sui-rpc-api server on {rpc_addr}");
    let server_handle = tokio::spawn(async move { service.start_service(rpc_addr).await });

    info!("forked network running, waiting for shutdown signal (Ctrl+C)");
    tokio::select! {
        res = tokio::signal::ctrl_c() => {
            res?;
            info!("shutdown signal received, stopping forked network");
        }
        join = server_handle => {
            if let Err(e) = join {
                return Err(anyhow!("rpc server task panicked: {e}"));
            }
            return Err(anyhow!("rpc server task exited unexpectedly"));
        }
        stopped = context.indexer_stopped() => {
            // Without this watchdog an indexer failure would only surface as
            // a 30s publication timeout on the next executed transaction.
            return match stopped {
                Ok(()) => Err(anyhow!("embedded rpc-store indexer stopped unexpectedly")),
                Err(e) => Err(e.context("embedded rpc-store indexer failed")),
            };
        }
    }
    Ok(())
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
