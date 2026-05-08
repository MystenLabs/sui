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
use simulacrum::store::SimulatorStore;
use simulacrum::store::in_mem_store::KeyStore;
use sui_swarm_config::network_config::NetworkConfig;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::KeypairTraits;
use sui_types::digests::ObjectDigest;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::execution_status::ExecutionStatus;
use sui_types::gas::GasCostSummary;
use sui_types::gas_coin::GasCoin;
use sui_types::object::MoveObject;
use sui_types::object::ObjectInner;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::storage::RpcIndexes;
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

fn test_data_store() -> (tempfile::TempDir, DataStore) {
    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let data_store = DataStore::new_for_testing(temp.path().to_path_buf());
    (temp, data_store)
}

fn make_gas_object(id: ObjectID, version: u64, owner: Owner) -> Object {
    let move_obj = MoveObject::new_gas_coin(SequenceNumber::from_u64(version), id, 1_000_000);
    ObjectInner {
        owner,
        data: sui_types::object::Data::Move(move_obj),
        previous_transaction: TransactionDigest::genesis_marker(),
        storage_rebate: 0,
    }
    .into()
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
    let persisted = sim
        .store()
        .get_transaction(tx_digest)
        .expect("transaction read should not error");
    assert!(persisted.is_some(), "transaction not persisted on disk");

    let persisted_effects = sim
        .store()
        .get_transaction_effects(tx_digest)
        .expect("effects read should not error");
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
        sim.store()
            .get_transaction(tx_digest)
            .expect("transaction read should not error")
            .is_some(),
        "transaction not persisted on disk",
    );
    assert_eq!(
        sim.store()
            .get_transaction_effects(tx_digest)
            .expect("effects read should not error")
            .unwrap(),
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

#[test]
fn test_owned_objects_tracks_address_owner_transfers() {
    let (_temp, mut store) = test_data_store();
    let owner = SuiAddress::random_for_testing_only();
    let recipient = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = make_gas_object(object_id, 1, Owner::AddressOwner(owner));

    store.update_objects(BTreeMap::from([(object_id, object)]), vec![]);
    let owner_objects: Vec<_> = SimulatorStore::owned_objects(&store, owner).collect();
    assert_eq!(owner_objects.len(), 1);
    assert_eq!(owner_objects[0].id(), object_id);

    let transferred = make_gas_object(object_id, 2, Owner::AddressOwner(recipient));
    store.update_objects(BTreeMap::from([(object_id, transferred)]), vec![]);

    assert_eq!(
        SimulatorStore::owned_objects(&store, owner).count(),
        0,
        "object should leave the previous owner's index",
    );
    let recipient_objects: Vec<_> = SimulatorStore::owned_objects(&store, recipient).collect();
    assert_eq!(recipient_objects.len(), 1);
    assert_eq!(recipient_objects[0].id(), object_id);
    assert_eq!(recipient_objects[0].version(), SequenceNumber::from_u64(2));
}

#[test]
fn test_owned_objects_removes_non_address_owned_transitions() {
    let (_temp, mut store) = test_data_store();
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = make_gas_object(object_id, 1, Owner::AddressOwner(owner));

    store.update_objects(BTreeMap::from([(object_id, object)]), vec![]);
    assert_eq!(SimulatorStore::owned_objects(&store, owner).count(), 1);

    let immutable = make_gas_object(object_id, 2, Owner::Immutable);
    store.update_objects(BTreeMap::from([(object_id, immutable)]), vec![]);
    assert_eq!(SimulatorStore::owned_objects(&store, owner).count(), 0);
}

#[test]
fn test_seeded_owned_object_metadata_lists_without_bcs_until_deleted() {
    let (_temp, mut store) = test_data_store();
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();

    store
        .local()
        .write_owned_object_entries(&[OwnedObjectEntry {
            owner,
            object_id,
            version: SequenceNumber::from_u64(7),
            object_type: GasCoin::type_(),
            balance: Some(123),
        }])
        .unwrap();

    let infos: Vec<_> = RpcIndexes::owned_objects_iter(&store, owner, Some(GasCoin::type_()), None)
        .expect("seeded owned-object iterator should build")
        .map(|result| result.expect("seeded entry should decode"))
        .collect();
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].object_id, object_id);
    assert_eq!(infos[0].version, SequenceNumber::from_u64(7));
    assert_eq!(infos[0].balance, Some(123));
    assert!(
        store
            .local()
            .get_latest_object(&object_id)
            .expect("local lookup should not fail")
            .is_none(),
        "seed metadata should not require local BCS",
    );

    store.update_objects(
        BTreeMap::new(),
        vec![(
            object_id,
            SequenceNumber::from_u64(8),
            ObjectDigest::OBJECT_DIGEST_DELETED,
        )],
    );

    assert_eq!(
        RpcIndexes::owned_objects_iter(&store, owner, Some(GasCoin::type_()), None)
            .expect("owned-object iterator should build")
            .count(),
        0,
    );
}

