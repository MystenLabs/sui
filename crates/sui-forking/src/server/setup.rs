// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroUsize;

use anyhow::{Context as _, anyhow};
use rand::rngs::OsRng;
use tracing::{info, warn};

use simulacrum::{Simulacrum, store::in_mem_store::KeyStore};
use sui_data_store::{FullCheckpointData, ObjectKey, ObjectStoreWriter as _, VersionQuery};
use sui_swarm_config::network_config::NetworkConfig;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::{
    accumulator_root::get_accumulator_root_obj_initial_shared_version,
    message_envelope::Envelope,
    messages_checkpoint::VerifiedCheckpoint,
    object::Object,
    sui_system_state::{SuiSystemState, SuiSystemStateTrait},
    supported_protocol_versions::Chain,
    transaction::{TransactionDataAPI, TransactionKind},
};

use crate::seeds::StartupSeeds;
use crate::store::ForkingStore;

use super::data_store_setup::DataStoreSetup;

/// Runtime artifacts prepared by the server setup path.
pub(super) struct SimulacrumSetup {
    /// Local simulacrum initialized from the selected checkpoint.
    pub(super) simulacrum: Simulacrum<OsRng, ForkingStore>,
}

/// Builds a deterministic single-validator config for local execution.
fn build_network_config(protocol_version: u64, chain: Chain, rng: &mut OsRng) -> NetworkConfig {
    ConfigBuilder::new_with_temp_dir()
        .rng(rng)
        .with_chain_start_timestamp_ms(0)
        .deterministic_committee_size(NonZeroUsize::MIN)
        .with_protocol_version(protocol_version.into())
        .with_chain_override(chain)
        .build()
}

/// Converts full checkpoint data into a verified checkpoint envelope.
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

/// Stores the object in local cache only when it is not already present.
fn cache_object_if_missing(
    data_store_setup: &DataStoreSetup,
    object: Object,
) -> Result<bool, anyhow::Error> {
    let object_id = object.id();
    if data_store_setup
        .filesystem()
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
    data_store_setup
        .objects()
        .write_object(&object_key, object, version)
        .with_context(|| format!("failed to seed local object {object_id}"))?;
    Ok(true)
}

/// Seeds a local gas object from genesis for the first configured validator account.
fn seed_genesis_faucet_coin(
    data_store_setup: &DataStoreSetup,
    genesis_objects: &[Object],
    local_owner: sui_types::base_types::SuiAddress,
) -> Result<(), anyhow::Error> {
    let Some(faucet_coin) = genesis_objects
        .iter()
        .filter(|object| object.is_gas_coin())
        .filter(|object| object.owner.get_address_owner_address().ok() == Some(local_owner))
        .max_by_key(|object| object.get_coin_value_unsafe())
        .cloned()
    else {
        warn!(
            local_owner = %local_owner,
            "No genesis gas coin found for the local validator account"
        );
        return Ok(());
    };

    let faucet_coin_id = faucet_coin.id();
    let faucet_coin_version = faucet_coin.version().value();
    let faucet_coin_balance = faucet_coin.get_coin_value_unsafe();
    if cache_object_if_missing(data_store_setup, faucet_coin)? {
        info!(
            local_owner = %local_owner,
            faucet_coin_id = %faucet_coin_id,
            faucet_coin_version,
            faucet_coin_balance,
            "Seeded local genesis gas coin into fork store"
        );
    } else {
        info!(
            local_owner = %local_owner,
            faucet_coin_id = %faucet_coin_id,
            "Local genesis gas coin already present in fork store"
        );
    }

    Ok(())
}

/// Returns the post-checkpoint system state when the checkpoint itself materializes one.
fn post_checkpoint_system_state(
    startup_checkpoint_data: &FullCheckpointData,
) -> Result<Option<SuiSystemState>, anyhow::Error> {
    startup_checkpoint_data
        .epoch_info()
        .map_err(|err| {
            anyhow!(
                "failed to derive post-checkpoint system state from checkpoint {}: {err}",
                startup_checkpoint_data.summary.sequence_number
            )
        })
        .map(|epoch_info| epoch_info.and_then(|info| info.system_state))
}

