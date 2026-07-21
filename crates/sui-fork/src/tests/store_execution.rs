// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end execution tests: build a `Simulacrum<OsRng, ForkStore>` over a
//! tempdir-backed fork store, execute transactions, and assert the resulting
//! state is saved. Wired via `#[cfg(test)] #[path]` in
//! `store.rs`, so `super::*` resolves into the `store` module.

use std::num::NonZeroUsize;
use std::path::Path;
use std::time::Duration;

use fastcrypto::encoding::Base64 as FastCryptoBase64;
use rand::rngs::OsRng;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::body_partial_json;
use wiremock::matchers::method;
use wiremock::matchers::path;

use simulacrum::Simulacrum;
use simulacrum::store::SimulatorStore;
use simulacrum::store::in_mem_store::KeyStore;
use sui_swarm_config::network_config::NetworkConfig;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::base_types::SuiAddress;
use sui_types::coin::CoinMetadata;
use sui_types::coin::RegulatedCoinMetadata;
use sui_types::coin::TreasuryCap;
use sui_types::crypto::KeypairTraits;
use sui_types::digests::CheckpointDigest;
use sui_types::digests::ObjectDigest;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::execution_status::ExecutionStatus;
use sui_types::gas::GasCostSummary;
use sui_types::gas_coin::GAS;
use sui_types::gas_coin::GasCoin;
use sui_types::id::UID;
use sui_types::object::MoveObject;
use sui_types::object::ObjectInner;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::storage::RpcIndexes;
use sui_types::transaction::GasData;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;
use sui_types::transaction::TransactionKind;

use super::*;
use crate::rpc::reader::ForkRpcReader;
use crate::runtime::ForkRuntime;