#[test]
fn test_local_deletion_removes_owned_object_and_blocks_remote_resurrection() {
    let (_temp, mut store) = test_data_store();
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = make_gas_object(object_id, 1, Owner::AddressOwner(owner));

    store.update_objects(BTreeMap::from([(object_id, object.clone())]), vec![]);
    store.update_objects(
        BTreeMap::new(),
        vec![(
            object_id,
            SequenceNumber::from_u64(2),
            ObjectDigest::OBJECT_DIGEST_DELETED,
        )],
    );

    assert_eq!(SimulatorStore::owned_objects(&store, owner).count(), 0);
    assert!(
        DataStore::get_object(&store, &object_id)
            .expect("current object read should not error")
            .is_none(),
        "current object lookup must not fall back to the remote after local deletion",
    );
    assert_eq!(
        DataStore::get_object_at_version(&store, &object_id, 1)
            .expect("exact version read should not error")
            .unwrap(),
        object,
    );
    assert!(
        sui_types::storage::ObjectStore::get_object_by_key(
            &store,
            &object_id,
            SequenceNumber::from_u64(1),
        )
        .is_none(),
        "execution-facing exact-version lookup must reject locally deleted objects",
    );
}

#[test]
fn test_local_wrap_removes_owned_object_and_blocks_direct_current_reads() {
    let (_temp, mut store) = test_data_store();
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = make_gas_object(object_id, 1, Owner::AddressOwner(owner));

    store.update_objects(BTreeMap::from([(object_id, object.clone())]), vec![]);
    store.apply_object_updates(
        BTreeMap::new(),
        vec![RemovedObject {
            object_ref: (
                object_id,
                SequenceNumber::from_u64(2),
                ObjectDigest::OBJECT_DIGEST_WRAPPED,
            ),
            kind: RemovedObjectKind::Wrapped,
        }],
    );

    assert_eq!(SimulatorStore::owned_objects(&store, owner).count(), 0);
    assert!(
        DataStore::get_object(&store, &object_id)
            .expect("current object read should not error")
            .is_none(),
        "current object lookup must not fall back to the remote after local wrapping",
    );
    assert_eq!(
        DataStore::get_object_at_version(&store, &object_id, 1)
            .expect("exact version read should not error")
            .unwrap(),
        object,
    );
    assert!(
        sui_types::storage::ObjectStore::get_object_by_key(
            &store,
            &object_id,
            SequenceNumber::from_u64(1),
        )
        .is_none(),
        "execution-facing exact-version lookup must reject locally wrapped objects",
    );
}

#[test]
fn test_unwrapped_write_clears_wrapped_marker_and_reindexes_owner() {
    let (_temp, mut store) = test_data_store();
    let owner = SuiAddress::random_for_testing_only();
    let recipient = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = make_gas_object(object_id, 1, Owner::AddressOwner(owner));

    store.update_objects(BTreeMap::from([(object_id, object)]), vec![]);
    store.apply_object_updates(
        BTreeMap::new(),
        vec![RemovedObject {
            object_ref: (
                object_id,
                SequenceNumber::from_u64(2),
                ObjectDigest::OBJECT_DIGEST_WRAPPED,
            ),
            kind: RemovedObjectKind::Wrapped,
        }],
    );

    let unwrapped = make_gas_object(object_id, 3, Owner::AddressOwner(recipient));
    store.apply_object_updates(BTreeMap::from([(object_id, unwrapped.clone())]), vec![]);

    assert!(!store.local().is_object_wrapped(&object_id).unwrap());
    assert_eq!(
        DataStore::get_object(&store, &object_id)
            .expect("current object read should not error")
            .unwrap(),
        unwrapped,
    );
    assert_eq!(SimulatorStore::owned_objects(&store, owner).count(), 0);
    let recipient_objects: Vec<_> = SimulatorStore::owned_objects(&store, recipient).collect();
    assert_eq!(recipient_objects.len(), 1);
    assert_eq!(recipient_objects[0].id(), object_id);
}