/// Persists the epoch-transition outputs so subsequent object-store reads see the post-checkpoint
/// system state, not the pre-transition one.
fn cache_epoch_transition_outputs(
    data_store_setup: &DataStoreSetup,
    startup_checkpoint_data: &FullCheckpointData,
) -> Result<(), anyhow::Error> {
    if startup_checkpoint_data.summary.end_of_epoch_data.is_none() {
        return Ok(());
    }

    let Some(epoch_change_transaction) = startup_checkpoint_data.transactions.iter().find(|tx| {
        matches!(
            tx.transaction.kind(),
            TransactionKind::ChangeEpoch(_) | TransactionKind::EndOfEpochTransaction(_)
        )
    }) else {
        anyhow::bail!(
            "checkpoint {} is marked end-of-epoch but is missing the epoch-change transaction",
            startup_checkpoint_data.summary.sequence_number
        );
    };

    for object in epoch_change_transaction
        .output_objects(&startup_checkpoint_data.object_set)
        .cloned()
    {
        let object_id = object.id();
        let version = object.version().value();
        let object_key = ObjectKey {
            object_id,
            version_query: VersionQuery::AtCheckpoint(
                startup_checkpoint_data.summary.sequence_number,
            ),
        };
        data_store_setup
            .objects()
            .write_object(&object_key, object, version)
            .with_context(|| {
                format!(
                    "failed to persist epoch-transition object {object_id} for startup checkpoint {}",
                    startup_checkpoint_data.summary.sequence_number
                )
            })?;
    }

    Ok(())
}

