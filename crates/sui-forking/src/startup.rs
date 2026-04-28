// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use prometheus::Registry;
use rand::rngs::OsRng;
use tracing::info;

use simulacrum::Simulacrum;
use simulacrum::store::in_mem_store::KeyStore;
use sui_protocol_config::ProtocolVersion;
use sui_rpc_api::subscription::{SubscriptionService, SubscriptionServiceHandle};
use sui_rpc_api::{RpcService, ServerVersion};
use sui_swarm_config::network_config::NetworkConfig;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::storage::RpcStateReader;
use sui_types::sui_system_state::{SuiSystemState, SuiSystemStateTrait};

use crate::Node;
use crate::context::Context;
use crate::proto::forking::forking_service_server::ForkingServiceServer;
use crate::rpc::executor::ForkedTransactionExecutor;
use crate::rpc::forking_service::ForkingServiceImpl;
use crate::seed::SeedInput;
use crate::store::DataStore;

/// Initialize a forked network by fetching the fork checkpoint from the remote
/// endpoint when needed, applying seed metadata, and starting a local Simulacrum
/// instance from the highest checkpoint already persisted locally. Also builds
/// the checkpoint subscription broker; the returned handle must be passed to
/// [`run`] so the gRPC server exposes the streaming RPC.
pub async fn initialize(
    node: Node,
    forked_at_checkpoint: CheckpointSequenceNumber,
    version: &str,
    data_dir: Option<std::path::PathBuf>,
    seed_input: SeedInput,
) -> Result<(Context, SubscriptionServiceHandle)> {
    // 1. Create DataStore — empty local cache, GraphQL wired up.
    let data_store = DataStore::new(node.clone(), forked_at_checkpoint, version, data_dir).await?;
    let chain_identifier = data_store.chain();
    crate::seed::ensure_seed_policy(&data_store, &seed_input)?;

    // 2. Download and persist the startup checkpoint (summary + contents),
    //    then read the summary back through the cache-aware getter.
    data_store.download_and_persist_startup_checkpoint()?;
    let manifest =
        crate::seed::prepare_seed_manifest(&data_store, node.network_name(), &seed_input).await?;
    if let Some(manifest) = &manifest {
        crate::seed::initialize_owned_index_from_seed(&data_store, manifest)?;
    }
    let checkpoint = data_store
        .get_highest_verified_checkpoint()?
        .ok_or_else(|| anyhow!("checkpoint {} not found", forked_at_checkpoint))?;

    // 3. Read system state — fetches object 0x5 + dynamic fields from remote via ObjectStore.
    let system_state =
        sui_types::sui_system_state::get_sui_system_state(&data_store).map_err(|e| {
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
        .with_chain_start_timestamp_ms(checkpoint.timestamp_ms)
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
        checkpoint,
        system_state,
        &config,
        data_store,
        rng,
    );

    // 8. Build the checkpoint subscription broker. The sender is owned by
    //    `Context` (producers push here on `advance_checkpoint`); the handle
    //    is wired into `RpcService` in `run` so subscribers can register.
    let registry = Registry::new();
    let (checkpoint_sender, subscription_handle) = SubscriptionService::build(&registry);

    Ok((
        Context::new(simulacrum, chain_identifier, checkpoint_sender),
        subscription_handle,
    ))
}

/// Run the forked network. Spawns a `sui-rpc-api` gRPC server bound to
/// `rpc_addr` on top of the context's `DataStore`, then blocks on Ctrl+C.
pub async fn run(
    context: Context,
    subscription_handle: SubscriptionServiceHandle,
    rpc_addr: SocketAddr,
    version: &'static str,
) -> Result<()> {
    let reader: Arc<dyn RpcStateReader> = {
        let sim = context.simulacrum().read().await;
        Arc::new(sim.store().clone())
    };
    let mut service = RpcService::new(reader);
    service.with_server_version(ServerVersion::new("sui-forking", version));
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