#[test]
fn test_terminal_deleted_marker_prevents_reindexing_written_object() {
    let (_temp, mut store) = test_data_store();
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = make_gas_object(object_id, 1, Owner::AddressOwner(owner));
    let written_again = make_gas_object(object_id, 3, Owner::AddressOwner(owner));

    store.update_objects(BTreeMap::from([(object_id, object)]), vec![]);
    store.apply_object_updates(
        BTreeMap::from([(object_id, written_again)]),
        vec![RemovedObject {
            object_ref: (
                object_id,
                SequenceNumber::from_u64(2),
                ObjectDigest::OBJECT_DIGEST_DELETED,
            ),
            kind: RemovedObjectKind::Deleted,
        }],
    );

    assert_eq!(SimulatorStore::owned_objects(&store, owner).count(), 0);
    assert!(
        DataStore::get_object(&store, &object_id)
            .expect("current object read should not error")
            .is_none(),
    );
}

#[test]
fn test_removed_objects_from_effects_marks_unwrapped_then_deleted_as_deleted() {
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object_ref = (
        object_id,
        SequenceNumber::from_u64(2),
        ObjectDigest::OBJECT_DIGEST_DELETED,
    );
    let gas_ref = (
        ObjectID::random(),
        SequenceNumber::from_u64(1),
        ObjectDigest::random(),
    );
    let effects = TransactionEffects::new_from_execution_v1(
        ExecutionStatus::Success,
        0,
        GasCostSummary::default(),
        vec![],
        vec![],
        TransactionDigest::random(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![object_ref],
        vec![],
        (gas_ref, Owner::AddressOwner(owner)),
        None,
        vec![],
    );

    assert_eq!(
        removed_objects_from_effects(&effects),
        vec![RemovedObject {
            object_ref,
            kind: RemovedObjectKind::Deleted,
        }],
    );
}

#[test]
fn test_rpc_owned_objects_iter_filters_and_pages_by_object_id() {
    let (_temp, mut store) = test_data_store();
    let owner = SuiAddress::random_for_testing_only();
    let other_owner = SuiAddress::random_for_testing_only();
    let first_id = ObjectID::random();
    let second_id = ObjectID::random();
    let other_id = ObjectID::random();
    let first = make_gas_object(first_id, 1, Owner::AddressOwner(owner));
    let second = make_gas_object(second_id, 1, Owner::AddressOwner(owner));
    let other = make_gas_object(other_id, 1, Owner::AddressOwner(other_owner));

    store.update_objects(
        BTreeMap::from([(first_id, first), (second_id, second), (other_id, other)]),
        vec![],
    );

    let infos: Vec<_> = RpcIndexes::owned_objects_iter(&store, owner, Some(GasCoin::type_()), None)
        .expect("owned-object iterator should build")
        .map(|result| result.expect("owned-object entry should decode"))
        .collect();
    assert_eq!(infos.len(), 2);
    assert!(infos[0].object_id < infos[1].object_id);
    assert!(infos.iter().all(|info| info.owner == owner));
    assert!(infos.iter().all(|info| info.balance == Some(1_000_000)));

    let wrong_type = "0x2::clock::Clock".parse::<StructTag>().unwrap();
    assert_eq!(
        RpcIndexes::owned_objects_iter(&store, owner, Some(wrong_type), None)
            .expect("owned-object iterator should build")
            .count(),
        0,
    );

    let page_from_cursor: Vec<_> = RpcIndexes::owned_objects_iter(
        &store,
        owner,
        Some(GasCoin::type_()),
        Some(infos[1].clone()),
    )
    .expect("owned-object iterator should build")
    .map(|result| result.expect("owned-object entry should decode"))
    .collect();
    assert_eq!(page_from_cursor.len(), 1);
    assert_eq!(page_from_cursor[0].object_id, infos[1].object_id);
}

#[test]
fn test_cloned_store_shares_owned_object_snapshot_guard() {
    let (_temp, mut store) = test_data_store();
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = make_gas_object(object_id, 1, Owner::AddressOwner(owner));
    store.update_objects(BTreeMap::from([(object_id, object)]), vec![]);

    let reader = store.clone();
    let local_snapshot_guard = store
        .write_local_snapshot()
        .expect("snapshot lock should not be poisoned");
    assert!(
        reader.inner.local_snapshot_lock.try_read().is_err(),
        "cloned stores should share the same snapshot guard",
    );
    drop(local_snapshot_guard);

    let infos: Vec<_> =
        RpcIndexes::owned_objects_iter(&reader, owner, Some(GasCoin::type_()), None)
            .expect("owned-object iterator should build")
            .map(|result| result.expect("owned-object entry should decode"))
            .collect();
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].object_id, object_id);
}
