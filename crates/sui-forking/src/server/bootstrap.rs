// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroUsize;

use anyhow::{Context as _, anyhow};
use rand::rngs::OsRng;
use tracing::{info, warn};

use simulacrum::{Simulacrum, store::in_mem_store::KeyStore};
use sui_data_store::{
    CheckpointStore as _, FullCheckpointData, ObjectKey, VersionQuery,
    stores::{DataStore, FileSystemStore, ReadThroughStore},
};
use sui_swarm_config::network_config::NetworkConfig;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::{
    accumulator_root::get_accumulator_root_obj_initial_shared_version,
    base_types::SuiAddress,
    message_envelope::Envelope,
    messages_checkpoint::VerifiedCheckpoint,
    object::Object,
    sui_system_state::{SuiSystemState, SuiSystemStateTrait},
    supported_protocol_versions::Chain,
};

use crate::graphql::GraphQLClient;
use crate::seeds::StartupSeeds;
use crate::store::ForkingStore;

pub(super) struct InitializedSimulacrum {
    pub(super) simulacrum: Simulacrum<OsRng, ForkingStore>,
    pub(super) faucet_owner: Option<SuiAddress>,
}

fn build_network_config(protocol_version: u64, chain: Chain, rng: &mut OsRng) -> NetworkConfig {
    ConfigBuilder::new_with_temp_dir()
        .rng(rng)
        .with_chain_start_timestamp_ms(0)
        .deterministic_committee_size(NonZeroUsize::MIN)
        .with_protocol_version(protocol_version.into())
        .with_chain_override(chain)
        .build()
}

fn load_startup_checkpoint_data(
    fs_store: &FileSystemStore,
    fs_gql_store: &ReadThroughStore<FileSystemStore, DataStore>,
    startup_checkpoint: u64,
) -> Result<FullCheckpointData, anyhow::Error> {
    match fs_store
        .get_checkpoint_by_sequence_number(startup_checkpoint)
        .context("failed to read startup checkpoint from local checkpoint store")?
    {
        Some(checkpoint) => Ok(checkpoint),
        None => fs_gql_store
            .get_checkpoint_by_sequence_number(startup_checkpoint)
            .context("failed to fetch startup checkpoint from rpc")?
            .with_context(|| format!("checkpoint {startup_checkpoint} not found")),
    }
}

fn build_verified_startup_checkpoint(
    startup_checkpoint_data: &FullCheckpointData,
) -> VerifiedCheckpoint {
    let certified_summary = startup_checkpoint_data.summary.clone();
    let env_checkpoint = Envelope::new_from_data_and_sig(
        certified_summary.data().clone(),
        certified_summary.auth_sig().clone(),
    );
    VerifiedCheckpoint::new_unchecked(env_checkpoint)
}

fn resolve_faucet_owner(keystore: &KeyStore) -> Option<SuiAddress> {
    keystore.accounts().next().map(|(owner, _)| *owner)
}

fn cache_object_if_missing(
    fs_store: &FileSystemStore,
    fs_gql_store: &ReadThroughStore<FileSystemStore, DataStore>,
    object: Object,
) -> Result<bool, anyhow::Error> {
    let object_id = object.id();
    if fs_store
        .get_object_latest(&object_id)
        .with_context(|| format!("failed to inspect local object cache for {object_id}"))?
        .is_some()
    {
        return Ok(false);
    }

    let version: u64 = object.version().into();
    let object_key = ObjectKey {
        object_id,
        version_query: VersionQuery::Version(version),
    };
    fs_gql_store
        .write_objects(vec![(object_key, object, version)])
        .with_context(|| format!("failed to seed local object {object_id}"))?;
    Ok(true)
}

/// Seeds a local faucet gas object from genesis for the configured faucet owner.
fn seed_genesis_faucet_coin(
    fs_store: &FileSystemStore,
    fs_gql_store: &ReadThroughStore<FileSystemStore, DataStore>,
    genesis_objects: &[Object],
    faucet_owner: SuiAddress,
) -> Result<(), anyhow::Error> {
    let Some(faucet_coin) = genesis_objects
        .iter()
        .filter(|object| object.is_gas_coin())
        .filter(|object| object.owner.get_address_owner_address().ok() == Some(faucet_owner))
        .max_by_key(|object| object.get_coin_value_unsafe())
        .cloned()
    else {
        warn!(
            faucet_owner = %faucet_owner,
            "No genesis gas coin found for local faucet owner; faucet may fail until a local gas coin is available"
        );
        return Ok(());
    };

    let faucet_coin_id = faucet_coin.id();
    let faucet_coin_version = faucet_coin.version().value();
    let faucet_coin_balance = faucet_coin.get_coin_value_unsafe();
    if cache_object_if_missing(fs_store, fs_gql_store, faucet_coin)? {
        info!(
            faucet_owner = %faucet_owner,
            faucet_coin_id = %faucet_coin_id,
            faucet_coin_version,
            faucet_coin_balance,
            "Seeded genesis faucet coin into local fork store"
        );
    } else {
        info!(
            faucet_owner = %faucet_owner,
            faucet_coin_id = %faucet_coin_id,
            "Genesis faucet coin already present in local fork store"
        );
    }

    Ok(())
}

