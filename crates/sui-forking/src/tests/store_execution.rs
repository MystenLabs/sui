// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end execution tests: build a `Simulacrum<OsRng, DataStore>` over a
//! tempdir-backed filesystem cache, execute transactions, and assert the
//! resulting state is persisted. Wired via `#[cfg(test)] #[path]` in
//! `store.rs`, so `super::*` resolves into the `store` module.

use std::num::NonZeroUsize;
use std::time::Duration;

use rand::rngs::OsRng;

use simulacrum::Simulacrum;
use simulacrum::store::in_mem_store::KeyStore;
use sui_swarm_config::network_config::NetworkConfig;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::KeypairTraits;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::gas_coin::GasCoin;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{GasData, Transaction, TransactionData, TransactionKind};

use super::*;

/// Build a `Simulacrum<OsRng, DataStore>` from a fresh genesis NetworkConfig.
/// The DataStore's local cache lives in the returned tempdir; its remote
/// endpoint is fake and never called. Genesis objects are populated directly
/// via `update_objects` to avoid touching the `init_with_genesis` checkpoint/
/// committee paths (which are still `todo!()`).
///
/// Returns the simulacrum, the underlying NetworkConfig (so tests can find
/// genesis objects and account keys), and the tempdir guarding the local cache.
fn test_simulacrum() -> (
    Simulacrum<OsRng, DataStore>,
    NetworkConfig,
    tempfile::TempDir,
) {
    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let mut rng = OsRng;
    let config = ConfigBuilder::new_with_temp_dir()
        .rng(&mut rng)
        .deterministic_committee_size(NonZeroUsize::MIN)
        .build();

    let mut data_store = DataStore::new_for_testing(temp.path().to_path_buf());
    let written: BTreeMap<ObjectID, Object> = config
        .genesis
        .objects()
        .iter()
        .map(|o| (o.id(), o.clone()))
        .collect();
    data_store.update_objects(written, vec![]);

    let keystore = KeyStore::from_network_config(&config);
    let sim = Simulacrum::new_from_custom_state(
        keystore,
        config.genesis.checkpoint(),
        config.genesis.sui_system_object(),
        &config,
        data_store,
        rng,
    );
    (sim, config, temp)
}

/// Find the first gas coin in the genesis object set owned by `owner`.
fn find_gas_coin(config: &NetworkConfig, owner: SuiAddress) -> Object {
    config
        .genesis
        .objects()
        .iter()
        .find(|obj| obj.owner == Owner::AddressOwner(owner) && obj.is_gas_coin())
        .expect("owner should have a gas coin in genesis")
        .clone()
}

#[test]
fn test_advance_clock_executes_and_persists() {
    let (mut sim, _config, _temp) = test_simulacrum();
    let initial_ts = sim.store().get_clock().timestamp_ms;

    let effects = sim.advance_clock(Duration::from_secs(60));
    assert!(
        effects.status().is_ok(),
        "execution failed: {:?}",
        effects.status()
    );

    assert_eq!(sim.store().get_clock().timestamp_ms, initial_ts + 60_000,);

    // The transaction was persisted to the filesystem cache.
    let tx_digest = effects.transaction_digest();
    let persisted = sim.store().get_transaction(tx_digest);
    assert!(persisted.is_some(), "transaction not persisted on disk");

    let persisted_effects = sim.store().get_transaction_effects(tx_digest);
    assert_eq!(persisted_effects.unwrap(), effects);
}

#[test]
fn test_transfer_sui_executes_and_persists() {
    let (mut sim, config, _temp) = test_simulacrum();

    // Pick a sender from the genesis keystore and a gas coin owned by the sender.
    let (sender, sender_key) = {
        let (addr, key) = sim
            .keystore()
            .accounts()
            .next()
            .expect("at least one account");
        (*addr, key.copy())
    };
    let gas_object = find_gas_coin(&config, sender);
    let gas_coin = GasCoin::try_from(&gas_object).unwrap();
    let initial_balance = gas_coin.value();
    let transfer_amount = initial_balance / 2;

    let recipient = SuiAddress::random_for_testing_only();

    // Build a transfer-SUI programmable transaction.
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.transfer_sui(recipient, Some(transfer_amount));
        builder.finish()
    };
    let tx_data = TransactionData::new_with_gas_data(
        TransactionKind::ProgrammableTransaction(pt),
        sender,
        GasData {
            payment: vec![gas_object.compute_object_reference()],
            owner: sender,
            price: sim.reference_gas_price(),
            budget: 100_000_000,
        },
    );

    // Sign with the real account key from the genesis keystore.
    let tx = Transaction::from_data_and_signer(tx_data, vec![&sender_key]);

    let (effects, exec_error) = sim.execute_transaction(tx).unwrap();
    assert!(
        effects.status().is_ok(),
        "transfer failed: status={:?} exec_error={:?}",
        effects.status(),
        exec_error,
    );

    // The transaction is persisted on disk.
    let tx_digest = effects.transaction_digest();
    assert!(
        sim.store().get_transaction(tx_digest).is_some(),
        "transaction not persisted on disk",
    );
    assert_eq!(
        sim.store().get_transaction_effects(tx_digest).unwrap(),
        effects,
    );

    // The recipient now owns a gas coin holding exactly `transfer_amount`.
    let recipient_coin = effects
        .created()
        .into_iter()
        .find_map(|((id, _, _), owner)| (owner == Owner::AddressOwner(recipient)).then_some(id))
        .expect("transfer should create a coin owned by the recipient");
    let recipient_obj = sim
        .store()
        .get_object(&recipient_coin)
        .expect("recipient coin lookup failed")
        .expect("recipient coin should be readable from the store");
    let recipient_gas = GasCoin::try_from(&recipient_obj).unwrap();
    assert_eq!(recipient_gas.value(), transfer_amount);

    // The sender's gas coin still exists, charged for gas, balance reduced by transfer_amount + net gas.
    let updated_gas_obj = sim
        .store()
        .get_object(&gas_object.id())
        .expect("sender gas coin lookup failed")
        .expect("sender gas coin should still exist");
    let updated_gas = GasCoin::try_from(&updated_gas_obj).unwrap();
    let net_gas = effects.gas_cost_summary().net_gas_usage();
    let expected = (initial_balance as i64 - transfer_amount as i64 - net_gas) as u64;
    assert_eq!(updated_gas.value(), expected);
}