/// Builds initial system state from the post-checkpoint state while reusing genesis validators.
fn build_initial_system_state(
    startup_checkpoint_data: &FullCheckpointData,
    store: &ForkingStore,
    config: &NetworkConfig,
) -> Result<SuiSystemState, anyhow::Error> {
    let system_state = match post_checkpoint_system_state(startup_checkpoint_data)? {
        Some(system_state) => system_state,
        None => sui_types::sui_system_state::get_sui_system_state(store).map_err(|err| {
            anyhow!(
                "failed to read Sui system state after startup checkpoint {}: {err}",
                startup_checkpoint_data.summary.sequence_number
            )
        })?,
    };

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

/// Installs validator override and committee required by local execution.
fn override_system_validators(store: &mut ForkingStore, initial_sui_system_state: &SuiSystemState) {
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

/// Constructs a simulacrum from preloaded checkpoint, state, and store.
fn build_simulacrum(
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
    let _acc_initial_shared_version = get_accumulator_root_obj_initial_shared_version(&store)?
        .ok_or_else(|| anyhow!("Failed to get accumulator root object from store"))?;

    Ok(Simulacrum::new_from_custom_state(
        keystore,
        verified_checkpoint,
        initial_sui_system_state,
        config,
        store,
        rng,
        // Some(acc_initial_shared_version),
    ))
}

/// Prepares simulacrum state and faucet owner for server startup.
pub(super) async fn setup_simulacrum(
    forked_at_checkpoint: u64,
    startup_checkpoint: u64,
    startup_seeds: &StartupSeeds,
    protocol_version: u64,
    chain: Chain,
    data_store_setup: DataStoreSetup,
) -> Result<SimulacrumSetup, anyhow::Error> {
    let mut rng = OsRng;
    let config = build_network_config(protocol_version, chain, &mut rng);
    let keystore = KeyStore::from_network_config(&config);
    let startup_checkpoint_data = data_store_setup
        .load_startup_checkpoint_data(startup_checkpoint)
        .await?;
    cache_epoch_transition_outputs(&data_store_setup, &startup_checkpoint_data)?;
    let verified_checkpoint = build_verified_startup_checkpoint(&startup_checkpoint_data);

    if let Some((local_owner, _)) = keystore.accounts().next() {
        if let Err(err) =
            seed_genesis_faucet_coin(&data_store_setup, config.genesis.objects(), *local_owner)
        {
            warn!(
                local_owner = %local_owner,
                "Failed to seed local genesis gas coin into fork store: {err}"
            );
        }
    } else {
        warn!("No local validator account keys available");
    }

    let (filesystem, objects) = data_store_setup.into_parts();
    let mut store = ForkingStore::new(forked_at_checkpoint, filesystem, objects);
    store.insert_startup_checkpoint_data(&startup_checkpoint_data)?;
    startup_seeds
        .prefetch_startup_objects(&store, startup_checkpoint)
        .await
        .context("Failed to prefetch startup objects")?;

    let initial_sui_system_state =
        build_initial_system_state(&startup_checkpoint_data, &store, &config)?;
    override_system_validators(&mut store, &initial_sui_system_state);

    let simulacrum = build_simulacrum(
        keystore,
        verified_checkpoint,
        initial_sui_system_state,
        &config,
        store,
        rng,
    )?;

    Ok(SimulacrumSetup { simulacrum })
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use rand::rngs::OsRng;
    use simulacrum::{AdvanceEpochConfig, Simulacrum, store::in_mem_store::KeyStore};
    use sui_types::effects::TransactionEffectsAPI;
    use sui_types::execution_status::ExecutionStatus;
    use sui_types::gas_coin::GasCoin;
    use sui_types::storage::ReadStore;
    use sui_types::{
        base_types::SuiAddress, sui_system_state::SuiSystemStateTrait, transaction::TransactionData,
    };
    use tempfile::TempDir;

    use crate::network::ForkNetwork;
    use crate::store::ForkingStore;

    use super::*;

    fn sample_end_of_epoch_checkpoint() -> FullCheckpointData {
        let mut simulacrum = Simulacrum::new();
        simulacrum.advance_epoch(AdvanceEpochConfig::default());

        let checkpoint = simulacrum
            .store_dyn()
            .get_highest_checkpint()
            .expect("highest checkpoint");
        let contents = simulacrum
            .store_dyn()
            .get_checkpoint_contents(&checkpoint.content_digest)
            .expect("checkpoint contents");

        simulacrum
            .get_checkpoint_data(checkpoint, contents)
            .expect("full checkpoint data")
    }

    fn sample_startup_checkpoint() -> (FullCheckpointData, SuiSystemState) {
        let simulacrum = Simulacrum::new();
        let checkpoint = simulacrum
            .store_dyn()
            .get_highest_checkpint()
            .expect("highest checkpoint");
        let contents = simulacrum
            .store_dyn()
            .get_checkpoint_contents(&checkpoint.content_digest)
            .expect("checkpoint contents");
        let checkpoint_data = simulacrum
            .get_checkpoint_data(checkpoint, contents)
            .expect("full checkpoint data");

        (checkpoint_data, simulacrum.store_dyn().get_system_state())
    }

    fn seed_checkpoint_object_set(
        data_store_setup: &DataStoreSetup,
        checkpoint: &FullCheckpointData,
    ) -> Result<()> {
        for object in checkpoint.object_set.iter().cloned() {
            let object_id = object.id();
            let version = object.version().value();
            data_store_setup.objects().write_object(
                &ObjectKey {
                    object_id,
                    version_query: VersionQuery::AtCheckpoint(checkpoint.summary.sequence_number),
                },
                object,
                version,
            )?;
        }
        Ok(())
    }

    fn build_test_setup(
        tempdir: &TempDir,
        at_checkpoint: u64,
    ) -> Result<DataStoreSetup, anyhow::Error> {
        super::super::data_store_setup::build_data_store_setup(
            &ForkNetwork::Testnet,
            "http://127.0.0.1:9000",
            at_checkpoint,
            tempdir.path(),
            "test-version",
        )
    }

    #[test]
    fn build_initial_system_state_uses_post_checkpoint_epoch_state() -> Result<()> {
        let startup_checkpoint_data = sample_end_of_epoch_checkpoint();
        let expected_system_state = post_checkpoint_system_state(&startup_checkpoint_data)?
            .expect("post-checkpoint system state");
        let tempdir = tempfile::tempdir()?;
        let data_store_setup =
            build_test_setup(&tempdir, startup_checkpoint_data.summary.sequence_number)?;

        cache_epoch_transition_outputs(&data_store_setup, &startup_checkpoint_data)?;
        let (filesystem, objects) = data_store_setup.into_parts();
        let store = ForkingStore::new(
            startup_checkpoint_data.summary.sequence_number,
            filesystem,
            objects,
        );
        let mut rng = OsRng;
        let config = build_network_config(
            expected_system_state.protocol_version(),
            Chain::Testnet,
            &mut rng,
        );

        let initial_system_state =
            build_initial_system_state(&startup_checkpoint_data, &store, &config)?;

        assert_eq!(startup_checkpoint_data.summary.epoch, 0);
        assert_eq!(expected_system_state.epoch(), 1);
        assert_eq!(initial_system_state.epoch(), expected_system_state.epoch());
        assert_eq!(
            initial_system_state.protocol_version(),
            expected_system_state.protocol_version()
        );

        Ok(())
    }

    #[test]
    fn cache_epoch_transition_outputs_seeds_fork_store_system_state() -> Result<()> {
        let startup_checkpoint_data = sample_end_of_epoch_checkpoint();
        let expected_system_state = post_checkpoint_system_state(&startup_checkpoint_data)?
            .expect("post-checkpoint system state");
        let tempdir = tempfile::tempdir()?;
        let data_store_setup =
            build_test_setup(&tempdir, startup_checkpoint_data.summary.sequence_number)?;

        cache_epoch_transition_outputs(&data_store_setup, &startup_checkpoint_data)?;
        let (filesystem, objects) = data_store_setup.into_parts();
        let store = ForkingStore::new(
            startup_checkpoint_data.summary.sequence_number,
            filesystem,
            objects,
        );

        assert_eq!(
            store.get_system_state().epoch(),
            expected_system_state.epoch()
        );
        assert_eq!(
            store.get_system_state().protocol_version(),
            expected_system_state.protocol_version()
        );

        Ok(())
    }

    #[test]
    fn impersonated_execution_smoke_test_updates_in_memory_state() -> Result<()> {
        let (startup_checkpoint_data, startup_system_state) = sample_startup_checkpoint();
        let startup_sequence = startup_checkpoint_data.summary.sequence_number;
        let tempdir = tempfile::tempdir()?;
        let data_store_setup = build_test_setup(&tempdir, startup_sequence)?;
        seed_checkpoint_object_set(&data_store_setup, &startup_checkpoint_data)?;

        let verified_checkpoint = build_verified_startup_checkpoint(&startup_checkpoint_data);
        let mut rng = OsRng;
        let config = build_network_config(
            startup_system_state.protocol_version(),
            Chain::Testnet,
            &mut rng,
        );
        let keystore = KeyStore::from_network_config(&config);

        let (filesystem, objects) = data_store_setup.into_parts();
        let mut store = ForkingStore::new(startup_sequence, filesystem, objects);
        store.insert_startup_checkpoint_data(&startup_checkpoint_data)?;

        let initial_system_state =
            build_initial_system_state(&startup_checkpoint_data, &store, &config)?;
        override_system_validators(&mut store, &initial_system_state);

        let mut simulacrum = build_simulacrum(
            keystore,
            verified_checkpoint,
            initial_system_state,
            &config,
            store,
            rng,
        )?;

        let gas_object = startup_checkpoint_data
            .object_set
            .iter()
            .find(|object| {
                object.is_gas_coin() && object.owner().get_address_owner_address().is_ok()
            })
            .cloned()
            .expect("startup checkpoint should contain an address-owned gas coin");
        let sender = gas_object
            .owner()
            .get_address_owner_address()
            .expect("gas coin should be address owned");
        let transfer_amount = GasCoin::try_from(&gas_object).expect("gas coin").value() / 2;
        let transaction_data = TransactionData::new_transfer_sui(
            SuiAddress::random_for_testing_only(),
            sender,
            Some(transfer_amount),
            gas_object.compute_object_reference(),
            1_000_000_000,
            simulacrum.reference_gas_price(),
        );

        let (effects, execution_error) =
            simulacrum.execute_transaction_impersonating(transaction_data)?;
        assert!(execution_error.is_none(), "execution should succeed");
        assert_eq!(effects.status(), &ExecutionStatus::Success);

        let checkpoint = simulacrum.create_checkpoint();
        assert_eq!(checkpoint.sequence_number, startup_sequence + 1);

        let store = simulacrum.store();
        assert!(
            store
                .get_transaction(effects.transaction_digest())
                .is_some()
        );
        assert!(
            store
                .get_transaction_effects(effects.transaction_digest())
                .is_some()
        );
        assert_eq!(
            store.get_highest_checkpint().unwrap().sequence_number,
            startup_sequence + 1
        );
        assert!(store.transaction_count() >= startup_checkpoint_data.transactions.len() + 1);
        assert!(store.effect_count() >= startup_checkpoint_data.transactions.len() + 1);
        assert_eq!(store.checkpoint_count(), 2);

        let updated_gas_object = store
            .get_object(&gas_object.id())
            .expect("updated gas object should be readable");
        assert!(updated_gas_object.version() > gas_object.version());

        Ok(())
    }
}