fn build_initial_system_state(
    store: &ForkingStore,
    config: &NetworkConfig,
) -> Result<SuiSystemState, anyhow::Error> {
    let system_state = sui_types::sui_system_state::get_sui_system_state(store)
        .map_err(|err| anyhow!("failed to read Sui system state from startup checkpoint: {err}"))?;

    let validators = match config.genesis.sui_system_object() {
        SuiSystemState::V1(genesis_inner) => genesis_inner.validators,
        SuiSystemState::V2(genesis_inner) => genesis_inner.validators,
        #[cfg(msim)]
        _ => anyhow::bail!("unsupported genesis system state variant"),
    };

    match system_state {
        SuiSystemState::V1(mut inner) => {
            inner.validators = validators.clone();
            Ok(SuiSystemState::V1(inner))
        }
        SuiSystemState::V2(mut inner) => {
            inner.validators = validators;
            Ok(SuiSystemState::V2(inner))
        }
        #[cfg(msim)]
        _ => anyhow::bail!("unsupported system state variant"),
    }
}

fn install_validator_override_and_committee(
    store: &mut ForkingStore,
    initial_sui_system_state: &SuiSystemState,
) {
    let validator_set_override = match initial_sui_system_state {
        SuiSystemState::V1(inner) => inner.validators.clone(),
        SuiSystemState::V2(inner) => inner.validators.clone(),
    };
    store.set_system_state_validator_set_override(validator_set_override);

    let initial_committee = initial_sui_system_state
        .get_current_epoch_committee()
        .committee()
        .clone();
    store.insert_committee(initial_committee);
}

fn build_simulacrum_from_bootstrap(
    keystore: KeyStore,
    verified_checkpoint: VerifiedCheckpoint,
    initial_sui_system_state: SuiSystemState,
    config: &NetworkConfig,
    store: ForkingStore,
    rng: OsRng,
) -> Result<Simulacrum<OsRng, ForkingStore>, anyhow::Error> {
    // The mock checkpoint builder relies on the accumulator root object to be present in the store
    // with the correct initial shared version, so we need to fetch it here and pass it to the
    // simulacrum. This is needed if protocol config has enabled accumulators
    // TODO: do we need this if protocol config does not have enabled accumulators?
    let acc_initial_shared_version = get_accumulator_root_obj_initial_shared_version(&store)?
        .ok_or_else(|| anyhow!("Failed to get accumulator root object from store"))?;

    Ok(Simulacrum::new_from_custom_state(
        keystore,
        verified_checkpoint,
        initial_sui_system_state,
        config,
        store,
        rng,
        Some(acc_initial_shared_version),
    ))
}

pub(super) async fn initialize_simulacrum(
    forked_at_checkpoint: u64,
    startup_checkpoint: u64,
    client: &GraphQLClient,
    startup_seeds: &StartupSeeds,
    protocol_version: u64,
    chain: Chain,
    fs_store: FileSystemStore,
    fs_gql_store: ReadThroughStore<FileSystemStore, DataStore>,
) -> Result<InitializedSimulacrum, anyhow::Error> {
    let mut rng = OsRng;
    let config = build_network_config(protocol_version, chain, &mut rng);
    let keystore = KeyStore::from_network_config(&config);
    let faucet_owner = resolve_faucet_owner(&keystore);

    let startup_checkpoint_data =
        load_startup_checkpoint_data(&fs_store, &fs_gql_store, startup_checkpoint)?;
    let verified_checkpoint = build_verified_startup_checkpoint(&startup_checkpoint_data);

    if let Some(faucet_owner) = faucet_owner {
        if let Err(err) = seed_genesis_faucet_coin(
            &fs_store,
            &fs_gql_store,
            config.genesis.objects(),
            faucet_owner,
        ) {
            warn!(
                faucet_owner = %faucet_owner,
                "Failed to seed genesis faucet coin into local fork store: {err}"
            );
        }
    } else {
        warn!("No local account keys available; faucet will be unavailable");
    }

    let mut store = ForkingStore::new(forked_at_checkpoint, fs_store, fs_gql_store);
    store.insert_checkpoint(verified_checkpoint.clone());
    store.insert_checkpoint_contents(startup_checkpoint_data.contents.clone());
    startup_seeds
        .prefetch_startup_objects(
            &store,
            client.endpoint(),
            startup_checkpoint,
            startup_checkpoint_data.summary.timestamp_ms,
        )
        .await
        .context("Failed to prefetch startup objects")?;

    let initial_sui_system_state = build_initial_system_state(&store, &config)?;
    install_validator_override_and_committee(&mut store, &initial_sui_system_state);

    let simulacrum = build_simulacrum_from_bootstrap(
        keystore,
        verified_checkpoint,
        initial_sui_system_state,
        &config,
        store,
        rng,
    )?;

    Ok(InitializedSimulacrum {
        simulacrum,
        faucet_owner,
    })
}