/// Build a `Simulacrum<OsRng, ForkStore>` from a fresh genesis NetworkConfig.
/// The ForkStore's local metadata and RPC store live in the returned tempdir;
/// its remote endpoint is fake and never called. Genesis objects are populated
/// directly via `update_objects` to avoid touching the `init_with_genesis`
/// checkpoint/committee paths (which are still `todo!()`).
///
/// Returns the simulacrum, the underlying NetworkConfig (so tests can find
/// genesis objects and account keys), and the tempdir guarding the local store.
fn test_simulacrum() -> (
    Simulacrum<OsRng, ForkStore>,
    NetworkConfig,
    tempfile::TempDir,
) {
    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let mut rng = OsRng;
    let config = ConfigBuilder::new_with_temp_dir()
        .rng(&mut rng)
        .deterministic_committee_size(NonZeroUsize::MIN)
        .build();

    let runtime = open_test_runtime(temp.path(), 0);
    let mut store = ForkStore::new_for_testing(temp.path().to_path_buf(), runtime.local_store());
    let written: BTreeMap<ObjectID, Object> = config
        .genesis
        .objects()
        .iter()
        .map(|o| (o.id(), o.clone()))
        .collect();
    store.update_objects(written, vec![]);

    let keystore = KeyStore::from_network_config(&config);
    let sim = Simulacrum::new_from_custom_state(
        keystore,
        config.genesis.checkpoint(),
        config.genesis.sui_system_object(),
        &config,
        store,
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

fn test_data_store() -> (tempfile::TempDir, ForkStore) {
    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let runtime = open_test_runtime(temp.path(), 0);
    let store = ForkStore::new_for_testing(temp.path().to_path_buf(), runtime.local_store());
    (temp, store)
}

fn fork_rpc_reader(store: &ForkStore) -> ForkRpcReader {
    ForkRpcReader::new(store.local_store().reader().clone(), store.clone())
}

fn test_data_store_with_remote(
    root: &Path,
    gql_url: String,
    forked_at_checkpoint: CheckpointSequenceNumber,
) -> (ForkStore, ForkRuntime) {
    let runtime = open_test_runtime(root, forked_at_checkpoint);
    let store = ForkStore::new_for_testing_with_remote(
        root.to_path_buf(),
        gql_url,
        forked_at_checkpoint,
        runtime.local_store(),
    );
    (store, runtime)
}

fn open_test_runtime(root: &Path, forked_at_checkpoint: CheckpointSequenceNumber) -> ForkRuntime {
    ForkRuntime::open(
        root,
        "custom".to_owned(),
        forked_at_checkpoint,
        CheckpointDigest::new([9; 32]).into(),
    )
    .expect("fork runtime should open")
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

fn object_at_checkpoint_response(object: &Object) -> serde_json::Value {
    serde_json::json!({
        "data": {
            "checkpoint": {
                "query": {
                    "object": {
                        "address": object.id().to_string(),
                        "version": object.version().value(),
                        "objectBcs": FastCryptoBase64::from_bytes(
                            &bcs::to_bytes(object).expect("object should serialize"),
                        )
                        .encoded(),
                    }
                }
            }
        }
    })
}

fn objects_response(objects: &[Option<&Object>]) -> serde_json::Value {
    serde_json::json!({
        "data": {
            "multiGetObjects": objects
                .iter()
                .map(|object| {
                    object.map(|object| {
                        serde_json::json!({
                            "address": object.id().to_string(),
                            "version": object.version().value(),
                            "objectBcs": FastCryptoBase64::from_bytes(
                                &bcs::to_bytes(object).expect("object should serialize"),
                            )
                            .encoded(),
                        })
                    })
                })
                .collect::<Vec<_>>(),
        }
    })
}

fn inventory_objects_response(objects: &[&Object]) -> serde_json::Value {
    serde_json::json!({
        "data": {
            "checkpoint": {
                "query": {
                    "objects": {
                        "nodes": objects
                            .iter()
                            .map(|object| {
                                serde_json::json!({
                                    "address": object.id().to_string(),
                                    "version": object.version().value(),
                                    "digest": object.digest().to_string(),
                                })
                            })
                            .collect::<Vec<_>>(),
                        "pageInfo": {
                            "hasNextPage": false,
                            "endCursor": null,
                        },
                    }
                }
            }
        }
    })
}

fn address_objects_response(objects: &[&Object]) -> serde_json::Value {
    serde_json::json!({
        "data": {
            "checkpoint": {
                "query": {
                    "address": {
                        "objects": {
                            "nodes": objects
                                .iter()
                                .map(|object| {
                                    serde_json::json!({
                                        "address": object.id().to_string(),
                                        "version": object.version().value(),
                                        "digest": object.digest().to_string(),
                                    })
                                })
                                .collect::<Vec<_>>(),
                            "pageInfo": {
                                "hasNextPage": false,
                                "endCursor": null,
                            },
                        }
                    }
                }
            }
        }
    })
}

async fn mock_address_owner_inventory(
    server: &MockServer,
    checkpoint: u64,
    owner: SuiAddress,
    objects: &[&Object],
) {
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_partial_json(serde_json::json!({
            "variables": {
                "sequenceNumber": checkpoint,
                "address": owner.to_string(),
                "after": null,
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(address_objects_response(objects)))
        .mount(server)
        .await;

    for object in objects {
        mock_seed_object(server, checkpoint, object).await;
    }
}

async fn mock_object_owner_inventory(
    server: &MockServer,
    checkpoint: u64,
    owner: ObjectID,
    objects: &[&Object],
) {
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_partial_json(serde_json::json!({
            "variables": {
                "sequenceNumber": checkpoint,
                "filter": {
                    "owner": owner.to_string(),
                },
                "after": null,
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(inventory_objects_response(objects)))
        .mount(server)
        .await;

    for object in objects {
        mock_seed_object(server, checkpoint, object).await;
    }
}

async fn mock_type_inventory(
    server: &MockServer,
    checkpoint: u64,
    object_type: &StructTag,
    objects: &[&Object],
) {
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_partial_json(serde_json::json!({
            "variables": {
                "sequenceNumber": checkpoint,
                "filter": {
                    "type": object_type.to_string(),
                },
                "after": null,
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(inventory_objects_response(objects)))
        .mount(server)
        .await;

    for object in objects {
        mock_seed_object(server, checkpoint, object).await;
    }
}

async fn mock_seed_object(server: &MockServer, checkpoint: u64, object: &Object) {
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_partial_json(serde_json::json!({
            "variables": {
                "sequenceNumber": checkpoint,
                "address": object.id().to_string(),
                "version": object.version().value(),
            }
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(object_at_checkpoint_response(object)),
        )
        .mount(server)
        .await;
}

#[tokio::test]
async fn test_current_object_read_saves_into_rpc_store_when_attached() {
    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let checkpoint = 42;
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = make_gas_object(object_id, 7, Owner::AddressOwner(owner));

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_partial_json(serde_json::json!({
            "variables": {
                "keys": [
                    {
                        "address": object_id.to_string(),
                        "atCheckpoint": checkpoint,
                    },
                ],
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(objects_response(&[Some(&object)])))
        .mount(&server)
        .await;

    let (store, runtime) = test_data_store_with_remote(temp.path(), server.uri(), checkpoint);

    let read = ForkStore::get_object(&store, &object_id)
        .expect("current object read should not error")
        .expect("remote object should be found");
    assert_eq!(read, object);

    let reader = runtime.reader();
    assert_eq!(
        sui_types::storage::ObjectStore::get_object(&reader, &object_id),
        Some(object),
    );
    assert!(
        !temp
            .path()
            .join("objects")
            .join(object_id.to_string())
            .exists(),
        "rpc-backed saves should not write object files",
    );
}

#[test]
fn test_rpc_store_tombstone_blocks_remote_current_fallback() {
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

    assert!(
        ForkStore::get_object(&store, &object_id)
            .expect("deleted object read should not call remote")
            .is_none(),
    );
    assert_eq!(
        ForkStore::get_object_at_version(&store, &object_id, 1)
            .expect("historical object read should not error"),
        Some(object),
    );
}

#[tokio::test]
async fn test_rpc_owned_objects_initializes_address_inventory_from_graphql() {
    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let checkpoint = 42;
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = make_gas_object(object_id, 7, Owner::AddressOwner(owner));

    let server = MockServer::start().await;
    mock_address_owner_inventory(&server, checkpoint, owner, &[&object]).await;

    let (store, _runtime) = test_data_store_with_remote(temp.path(), server.uri(), checkpoint);

    let reader = fork_rpc_reader(&store);
    let infos: Vec<_> =
        RpcIndexes::owned_objects_iter(&reader, owner, Some(GasCoin::type_()), None)
            .expect("owned-object iterator should initialize address inventory")
            .map(|result| result.expect("owned-object entry should decode"))
            .collect();
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].owner, owner);
    assert_eq!(infos[0].object_id, object_id);
    assert_eq!(infos[0].version, SequenceNumber::from_u64(7));
    assert_eq!(infos[0].balance, Some(1_000_000));

    let balance = RpcIndexes::get_balance(&reader, &owner, &GAS::type_())
        .expect("balance lookup should use initialized address inventory")
        .expect("gas balance should exist");
    assert_eq!(balance.coin_balance, 1_000_000);
    assert_eq!(balance.address_balance, 0);

    let balances: Vec<_> = RpcIndexes::balance_iter(&reader, &owner, None)
        .expect("balance iterator should use initialized address inventory")
        .map(|entry| entry.expect("balance row should decode"))
        .collect();
    assert_eq!(balances.len(), 1);
    assert_eq!(balances[0].0, GAS::type_());
    assert_eq!(balances[0].1.coin_balance, 1_000_000);
}

#[tokio::test]
async fn test_seed_save_survives_restart_without_remote() {
    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let checkpoint = 42;
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = make_gas_object(object_id, 7, Owner::AddressOwner(owner));

    let server = MockServer::start().await;
    mock_seed_object(&server, checkpoint, &object).await;
    mock_address_owner_inventory(&server, checkpoint, owner, &[&object]).await;
    let object_ref = object.compute_object_reference();

    {
        let (store, runtime) = test_data_store_with_remote(temp.path(), server.uri(), checkpoint);
        store
            .save_address_owned_seed_objects(&[object_ref])
            .expect("seed object should be saved");
        let reader = fork_rpc_reader(&store);
        let infos: Vec<_> =
            RpcIndexes::owned_objects_iter(&reader, owner, Some(GasCoin::type_()), None)
                .expect("owned-object iterator should use saved seed")
                .map(|result| result.expect("owned-object entry should decode"))
                .collect();
        assert_eq!(infos.len(), 1);
        let balance = RpcIndexes::get_balance(&reader, &owner, &GAS::type_())
            .expect("balance lookup should use saved seed")
            .expect("gas balance should exist");
        assert_eq!(balance.coin_balance, 1_000_000);
        drop(runtime);
    }

    let (store, _runtime) =
        test_data_store_with_remote(temp.path(), "http://localhost:1".to_owned(), checkpoint);
    store
        .save_address_owned_seed_objects(&[object_ref])
        .expect("existing seed object should be saved without remote");
    let reader = fork_rpc_reader(&store);
    let infos: Vec<_> =
        RpcIndexes::owned_objects_iter(&reader, owner, Some(GasCoin::type_()), None)
            .expect("owned-object iterator should use reopened seed index")
            .map(|result| result.expect("owned-object entry should decode"))
            .collect();

    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].object_id, object_id);
}

#[tokio::test]
async fn test_rpc_dynamic_field_iter_initializes_object_owner_inventory_from_graphql() {
    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let checkpoint = 42;
    let parent = ObjectID::random();
    let child_id = ObjectID::random();
    let child = make_gas_object(child_id, 7, Owner::ObjectOwner(parent.into()));

    let server = MockServer::start().await;
    mock_object_owner_inventory(&server, checkpoint, parent, &[&child]).await;

    let (store, _runtime) = test_data_store_with_remote(temp.path(), server.uri(), checkpoint);

    let reader = fork_rpc_reader(&store);
    let fields: Vec<_> = RpcIndexes::dynamic_field_iter(&reader, parent, None)
        .expect("dynamic-field iterator should initialize object-owner inventory")
        .map(|result| result.expect("dynamic-field row should decode"))
        .collect();

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].parent, parent);
    assert_eq!(fields[0].field_id, child_id);
}

#[tokio::test]
async fn test_rpc_get_coin_info_initializes_type_inventory_from_graphql() {
    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let checkpoint = 42;
    let coin_type = GAS::type_();
    let metadata_id = ObjectID::random();
    let metadata_object = Object::coin_metadata_for_testing(
        coin_type.clone(),
        CoinMetadata {
            id: UID::new(metadata_id),
            decimals: 9,
            name: "Sui".to_owned(),
            symbol: "SUI".to_owned(),
            description: "Sui gas coin".to_owned(),
            icon_url: None,
        },
    );
    assert_eq!(metadata_object.id(), metadata_id);

    let server = MockServer::start().await;
    mock_type_inventory(
        &server,
        checkpoint,
        &CoinMetadata::type_(coin_type.clone()),
        &[&metadata_object],
    )
    .await;
    mock_type_inventory(
        &server,
        checkpoint,
        &TreasuryCap::type_(coin_type.clone()),
        &[],
    )
    .await;
    mock_type_inventory(
        &server,
        checkpoint,
        &RegulatedCoinMetadata::type_(coin_type.clone()),
        &[],
    )
    .await;

    let (store, _runtime) = test_data_store_with_remote(temp.path(), server.uri(), checkpoint);

    let reader = fork_rpc_reader(&store);
    let info = RpcIndexes::get_coin_info(&reader, &coin_type)
        .expect("coin-info lookup should initialize type inventories")
        .expect("coin info should be assembled from indexed wrapper objects");
    assert_eq!(info.coin_metadata_object_id, Some(metadata_id));
    assert_eq!(info.treasury_object_id, None);
    assert_eq!(info.regulated_coin_metadata_object_id, None);
}

#[tokio::test]
async fn test_address_inventory_does_not_resurrect_locally_moved_objects() {
    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let checkpoint = 42;
    let owner = SuiAddress::random_for_testing_only();
    let recipient = SuiAddress::random_for_testing_only();
    let first_id = ObjectID::random();
    let remote_object = make_gas_object(first_id, 1, Owner::AddressOwner(owner));
    let transferred = make_gas_object(first_id, 2, Owner::AddressOwner(recipient));

    let server = MockServer::start().await;
    mock_address_owner_inventory(&server, checkpoint, owner, &[&remote_object]).await;
    mock_address_owner_inventory(&server, checkpoint, recipient, &[]).await;

    let (mut store, _runtime) = test_data_store_with_remote(temp.path(), server.uri(), checkpoint);
    store.update_objects(BTreeMap::from([(first_id, transferred)]), vec![]);

    let reader = fork_rpc_reader(&store);
    assert!(
        RpcIndexes::get_balance(&reader, &owner, &GAS::type_())
            .expect("balance lookup should initialize address inventory")
            .is_none(),
        "remote address inventory must not re-credit an object already moved locally",
    );

    let owner_infos: Vec<_> =
        RpcIndexes::owned_objects_iter(&reader, owner, Some(GasCoin::type_()), None)
            .expect("owned-object iterator should read initialized address inventory")
            .map(|result| result.expect("owned-object entry should decode"))
            .collect();
    assert!(owner_infos.is_empty());

    // The recipient's owner-index row is written by the embedded indexer at
    // checkpoint publication, not synchronously by the local transfer. Before
    // the indexer runs, the index stays empty while canonical reads already
    // serve the transferred version.
    let recipient_infos: Vec<_> =
        RpcIndexes::owned_objects_iter(&reader, recipient, Some(GasCoin::type_()), None)
            .expect("owned-object iterator should read initialized address inventory")
            .map(|result| result.expect("owned-object entry should decode"))
            .collect();
    assert!(recipient_infos.is_empty());
    assert_eq!(
        ForkStore::get_object(&store, &first_id)
            .expect("current object read should not error")
            .unwrap()
            .version(),
        SequenceNumber::from_u64(2),
    );
}

#[tokio::test]
async fn test_rpc_reader_latest_ignores_stale_cached_history() {
    // A pre-fork object whose true current-at-fork version is 9, while the
    // local store holds only a cached historical row at version 5 (the exact
    // state a bounded child read or exact-version read leaves behind: raw row,
    // no live-state pointer). A latest read through the RPC reader must not
    // trust the sparse objects CF's highest row — it must consult the fork's
    // currency authority and fetch the real current version.
    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let checkpoint = 42;
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let stale = make_gas_object(object_id, 5, Owner::AddressOwner(owner));
    let current = make_gas_object(object_id, 9, Owner::AddressOwner(owner));

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_partial_json(serde_json::json!({
            "variables": {
                "keys": [{
                    "address": object_id.to_string(),
                    "atCheckpoint": checkpoint,
                }]
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(objects_response(&[Some(&current)])))
        .mount(&server)
        .await;

    let (store, _runtime) = test_data_store_with_remote(temp.path(), server.uri(), checkpoint);
    store
        .local_store()
        .save_object_version_only(&stale)
        .expect("historical row should save");

    let reader = fork_rpc_reader(&store);
    let got = sui_types::storage::ObjectStore::get_object(&reader, &object_id)
        .expect("latest read should resolve the current version");
    assert_eq!(
        got.version(),
        SequenceNumber::from_u64(9),
        "reader must serve the current-at-fork version, not cached history",
    );

    // The fetch-and-persist leg must also have recorded currency.
    assert_eq!(
        store
            .local_store()
            .get_latest_object_status(object_id)
            .unwrap(),
        Some((SequenceNumber::from_u64(9), Status::Live(current))),
    );
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

    // Transaction rows are keyed by checkpoint position in `sui-rpc-store`,
    // so they are saved once the pending transaction is checkpointed.
    let _checkpoint = sim.create_checkpoint();
    let tx_digest = effects.transaction_digest();
    let persisted = sim
        .store()
        .get_transaction(tx_digest)
        .expect("transaction read should not error");
    assert!(persisted.is_some(), "transaction not persisted");

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

    // Transaction rows are keyed by checkpoint position in `sui-rpc-store`,
    // so they are saved once the pending transaction is checkpointed.
    let _checkpoint = sim.create_checkpoint();
    let tx_digest = effects.transaction_digest();
    assert!(
        sim.store()
            .get_transaction(tx_digest)
            .expect("transaction read should not error")
            .is_some(),
        "transaction not persisted",
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
fn test_owned_objects_reads_seeded_index_across_owner_moves() {
    let (_temp, store) = test_data_store();
    let owner = SuiAddress::random_for_testing_only();
    let recipient = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = make_gas_object(object_id, 1, Owner::AddressOwner(owner));

    // Pre-fork materialization (the seed/inventory path) writes the owner
    // index synchronously; `owned_objects` joins those rows against current
    // canonical state.
    store
        .local_store()
        .save_address_owned_seed_object(&object)
        .unwrap();
    let owner_objects: Vec<_> = SimulatorStore::owned_objects(&store, owner).collect();
    assert_eq!(owner_objects.len(), 1);
    assert_eq!(owner_objects[0].id(), object_id);

    let transferred = make_gas_object(object_id, 2, Owner::AddressOwner(recipient));
    store
        .local_store()
        .save_address_owned_seed_object(&transferred)
        .unwrap();

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
fn test_owned_objects_tracks_consensus_address_owner_writes() {
    let (_temp, store) = test_data_store();
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = make_gas_object(
        object_id,
        1,
        Owner::ConsensusAddressOwner {
            start_version: SequenceNumber::from_u64(1),
            owner,
        },
    );

    // The seed/inventory path collapses ConsensusAddressOwner into the
    // address-owner index kind.
    store
        .local_store()
        .save_address_owned_seed_object(&object)
        .unwrap();

    let reader = store.local_store().reader().clone();
    let infos: Vec<_> =
        RpcIndexes::owned_objects_iter(&reader, owner, Some(GasCoin::type_()), None)
            .expect("owned-object iterator should build")
            .map(|result| result.expect("owned-object entry should decode"))
            .collect();
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].owner, owner);
    assert_eq!(infos[0].object_id, object_id);
    assert_eq!(infos[0].version, SequenceNumber::from_u64(1));
    assert_eq!(infos[0].balance, Some(1_000_000));

    // Indexing a newer non-address-owned version removes the address row.
    let immutable = make_gas_object(object_id, 2, Owner::Immutable);
    store
        .local_store()
        .save_type_inventory_object(&immutable)
        .unwrap();
    assert_eq!(
        RpcIndexes::owned_objects_iter(&reader, owner, Some(GasCoin::type_()), None)
            .expect("owned-object iterator should build")
            .count(),
        0,
    );
}

#[test]
fn test_read_child_object_uses_highest_local_version_within_bound() {
    let (_temp, store) = test_data_store();
    let parent = ObjectID::random();
    let child_id = ObjectID::random();
    let child_v5 = make_gas_object(child_id, 5, Owner::ObjectOwner(parent.into()));
    let child_v7 = make_gas_object(child_id, 7, Owner::ObjectOwner(parent.into()));

    let local_store = store.local_store();
    local_store.save_object_version_only(&child_v5).unwrap();
    local_store.save_object_version_only(&child_v7).unwrap();

    let child = sui_types::storage::RuntimeObjectResolver::read_child_object(
        &store,
        &parent,
        &child_id,
        SequenceNumber::from_u64(6),
    )
    .expect("bounded child read should not error")
    .expect("child object should be found");

    assert_eq!(child, child_v5);
}

#[tokio::test]
async fn test_read_child_object_falls_back_to_remote_root_version() {
    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let checkpoint = 42;
    let parent = ObjectID::random();
    let child_id = ObjectID::random();
    let child = make_gas_object(child_id, 5, Owner::ObjectOwner(parent.into()));

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_partial_json(serde_json::json!({
            "variables": {
                "keys": [
                    {
                        "address": child_id.to_string(),
                        "rootVersion": 6,
                    },
                ],
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(objects_response(&[Some(&child)])))
        .mount(&server)
        .await;

    let (store, _runtime) = test_data_store_with_remote(temp.path(), server.uri(), checkpoint);
    let read = sui_types::storage::RuntimeObjectResolver::read_child_object(
        &store,
        &parent,
        &child_id,
        SequenceNumber::from_u64(6),
    )
    .expect("remote bounded child read should not error")
    .expect("child object should be found");

    assert_eq!(read, child);
    assert_eq!(
        ForkStore::get_object_at_version(&store, &child_id, 5).unwrap(),
        Some(child),
    );
}

#[test]
fn test_read_child_object_rejects_wrong_owner_after_bounded_lookup() {
    let (_temp, store) = test_data_store();
    let parent = ObjectID::random();
    let other_parent = ObjectID::random();
    let child_id = ObjectID::random();
    let child = make_gas_object(child_id, 5, Owner::ObjectOwner(other_parent.into()));

    store
        .local_store()
        .save_object_version_only(&child)
        .unwrap();

    let err = sui_types::storage::RuntimeObjectResolver::read_child_object(
        &store,
        &parent,
        &child_id,
        SequenceNumber::from_u64(6),
    )
    .expect_err("wrong child owner should error");

    assert!(matches!(
        err.as_inner(),
        sui_types::error::SuiErrorKind::InvalidChildObjectAccess {
            object,
            given_parent,
            actual_owner,
        } if *object == child_id && *given_parent == parent && actual_owner == &child.owner
    ));
}

#[test]
fn test_local_deletion_removes_current_object_but_preserves_historical_lookup() {
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
        ForkStore::get_object(&store, &object_id)
            .expect("current object read should not error")
            .is_none(),
        "current object lookup must not fall back to the remote after local deletion",
    );
    assert_eq!(
        ForkStore::get_object_at_version(&store, &object_id, 1)
            .expect("exact version read should not error")
            .unwrap(),
        object,
    );
    assert_eq!(
        sui_types::storage::ObjectStore::get_object_by_key(
            &store,
            &object_id,
            SequenceNumber::from_u64(1),
        )
        .expect("execution-facing exact-version lookup should read local history"),
        object,
    );
}

#[test]
fn test_local_wrap_removes_current_object_but_preserves_historical_lookup() {
    let (_temp, mut store) = test_data_store();
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = make_gas_object(object_id, 1, Owner::AddressOwner(owner));

    store.update_objects(BTreeMap::from([(object_id, object.clone())]), vec![]);
    let result = store.apply_object_updates(
        BTreeMap::new(),
        vec![ObjectRemoval {
            object_id,
            version: SequenceNumber::from_u64(2),
            kind: TombstoneKind::Wrapped,
        }],
    );
    assert!(result.is_ok(), "object updates should apply: {result:?}");

    assert_eq!(SimulatorStore::owned_objects(&store, owner).count(), 0);
    assert!(
        ForkStore::get_object(&store, &object_id)
            .expect("current object read should not error")
            .is_none(),
        "current object lookup must not fall back to the remote after local wrapping",
    );
    assert_eq!(
        ForkStore::get_object_at_version(&store, &object_id, 1)
            .expect("exact version read should not error")
            .unwrap(),
        object,
    );
    assert_eq!(
        sui_types::storage::ObjectStore::get_object_by_key(
            &store,
            &object_id,
            SequenceNumber::from_u64(1),
        )
        .expect("execution-facing exact-version lookup should read local history"),
        object,
    );
}

#[test]
fn test_unwrapped_write_clears_wrapped_latest() {
    let (_temp, mut store) = test_data_store();
    let owner = SuiAddress::random_for_testing_only();
    let recipient = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = make_gas_object(object_id, 1, Owner::AddressOwner(owner));

    store.update_objects(BTreeMap::from([(object_id, object)]), vec![]);
    let result = store.apply_object_updates(
        BTreeMap::new(),
        vec![ObjectRemoval {
            object_id,
            version: SequenceNumber::from_u64(2),
            kind: TombstoneKind::Wrapped,
        }],
    );
    assert!(result.is_ok(), "object updates should apply: {result:?}");

    let unwrapped = make_gas_object(object_id, 3, Owner::AddressOwner(recipient));
    let result =
        store.apply_object_updates(BTreeMap::from([(object_id, unwrapped.clone())]), vec![]);
    assert!(result.is_ok(), "object updates should apply: {result:?}");

    assert_eq!(
        ForkStore::get_object(&store, &object_id)
            .expect("current object read should not error")
            .unwrap(),
        unwrapped,
    );
}

#[test]
fn test_terminal_deleted_latest_prevents_reindexing_written_object() {
    let (_temp, mut store) = test_data_store();
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = make_gas_object(object_id, 1, Owner::AddressOwner(owner));
    let written_again = make_gas_object(object_id, 3, Owner::AddressOwner(owner));

    store.update_objects(BTreeMap::from([(object_id, object)]), vec![]);
    let result = store.apply_object_updates(
        BTreeMap::from([(object_id, written_again)]),
        vec![ObjectRemoval {
            object_id,
            version: SequenceNumber::from_u64(2),
            kind: TombstoneKind::Deleted,
        }],
    );
    assert!(result.is_ok(), "object updates should apply: {result:?}");

    assert_eq!(SimulatorStore::owned_objects(&store, owner).count(), 0);
    assert!(
        ForkStore::get_object(&store, &object_id)
            .expect("current object read should not error")
            .is_none(),
    );
}

#[test]
fn test_removed_objects_from_effects_maps_to_tombstones() {
    let owner = SuiAddress::random_for_testing_only();
    let deleted_id = ObjectID::random();
    let deleted_ref = (
        deleted_id,
        SequenceNumber::from_u64(2),
        ObjectDigest::OBJECT_DIGEST_DELETED,
    );
    let unwrapped_then_deleted_id = ObjectID::random();
    let unwrapped_then_deleted_ref = (
        unwrapped_then_deleted_id,
        SequenceNumber::from_u64(3),
        ObjectDigest::OBJECT_DIGEST_DELETED,
    );
    let wrapped_id = ObjectID::random();
    let wrapped_ref = (
        wrapped_id,
        SequenceNumber::from_u64(4),
        ObjectDigest::OBJECT_DIGEST_WRAPPED,
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
        vec![deleted_ref],
        vec![unwrapped_then_deleted_ref],
        vec![wrapped_ref],
        (gas_ref, Owner::AddressOwner(owner)),
        None,
        vec![],
    );

    assert_eq!(
        removed_objects_from_effects(&effects),
        vec![
            ObjectRemoval {
                object_id: deleted_id,
                version: deleted_ref.1,
                kind: TombstoneKind::Deleted,
            },
            ObjectRemoval {
                object_id: unwrapped_then_deleted_id,
                version: unwrapped_then_deleted_ref.1,
                kind: TombstoneKind::Deleted,
            },
            ObjectRemoval {
                object_id: wrapped_id,
                version: wrapped_ref.1,
                kind: TombstoneKind::Wrapped,
            },
        ],
    );
}

#[test]
fn test_rpc_owned_objects_iter_filters_and_pages_by_object_id() {
    let (_temp, store) = test_data_store();
    let owner = SuiAddress::random_for_testing_only();
    let other_owner = SuiAddress::random_for_testing_only();
    let first_id = ObjectID::random();
    let second_id = ObjectID::random();
    let other_id = ObjectID::random();
    let first = make_gas_object(first_id, 1, Owner::AddressOwner(owner));
    let second = make_gas_object(second_id, 1, Owner::AddressOwner(owner));
    let other = make_gas_object(other_id, 1, Owner::AddressOwner(other_owner));

    for object in [&first, &second, &other] {
        store
            .local_store()
            .save_address_owned_seed_object(object)
            .unwrap();
    }

    let reader = store.local_store().reader().clone();
    let infos: Vec<_> =
        RpcIndexes::owned_objects_iter(&reader, owner, Some(GasCoin::type_()), None)
            .expect("owned-object iterator should build")
            .map(|result| result.expect("owned-object entry should decode"))
            .collect();
    assert_eq!(infos.len(), 2);
    assert!(infos[0].object_id < infos[1].object_id);
    assert!(infos.iter().all(|info| info.owner == owner));
    assert!(infos.iter().all(|info| info.balance == Some(1_000_000)));

    let wrong_type = "0x2::clock::Clock".parse::<StructTag>().unwrap();
    assert_eq!(
        RpcIndexes::owned_objects_iter(&reader, owner, Some(wrong_type), None)
            .expect("owned-object iterator should build")
            .count(),
        0,
    );

    // The rpc-store cursor is inclusive: it carries the full sort position of
    // the first *unread* object, and the resumed scan seeks straight to it. So
    // resuming from `infos[1]` yields exactly that trailing object.
    let page_from_cursor: Vec<_> = RpcIndexes::owned_objects_iter(
        &reader,
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
    let (_temp, store) = test_data_store();
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = make_gas_object(object_id, 1, Owner::AddressOwner(owner));
    store
        .local_store()
        .save_address_owned_seed_object(&object)
        .unwrap();

    let cloned_store = store.clone();
    let local_snapshot_guard = store
        .write_local_snapshot()
        .expect("snapshot lock should not be poisoned");
    assert!(
        cloned_store.inner.local_snapshot_lock.try_read().is_err(),
        "cloned stores should share the same snapshot guard",
    );
    drop(local_snapshot_guard);

    let reader = cloned_store.local_store().reader().clone();
    let infos: Vec<_> =
        RpcIndexes::owned_objects_iter(&reader, owner, Some(GasCoin::type_()), None)
            .expect("owned-object iterator should build")
            .map(|result| result.expect("owned-object entry should decode"))
            .collect();
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].object_id, object_id);
}
