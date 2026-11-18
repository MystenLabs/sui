// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end execution tests. They build a `Simulacrum<OsRng, ForkStore>` over a tempdir-backed
//! fork store, execute transactions, and assert the resulting state is saved. Wired via
//! `#[cfg(test)] #[path]` in `store.rs`, so `super::*` resolves into the `store` module.

use std::num::NonZeroUsize;
use std::path::Path;
use std::time::Duration;

use fastcrypto::encoding::Base64 as FastCryptoBase64;
use rand::rngs::OsRng;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::body_partial_json;
use wiremock::matchers::body_string_contains;
use wiremock::matchers::method;
use wiremock::matchers::path;

use simulacrum::Simulacrum;
use simulacrum::store::SimulatorStore;
use simulacrum::store::in_mem_store::KeyStore;
use sui_swarm_config::network_config::NetworkConfig;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::base_types::SuiAddress;
use sui_types::coin::CoinMetadata;
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
use crate::seed::SeedEntry;
use crate::seed::SeedManifest;
use crate::seed::load_seed_objects;
use crate::services::ServiceManager;

/// Build a `Simulacrum<OsRng, ForkStore>` from a fresh genesis NetworkConfig.
///
/// The ForkStore's local metadata and RPC store live in the returned tempdir, and its remote
/// endpoint is fake and never called. Genesis objects are populated directly via `update_objects`
/// to avoid touching the `init_with_genesis` checkpoint and committee paths (which are still
/// `todo!()`). Returns the simulacrum, the underlying NetworkConfig (so tests can find genesis
/// objects and account keys), and the tempdir guarding the local store.
async fn test_simulacrum() -> (
    Simulacrum<OsRng, ForkStore>,
    NetworkConfig,
    tempfile::TempDir,
    MockServer,
) {
    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let mut rng = OsRng;
    let config = ConfigBuilder::new_with_temp_dir()
        .rng(&mut rng)
        .deterministic_committee_size(NonZeroUsize::MIN)
        .build();

    let services = open_test_services(temp.path(), 0);
    let gql_server = crate::test_support::absent_objects_gql_server().await;
    let mut store = ForkStore::new_for_testing_with_remote(
        temp.path().to_path_buf(),
        gql_server.uri(),
        0,
        services.local_store(),
    );
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
        (*config.genesis.checkpoint().digest()).into(),
        &config,
        store,
        rng,
    );
    (sim, config, temp, gql_server)
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

fn data_store() -> (tempfile::TempDir, ForkStore) {
    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let services = open_test_services(temp.path(), 0);
    let store = ForkStore::new_for_testing(temp.path().to_path_buf(), services.local_store());
    (temp, store)
}

fn data_store_with_remote(
    root: &Path,
    gql_url: String,
    forked_at_checkpoint: CheckpointSequenceNumber,
) -> (ForkStore, ServiceManager) {
    let services = open_test_services(root, forked_at_checkpoint);
    let store = ForkStore::new_for_testing_with_remote(
        root.to_path_buf(),
        gql_url,
        forked_at_checkpoint,
        services.local_store(),
    );
    (store, services)
}

fn open_test_services(
    root: &Path,
    forked_at_checkpoint: CheckpointSequenceNumber,
) -> ServiceManager {
    ServiceManager::open(
        root,
        "custom".to_owned(),
        forked_at_checkpoint,
        CheckpointDigest::new([9; 32]).into(),
    )
    .expect("service manager should open")
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

/// Exact versions pinned at a checkpoint are fetched through a checkpoint-scoped `multiGetObjects`,
/// so the response is a list even for a single object.
fn object_at_checkpoint_response(objects: &[&Object]) -> serde_json::Value {
    serde_json::json!({
        "data": {
            "checkpoint": {
                "query": {
                    "multiGetObjects": objects
                        .iter()
                        .map(|object| serde_json::json!({
                            "address": object.id().to_string(),
                            "version": object.version().value(),
                            "objectBcs": FastCryptoBase64::from_bytes(
                                &bcs::to_bytes(object).expect("object should serialize"),
                            )
                            .encoded(),
                        }))
                        .collect::<Vec<_>>(),
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

async fn mock_seed_object(server: &MockServer, checkpoint: u64, object: &Object) {
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_partial_json(serde_json::json!({
            "variables": {
                "sequenceNumber": checkpoint,
                "keys": [{
                    "address": object.id().to_string(),
                    "version": object.version().value(),
                }],
            }
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(object_at_checkpoint_response(&[object])),
        )
        .mount(server)
        .await;
}

/// Serve the seed load's request for every system package at `checkpoint`, answering with
/// `packages` and with no object for every other system package id.
async fn mock_system_packages(server: &MockServer, checkpoint: u64, packages: &[Object]) {
    let package_ids: Vec<ObjectID> = sui_types::SYSTEM_PACKAGE_ADDRESSES
        .iter()
        .copied()
        .map(ObjectID::from)
        .collect();
    let keys: Vec<_> = package_ids
        .iter()
        .map(|id| serde_json::json!({ "address": id.to_string(), "atCheckpoint": checkpoint }))
        .collect();
    let objects: Vec<_> = package_ids
        .iter()
        .map(|id| packages.iter().find(|package| package.id() == *id))
        .collect();
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_partial_json(
            serde_json::json!({ "variables": { "keys": keys } }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(objects_response(&objects)))
        .mount(server)
        .await;
}

/// System package objects at `(id, version)`, each with the modules genesis publishes at `id`: a
/// package past its first version names its original id through its modules' address.
fn system_package_objects(versions: &[(ObjectID, u64)]) -> Vec<Object> {
    let config = ConfigBuilder::new_with_temp_dir()
        .rng(&mut OsRng)
        .deterministic_committee_size(NonZeroUsize::MIN)
        .build();
    versions
        .iter()
        .map(|&(id, version)| {
            let genesis_package = config
                .genesis
                .objects()
                .iter()
                .find(|object| object.id() == id)
                .and_then(|object| object.data.try_as_package())
                .expect("genesis publishes every system package");
            Object::new_package_from_data(
                sui_types::object::Data::Package(
                    sui_types::move_package::MovePackage::new(
                        id,
                        SequenceNumber::from_u64(version),
                        genesis_package.serialized_module_map().clone(),
                        u64::MAX,
                        vec![],
                        BTreeMap::new(),
                    )
                    .unwrap(),
                ),
                TransactionDigest::genesis_marker(),
            )
        })
        .collect()
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

    let (store, services) = data_store_with_remote(temp.path(), server.uri(), checkpoint);

    let read = ForkStore::get_object(&store, &object_id)
        .expect("current object read should not error")
        .expect("remote object should be found");
    assert_eq!(read, object);

    let reader = services.reader();
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
    let (_temp, mut store) = data_store();
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

/// Index reads resolve from the seed load and local execution alone.
///
/// An owner, parent, or coin type outside the seed set reads as empty, because the
/// checkpoint-pinned enumeration that could resolve it belongs to fork creation and cannot be
/// re-run at read time. The endpoint here is deliberately unreachable, so these calls only succeed
/// because no remote scan is attempted.
#[tokio::test]
async fn test_index_reads_are_seed_bounded_and_never_scan_the_remote() {
    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let checkpoint = 42;
    let (store, _services) =
        data_store_with_remote(temp.path(), "http://localhost:1".to_owned(), checkpoint);
    let reader = store.local_store().reader().clone();

    let seeded_owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = make_gas_object(object_id, 7, Owner::AddressOwner(seeded_owner));
    store.local_store().restore_seed_objects(&[object]).unwrap();

    // A seeded owner has the full derived surface: owner index, balance, and
    // balance iteration.
    let infos: Vec<_> =
        RpcIndexes::owned_objects_iter(&reader, seeded_owner, Some(GasCoin::type_()), None)
            .expect("owned-object iterator should read the seeded index")
            .map(|result| result.expect("owned-object entry should decode"))
            .collect();
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].owner, seeded_owner);
    assert_eq!(infos[0].object_id, object_id);
    assert_eq!(infos[0].version, SequenceNumber::from_u64(7));
    assert_eq!(infos[0].balance, Some(1_000_000));

    let balance = RpcIndexes::get_balance(&reader, &seeded_owner, &GAS::type_())
        .expect("balance lookup should read the seeded index")
        .expect("gas balance should exist");
    assert_eq!(balance.coin_balance, 1_000_000);
    assert_eq!(balance.address_balance, 0);

    let balances: Vec<_> = RpcIndexes::balance_iter(&reader, &seeded_owner, None)
        .expect("balance iteration should read the seeded index")
        .map(|entry| entry.expect("balance row should decode"))
        .collect();
    assert_eq!(balances.len(), 1);
    assert_eq!(balances[0].0, GAS::type_());
    assert_eq!(balances[0].1.coin_balance, 1_000_000);

    // Everything outside the seed set answers empty rather than erroring or
    // reaching for the remote.
    let unseeded = SuiAddress::random_for_testing_only();
    assert_eq!(
        RpcIndexes::owned_objects_iter(&reader, unseeded, Some(GasCoin::type_()), None)
            .expect("unseeded owner read should not error")
            .count(),
        0,
    );
    assert!(
        RpcIndexes::get_balance(&reader, &unseeded, &GAS::type_())
            .expect("unseeded balance read should not error")
            .is_none(),
    );
    assert_eq!(
        RpcIndexes::dynamic_field_iter(&reader, ObjectID::random(), None)
            .expect("unseeded parent read should not error")
            .count(),
        0,
    );
    assert!(
        RpcIndexes::get_coin_info(&reader, &GAS::type_())
            .expect("unseeded coin-info read should not error")
            .is_none(),
    );
}

#[tokio::test]
async fn test_seed_load_survives_restart_without_remote() {
    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let checkpoint = 42;
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = make_gas_object(object_id, 7, Owner::AddressOwner(owner));

    let server = MockServer::start().await;
    mock_seed_object(&server, checkpoint, &object).await;
    let system_packages = system_package_objects(&[
        (ObjectID::from_single_byte(1), 1),
        (ObjectID::from_single_byte(2), 1),
        (ObjectID::from_single_byte(3), 1),
    ]);
    mock_system_packages(&server, checkpoint, &system_packages).await;
    let manifest = SeedManifest {
        network: "custom".to_owned(),
        checkpoint,
        addresses: vec![owner],
        entries: vec![SeedEntry {
            object_ref: object.compute_object_reference(),
        }],
    };

    {
        let (store, services) = data_store_with_remote(temp.path(), server.uri(), checkpoint);
        load_seed_objects(&store, &manifest).expect("seed load should hydrate from the remote");

        let reader = store.local_store().reader().clone();
        let infos: Vec<_> =
            RpcIndexes::owned_objects_iter(&reader, owner, Some(GasCoin::type_()), None)
                .expect("owned-object iterator should read the seeded index")
                .map(|result| result.expect("owned-object entry should decode"))
                .collect();
        assert_eq!(infos.len(), 1);
        let balance = RpcIndexes::get_balance(&reader, &owner, &GAS::type_())
            .expect("balance lookup should read the seeded index")
            .expect("gas balance should exist");
        assert_eq!(balance.coin_balance, 1_000_000);
        drop(services);
    }

    // Reopened against an unreachable endpoint: everything the seed established
    // is durable, and the load recognizes that it already ran rather than
    // re-fetching (which would fail here) or re-merging the balance (which
    // would silently double it).
    let (store, _services) =
        data_store_with_remote(temp.path(), "http://localhost:1".to_owned(), checkpoint);
    load_seed_objects(&store, &manifest).expect("a resumed seed load must not touch the remote");

    let reader = store.local_store().reader().clone();
    let infos: Vec<_> =
        RpcIndexes::owned_objects_iter(&reader, owner, Some(GasCoin::type_()), None)
            .expect("owned-object iterator should read the reopened seed index")
            .map(|result| result.expect("owned-object entry should decode"))
            .collect();
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].object_id, object_id);
    assert_eq!(
        RpcIndexes::get_balance(&reader, &owner, &GAS::type_())
            .expect("balance lookup should succeed")
            .expect("gas balance should exist")
            .coin_balance,
        1_000_000,
        "a resumed seed load must not re-credit the balance",
    );
}

/// `ObjectByOwner::restore` keys object-owned children under their parent, so a seeded child is
/// reachable as a dynamic field without any read-time scan.
#[test]
fn test_rpc_dynamic_field_iter_reads_seeded_object_owner_index() {
    let (_temp, store) = data_store();
    let parent = ObjectID::random();
    let child_id = ObjectID::random();
    let child = make_gas_object(child_id, 7, Owner::ObjectOwner(parent.into()));

    store.local_store().restore_seed_objects(&[child]).unwrap();

    let reader = store.local_store().reader().clone();
    let fields: Vec<_> = RpcIndexes::dynamic_field_iter(&reader, parent, None)
        .expect("dynamic-field iterator should read the seeded object-owner index")
        .map(|result| result.expect("dynamic-field row should decode"))
        .collect();

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].parent, parent);
    assert_eq!(fields[0].field_id, child_id);
}

/// Coin metadata is assembled from three type-keyed lookups against the type index, which
/// `ObjectByType::restore` populates during the seed load.
#[test]
fn test_rpc_get_coin_info_reads_seeded_type_index() {
    let (_temp, store) = data_store();
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

    store
        .local_store()
        .restore_seed_objects(&[metadata_object])
        .unwrap();

    let reader = store.local_store().reader().clone();
    let info = RpcIndexes::get_coin_info(&reader, &coin_type)
        .expect("coin-info lookup should read the seeded type index")
        .expect("coin info should be assembled from indexed wrapper objects");
    assert_eq!(info.coin_metadata_object_id, Some(metadata_id));
    assert_eq!(info.treasury_object_id, None);
    assert_eq!(info.regulated_coin_metadata_object_id, None);
}

#[tokio::test]
async fn test_rpc_latest_read_ignores_stale_cached_history() {
    // A pre-fork object whose true current-at-fork version is 9, while the
    // local store holds only a cached historical row at version 5 (the exact
    // state a bounded child read or exact-version read leaves behind: raw row,
    // no current-version row). A latest read through the RPC reader must not
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

    let (store, _services) = data_store_with_remote(temp.path(), server.uri(), checkpoint);
    store
        .local_store()
        .save_object_version_only(&stale)
        .expect("historical row should save");

    let reader = store.clone();
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

async fn mock_available_range(server: &MockServer, first_sequence_number: u64) {
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_string_contains("availableRange"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {
                "serviceConfig": {
                    "availableRange": {
                        "first": { "sequenceNumber": first_sequence_number }
                    }
                }
            }
        })))
        .mount(server)
        .await;
}

/// A remote whose retention window has passed the fork point must not make the fork report its own
/// locally held range as unavailable.
#[tokio::test]
async fn test_availability_floor_is_clamped_at_the_fork_checkpoint() {
    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let server = MockServer::start().await;
    mock_available_range(&server, 5000).await;

    let (store, _services) = data_store_with_remote(temp.path(), server.uri(), 100);
    assert_eq!(store.get_lowest_available_checkpoint().unwrap(), 100);
    assert_eq!(
        store.get_lowest_available_checkpoint_objects().unwrap(),
        100
    );
}

/// A remote floor below the fork point passes through unchanged, so pre-fork availability keeps
/// tracking the remote's retention window.
#[tokio::test]
async fn test_availability_floor_below_the_fork_passes_through() {
    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let server = MockServer::start().await;
    mock_available_range(&server, 10).await;

    let (store, _services) = data_store_with_remote(temp.path(), server.uri(), 100);
    assert_eq!(store.get_lowest_available_checkpoint().unwrap(), 10);
    assert_eq!(store.get_lowest_available_checkpoint_objects().unwrap(), 10);
}

/// The fork's indexes record only the fork's own checkpoints and load pre-fork packages as of the
/// fork checkpoint, without the history before it, so checkpoint-pinned reads below the fork
/// checkpoint are rejected even when the remote's retention reaches further back; at the fork
/// checkpoint they are served. A listing's checkpoint range starts from the version as of the
/// checkpoint before `start_checkpoint`, so the lowest one it accepts is the checkpoint after the
/// fork, and a range ending at or below that checkpoint is refused too. The listing of every
/// package, which needs each version's publish checkpoint, starts after it.
#[tokio::test]
async fn test_checkpoint_pinned_reads_below_the_fork_checkpoint_are_rejected() {
    use sui_rpc_api::RpcService;
    use sui_rpc_api::grpc::v2::move_package_service::PackageVersionCursor;
    use sui_rpc_api::proto::sui::rpc::v2::GetPackageRequest;
    use sui_rpc_api::proto::sui::rpc::v2::ListPackageVersionsRequest;
    use sui_rpc_api::proto::sui::rpc::v2::QueryEndReason;
    use sui_rpc_api::proto::sui::rpc::v2::QueryOptions;
    use sui_rpc_api::proto::sui::rpc::v2::get_package_request::Selector;
    use sui_rpc_api::proto::sui::rpc::v2::move_package_service_server::MovePackageService;

    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let server = MockServer::start().await;
    let fork_checkpoint = 100;
    // The remote retains history from checkpoint 10, below the fork checkpoint.
    mock_available_range(&server, 10).await;
    let (store, _services) = data_store_with_remote(temp.path(), server.uri(), fork_checkpoint);
    assert_eq!(
        sui_types::storage::RpcStateReader::get_lowest_checkpoint_with_index_history(&store)
            .unwrap(),
        fork_checkpoint + 1
    );

    let package_id = ObjectID::random();
    let package = sui_types::object::Object::new_package_from_data(
        sui_types::object::Data::Package(
            sui_types::move_package::MovePackage::new(
                package_id,
                SequenceNumber::from_u64(1),
                std::collections::BTreeMap::new(),
                100_000,
                vec![],
                std::collections::BTreeMap::new(),
            )
            .unwrap(),
        ),
        TransactionDigest::genesis_marker(),
    );
    store
        .local_store()
        .restore_seed_objects(&[package])
        .unwrap();
    let service = RpcService::new(Arc::new(store));

    let list = |range: [Option<u64>; 2]| {
        let mut request = ListPackageVersionsRequest::default();
        request.package_id = Some(package_id.to_string());
        [request.start_checkpoint, request.end_checkpoint] = range;
        tonic::Request::new(request)
    };
    let below_range_floor =
        "requested data below earliest available; lowest available checkpoint is 101";
    for range in [
        [Some(fork_checkpoint), None],
        [None, Some(fork_checkpoint)],
        [None, Some(fork_checkpoint + 1)],
    ] {
        let error = service
            .list_package_versions(list(range))
            .await
            .unwrap_err();
        assert_eq!(error.code(), tonic::Code::OutOfRange, "{range:?}");
        assert_eq!(error.message(), below_range_floor, "{range:?}");
    }
    // The version loaded at the fork was published by its end, before the lowest accepted
    // `start_checkpoint`, so a range from there holds none of it.
    for (range, expected) in [
        ([Some(fork_checkpoint + 1), None], vec![]),
        ([None, Some(fork_checkpoint + 2)], vec![(Some(1), None)]),
    ] {
        let versions = service
            .list_package_versions(list(range))
            .await
            .unwrap()
            .into_inner()
            .versions;
        let listed: Vec<_> = versions
            .iter()
            .map(|version| (version.version, version.checkpoint))
            .collect();
        assert_eq!(listed, expected, "{range:?}");
    }
    let below_floor = "requested data below earliest available; lowest available checkpoint is 100";

    // The seeded package has no publish checkpoint, so the listing of every package covers only
    // the checkpoints after the fork: a range starting at or before the fork checkpoint is
    // refused, unless an `after` cursor past the fork bounds it, and so is one with no lower
    // bound that ends at or before the checkpoint after the fork. The fork indexes nothing past
    // its checkpoint, so every range accepted here lists nothing and ends at the ledger tip.
    let list_all = |range: [Option<u64>; 2], after: Option<u64>| {
        let mut options = QueryOptions::default();
        options.after = after.map(|checkpoint| {
            PackageVersionCursor::AllPackages {
                checkpoint,
                original_package_id: package_id,
                version: 1,
                minted_at: checkpoint,
            }
            .encode()
        });
        let mut request = ListPackageVersionsRequest::default();
        [request.start_checkpoint, request.end_checkpoint] = range;
        request.options = Some(options);
        service.list_package_versions(tonic::Request::new(request))
    };
    for (range, after) in [
        ([Some(fork_checkpoint), None], None),
        ([Some(fork_checkpoint), None], Some(fork_checkpoint)),
        ([None, Some(fork_checkpoint)], None),
        ([None, Some(fork_checkpoint + 1)], None),
    ] {
        let error = list_all(range, after).await.unwrap_err();
        assert_eq!(error.code(), tonic::Code::OutOfRange, "{range:?} {after:?}");
        assert_eq!(error.message(), below_range_floor, "{range:?} {after:?}");
    }
    for (range, after) in [
        ([Some(fork_checkpoint + 1), None], None),
        ([None, Some(fork_checkpoint + 2)], None),
        ([Some(fork_checkpoint), None], Some(fork_checkpoint + 1)),
    ] {
        let response = list_all(range, after).await.unwrap().into_inner();
        assert_eq!(
            (
                response.versions.len(),
                response.end.map(|end| end.reason())
            ),
            (0, Some(QueryEndReason::LedgerTip)),
            "{range:?} {after:?}"
        );
    }

    let mut request = GetPackageRequest::default();
    request.package_id = Some(package_id.to_string());
    request.selector = Some(Selector::AtCheckpoint(fork_checkpoint - 1));
    let error = service
        .get_package(tonic::Request::new(request))
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::OutOfRange);
    assert_eq!(error.message(), below_floor);

    // The fork checkpoint itself is served: the loaded rows hold the state at its end. Both
    // bounds accept it and the read finds the package, whose response then fails to build only
    // because this seeded package has no modules.
    let mut request = GetPackageRequest::default();
    request.package_id = Some(package_id.to_string());
    request.selector = Some(Selector::AtCheckpoint(fork_checkpoint));
    let error = service
        .get_package(tonic::Request::new(request))
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::Internal, "{error:?}");
    assert_eq!(
        error.message(),
        format!("Package has no modules: {package_id}")
    );
}

/// The fork loads a system package as of the fork checkpoint, so it stores none of the versions
/// that package replaced before the fork. `GetPackage` refuses those versions as pruned rather
/// than as versions that never existed.
#[tokio::test]
async fn test_system_package_versions_replaced_before_the_fork_are_pruned() {
    use sui_rpc_api::RpcService;
    use sui_rpc_api::proto::sui::rpc::v2::GetPackageRequest;
    use sui_rpc_api::proto::sui::rpc::v2::get_package_request::Selector;
    use sui_rpc_api::proto::sui::rpc::v2::move_package_service_server::MovePackageService;

    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let server = MockServer::start().await;
    let fork_checkpoint = 100;
    mock_available_range(&server, 10).await;
    let (store, _services) = data_store_with_remote(temp.path(), server.uri(), fork_checkpoint);

    // A system package at version 3, upgraded in place twice before the fork.
    let package_id = ObjectID::random();
    let mut module = move_binary_format::file_format::empty_module();
    module.address_identifiers[0] = package_id.into();
    let package = sui_types::object::Object::new_package_from_data(
        sui_types::object::Data::Package(sui_types::move_package::MovePackage::new_system(
            SequenceNumber::from_u64(3),
            &[module],
            [],
        )),
        TransactionDigest::genesis_marker(),
    );
    store
        .local_store()
        .restore_seed_objects(&[package])
        .unwrap();
    let service = RpcService::new(Arc::new(store));

    let get_version = |version| {
        let mut request = GetPackageRequest::default();
        request.package_id = Some(package_id.to_string());
        request.selector = Some(Selector::Version(version));
        service.get_package(tonic::Request::new(request))
    };

    for version in [1, 2] {
        let error = get_version(version).await.unwrap_err();
        assert_eq!(
            error.code(),
            tonic::Code::OutOfRange,
            "v{version}: {error:?}"
        );
        assert_eq!(
            error.message(),
            "requested data below earliest available; lowest available checkpoint is 100",
            "v{version}"
        );
    }

    let package = get_version(3).await.unwrap().into_inner().package.unwrap();
    assert_eq!(package.version, Some(3));

    // Versions this lineage never had.
    for version in [0, 4] {
        let error = get_version(version).await.unwrap_err();
        assert_eq!(error.code(), tonic::Code::NotFound, "v{version}: {error:?}");
    }
}

/// The fork has no rows for the system-package versions replaced before it, so a listing that
/// starts below the lowest version it stores starts at that version, with or without a
/// checkpoint range. A page token continuing a listing whose client holds a version below it is
/// refused as below the fork's history, naming only the lowest `start_checkpoint` it accepts,
/// rather than skip the versions in between; one whose client holds the version just below it is
/// served. A range ending at version 1 holds no version on any node, so it is an empty answer.
#[tokio::test]
async fn test_system_package_listing_below_the_lowest_stored_version_starts_there() {
    use prost::Message;
    use sui_rpc_api::RpcService;
    use sui_rpc_api::proto::google::rpc::ErrorInfo;
    use sui_rpc_api::proto::google::rpc::Status as RpcStatus;
    use sui_rpc_api::proto::sui::rpc::v2::ListPackageVersionsRequest;
    use sui_rpc_api::proto::sui::rpc::v2::ListPackageVersionsResponse;
    use sui_rpc_api::proto::sui::rpc::v2::Ordering;
    use sui_rpc_api::proto::sui::rpc::v2::QueryOptions;
    use sui_rpc_api::proto::sui::rpc::v2::move_package_service_server::MovePackageService;

    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let server = MockServer::start().await;
    let fork_checkpoint = 100;
    mock_available_range(&server, 10).await;
    let (store, _services) = data_store_with_remote(temp.path(), server.uri(), fork_checkpoint);

    // A system package at version 3, upgraded in place twice before the fork.
    let package_id = ObjectID::random();
    let mut module = move_binary_format::file_format::empty_module();
    module.address_identifiers[0] = package_id.into();
    let package = sui_types::object::Object::new_package_from_data(
        sui_types::object::Data::Package(sui_types::move_package::MovePackage::new_system(
            SequenceNumber::from_u64(3),
            &[module],
            [],
        )),
        TransactionDigest::genesis_marker(),
    );
    store
        .local_store()
        .restore_seed_objects(&[package])
        .unwrap();
    let service = RpcService::new(Arc::new(store));

    // The page token naming `version` as the first version of the next page.
    let page_token = |version: u64| -> prost::bytes::Bytes {
        bcs::to_bytes(&(package_id, version)).unwrap().into()
    };
    let list = |end_checkpoint, from_version: Option<u64>, token: Option<u64>| {
        let mut request = ListPackageVersionsRequest::default();
        request.package_id = Some(package_id.to_string());
        request.end_checkpoint = end_checkpoint;
        request.start_version = from_version;
        request.page_token = token.map(page_token);
        service.list_package_versions(tonic::Request::new(request))
    };
    let listed = |response: tonic::Response<ListPackageVersionsResponse>| {
        response
            .into_inner()
            .versions
            .iter()
            .map(|version| version.version.unwrap())
            .collect::<Vec<_>>()
    };

    for end_checkpoint in [None, Some(fork_checkpoint + 2)] {
        for from_version in [None, Some(1), Some(2), Some(3)] {
            let response = list(end_checkpoint, from_version, None).await.unwrap();
            assert_eq!(
                listed(response),
                vec![3],
                "end {end_checkpoint:?} from {from_version:?}"
            );
        }
    }

    // A page token at version 2: the client holds version 1, and version 2 is gone.
    let error = list(None, None, Some(2)).await.unwrap_err();
    assert_eq!(
        (error.code(), error.message()),
        (
            tonic::Code::OutOfRange,
            "requested data below earliest available; lowest available checkpoint is 101"
        ),
    );
    let status = RpcStatus::decode(error.details()).unwrap();
    let error_info = status
        .details
        .iter()
        .find(|detail| detail.type_url.ends_with("google.rpc.ErrorInfo"))
        .map(|detail| ErrorInfo::decode(detail.value.as_slice()).unwrap())
        .unwrap();
    assert_eq!(error_info.reason, "BELOW_EARLIEST_AVAILABLE");
    assert_eq!(
        error_info.metadata,
        [("lowestAvailableCheckpoint".to_owned(), "101".to_owned())]
            .into_iter()
            .collect()
    );

    // A page token at version 3, and a `start_version` above the one the client holds, skip
    // nothing.
    for (from_version, token) in [(None, 3), (Some(3), 2)] {
        let response = list(None, from_version, Some(token)).await.unwrap();
        assert_eq!(
            listed(response),
            vec![3],
            "from {from_version:?} token {token}"
        );
    }

    // A range below `OBJECT_START_VERSION`, in page-token mode and in descending cursor mode.
    for options in [None, Some(Ordering::Descending)] {
        let mut request = ListPackageVersionsRequest::default();
        request.package_id = Some(package_id.to_string());
        request.end_version = Some(1);
        request.options = options.map(|ordering| {
            let mut options = QueryOptions::default();
            options.ordering = Some(ordering as i32);
            options
        });
        let response = service
            .list_package_versions(tonic::Request::new(request))
            .await
            .unwrap();
        assert_eq!(listed(response), Vec::<u64>::new(), "{options:?}");
    }
}

/// The fork loads a system package at its version at the fork checkpoint, without the versions
/// below it, and a listing covers only the versions it has: one reaching below the loaded version
/// starts there, so a descending listing ends there with a short last page. A cursor naming a
/// version below it comes from a view holding them, such as kv-rpc's. A `before` cursor there
/// leaves nothing to list. An `after` cursor names a version the client holds, and when versions
/// lie between it and the loaded version, the listing is refused in either order as below where
/// the fork's history begins, naming only the lowest `start_checkpoint` it accepts, rather than
/// skip them. A cursor above every version the fork has lies past its tip: a descending listing
/// refuses it, and an ascending one lists what the fork has. All of this holds with or without an
/// `end_checkpoint`.
#[tokio::test]
async fn test_list_package_versions_by_cursor_from_below_the_fork() {
    use prost::Message;
    use sui_rpc_api::RpcService;
    use sui_rpc_api::grpc::v2::move_package_service::PackageVersionCursor;
    use sui_rpc_api::proto::google::rpc::ErrorInfo;
    use sui_rpc_api::proto::google::rpc::Status as RpcStatus;
    use sui_rpc_api::proto::sui::rpc::v2::ListPackageVersionsRequest;
    use sui_rpc_api::proto::sui::rpc::v2::ListPackageVersionsResponse;
    use sui_rpc_api::proto::sui::rpc::v2::Ordering;
    use sui_rpc_api::proto::sui::rpc::v2::QueryOptions;
    use sui_rpc_api::proto::sui::rpc::v2::move_package_service_server::MovePackageService;

    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let server = MockServer::start().await;
    let fork_checkpoint = 100;
    mock_available_range(&server, 10).await;
    let (store, _services) = data_store_with_remote(temp.path(), server.uri(), fork_checkpoint);
    let framework = ObjectID::from_single_byte(2);
    store
        .local_store()
        .restore_seed_objects(&system_package_objects(&[(framework, 3)]))
        .unwrap();
    let highest_indexed = sui_types::storage::RpcStateReader::indexes(&store)
        .unwrap()
        .get_highest_indexed_checkpoint_seq_number()
        .unwrap()
        .unwrap();
    let service = RpcService::new(Arc::new(store));

    let list = |ordering: Ordering,
                after: Option<u64>,
                before: Option<u64>,
                end_checkpoint,
                limit: Option<u32>| {
        let cursor = |version| {
            PackageVersionCursor::Lineage {
                original_package_id: framework,
                version,
                minted_at: highest_indexed,
            }
            .encode()
        };
        let mut options = QueryOptions::default();
        options.ordering = Some(ordering as i32);
        options.after = after.map(cursor);
        options.before = before.map(cursor);
        options.limit = limit;
        let mut request = ListPackageVersionsRequest::default();
        request.package_id = Some(framework.to_string());
        request.options = Some(options);
        request.end_checkpoint = end_checkpoint;
        service.list_package_versions(tonic::Request::new(request))
    };
    let listed = |response: tonic::Response<ListPackageVersionsResponse>| {
        response
            .into_inner()
            .versions
            .iter()
            .map(|version| version.version.unwrap())
            .collect::<Vec<_>>()
    };
    let assert_below_fork = |error: tonic::Status, case: &str| {
        assert_eq!(
            (error.code(), error.message()),
            (
                tonic::Code::OutOfRange,
                "requested data below earliest available; lowest available checkpoint is 101"
            ),
            "{case}"
        );
        let metadata = RpcStatus::decode(error.details())
            .unwrap()
            .details
            .iter()
            .find(|detail| detail.type_url.ends_with("google.rpc.ErrorInfo"))
            .map(|detail| ErrorInfo::decode(detail.value.as_slice()).unwrap())
            .unwrap()
            .metadata;
        assert_eq!(
            metadata,
            [("lowestAvailableCheckpoint".to_owned(), "101".to_owned())]
                .into_iter()
                .collect(),
            "{case}"
        );
    };

    let past_tip = format!(
        "requested data above latest available; cursor is at checkpoint {0}, and only \
         checkpoints below {0} are available",
        highest_indexed + 1
    );
    // A descending `end_checkpoint` must be indexed, and the fork has indexed nothing above the
    // fork checkpoint, below the lowest `end_checkpoint` it accepts; so only ascending listings
    // set one here.
    for (ordering, end_checkpoint) in [
        (Ordering::Ascending, None),
        (Ordering::Descending, None),
        (Ordering::Ascending, Some(fork_checkpoint + 2)),
    ] {
        let case = |cursors: &str| format!("{ordering:?} {cursors} end {end_checkpoint:?}");

        for (after, before, expected) in [(Some(4), None, vec![]), (None, Some(4), vec![3])] {
            let result = list(ordering, after, before, end_checkpoint, None).await;
            let case = case(&format!("after {after:?} before {before:?}"));
            if ordering == Ordering::Descending {
                let error = result.unwrap_err();
                assert_eq!(
                    (error.code(), error.message()),
                    (tonic::Code::OutOfRange, past_tip.as_str()),
                    "{case}"
                );
            } else {
                assert_eq!(listed(result.unwrap()), expected, "{case}");
            }
        }

        // The listing starts at the loaded version.
        let response = list(ordering, None, None, end_checkpoint, Some(2))
            .await
            .unwrap();
        assert_eq!(listed(response), vec![3], "{}", case("no cursor"));

        // No lineage has a version below 1, so a `before` cursor at version 0 skips nothing,
        // and one below the loaded version leaves nothing to list.
        for before in [0, 1, 2, 3] {
            let response = list(ordering, None, Some(before), end_checkpoint, None)
                .await
                .unwrap();
            assert_eq!(
                listed(response),
                Vec::<u64>::new(),
                "{}",
                case(&format!("before {before}"))
            );
        }

        // The client holds version 2, just below the loaded version, so nothing is skipped;
        // past the loaded version, nothing is left.
        for (after, expected) in [(2, vec![3]), (3, vec![])] {
            let response = list(ordering, Some(after), None, end_checkpoint, None)
                .await
                .unwrap();
            assert_eq!(
                listed(response),
                expected,
                "{}",
                case(&format!("after {after}"))
            );
        }

        // The client holds version 1, and version 2 is gone: a listing in either order would
        // skip it.
        let error = list(ordering, Some(1), None, end_checkpoint, None)
            .await
            .unwrap_err();
        assert_below_fork(error, &case("after 1"));
    }
}

/// A version's key: its publish checkpoint, original id and version. Within one lineage, key order
/// is version order.
type PublishKey = (u64, ObjectID, u64);

/// A version one `ListPackageVersions` page listed: its key, and the cursor naming it.
struct ListedVersion {
    key: PublishKey,
    cursor: prost::bytes::Bytes,
}

/// The keys of `listed`, in listing order.
fn listed_keys(listed: &[ListedVersion]) -> Vec<PublishKey> {
    listed.iter().map(|version| version.key).collect()
}

/// The keys among `published`, given in key order, that a listing of the checkpoint range
/// `[start, end)` holds strictly after `after` and strictly before `before`, in `ordering`.
fn expected_keys(
    published: &[PublishKey],
    [start, end]: [Option<u64>; 2],
    ordering: sui_rpc_api::proto::sui::rpc::v2::Ordering,
    after: Option<PublishKey>,
    before: Option<PublishKey>,
) -> Vec<PublishKey> {
    let mut keys: Vec<_> = published
        .iter()
        .copied()
        .filter(|key| {
            let (checkpoint, ..) = *key;
            start.is_none_or(|start| checkpoint >= start)
                && end.is_none_or(|end| checkpoint < end)
                && after.is_none_or(|after| *key > after)
                && before.is_none_or(|before| *key < before)
        })
        .collect();
    if ordering == sui_rpc_api::proto::sui::rpc::v2::Ordering::Descending {
        keys.reverse();
    }
    keys
}

/// A fork whose own checkpoints publish packages, read through `RpcService` as the fork's RPC
/// server reads it. The first checkpoint after the fork publishes the package whose lineage the
/// tests list, the second publishes three packages together, and each of the next four upgrades
/// the first package once: its versions 1 to 5 are published in `checkpoints[0]` and
/// `checkpoints[2..]`.
struct ForkWithPublishes {
    service: sui_rpc_api::RpcService,
    /// The original id of the upgraded package.
    lineage: ObjectID,
    /// Every published version, in key order.
    published: Vec<PublishKey>,
    /// The checkpoints the fork created after the fork checkpoint, in order. The last one is the
    /// highest indexed checkpoint.
    checkpoints: Vec<CheckpointSequenceNumber>,
    _context: Arc<crate::context::Context>,
    _checkpoint_receiver:
        tokio::sync::broadcast::Receiver<Arc<sui_types::full_checkpoint_content::Checkpoint>>,
    _temp: tempfile::TempDir,
    _gql_server: MockServer,
}

/// Build a [`ForkWithPublishes`]: a fork at genesis whose publishes and upgrades execute in
/// Simulacrum, each batch sealed into its own checkpoint, which the embedded indexer indexes
/// before the next batch runs.
async fn fork_with_publishes() -> ForkWithPublishes {
    use prometheus::Registry;
    use sui_types::MOVE_STDLIB_PACKAGE_ID;
    use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
    use sui_types::move_package::MovePackage;
    use sui_types::move_package::UpgradePolicy;

    /// Signs and executes publishes and upgrades of one module from one genesis account.
    struct Publisher {
        sender: SuiAddress,
        key: sui_types::crypto::AccountKeyPair,
        gas: ObjectID,
        modules: Vec<Vec<u8>>,
        deps: Vec<ObjectID>,
    }

    impl Publisher {
        const GAS_BUDGET: u64 = 50_000_000_000;

        fn object_ref(
            sim: &Simulacrum<OsRng, ForkStore>,
            id: ObjectID,
        ) -> sui_types::base_types::ObjectRef {
            sim.store()
                .get_object(&id)
                .expect("object lookup failed")
                .expect("object should exist")
                .compute_object_reference()
        }

        /// Execute `data`, which must succeed, and return the ids of the objects it created with
        /// each of `owners`, one per owner.
        fn execute(
            &self,
            sim: &mut Simulacrum<OsRng, ForkStore>,
            data: TransactionData,
            owners: &[Owner],
        ) -> Vec<ObjectID> {
            let tx = Transaction::from_data_and_signer(data, vec![&self.key]);
            let (effects, error) = sim.execute_transaction(tx).unwrap();
            assert!(
                effects.status().is_ok(),
                "status={:?} error={error:?}",
                effects.status()
            );
            owners
                .iter()
                .map(|owner| {
                    effects
                        .created()
                        .into_iter()
                        .find_map(|((id, _, _), created_owner)| {
                            (created_owner == *owner).then_some(id)
                        })
                        .unwrap_or_else(|| panic!("no object created with owner {owner:?}"))
                })
                .collect()
        }

        /// Publish the module as a new package, returning the package's id and the id of its
        /// upgrade cap.
        fn publish(&self, sim: &mut Simulacrum<OsRng, ForkStore>) -> (ObjectID, ObjectID) {
            let data = TransactionData::new_module(
                self.sender,
                Self::object_ref(sim, self.gas),
                self.modules.clone(),
                self.deps.clone(),
                Self::GAS_BUDGET,
                sim.reference_gas_price(),
            );
            let created = self.execute(
                sim,
                data,
                &[Owner::Immutable, Owner::AddressOwner(self.sender)],
            );
            (created[0], created[1])
        }

        /// Upgrade the package whose latest version is stored at `package` with the upgrade cap
        /// `cap`, returning the id of the new version.
        fn upgrade(
            &self,
            sim: &mut Simulacrum<OsRng, ForkStore>,
            package: ObjectID,
            cap: ObjectID,
        ) -> ObjectID {
            let digest =
                MovePackage::compute_digest_for_modules_and_deps(&self.modules, &self.deps, true);
            let data = TransactionData::new_upgrade(
                self.sender,
                Self::object_ref(sim, self.gas),
                package,
                self.modules.clone(),
                self.deps.clone(),
                (Self::object_ref(sim, cap), Owner::AddressOwner(self.sender)),
                UpgradePolicy::COMPATIBLE,
                digest.to_vec(),
                Self::GAS_BUDGET,
                sim.reference_gas_price(),
            )
            .unwrap();
            self.execute(sim, data, &[Owner::Immutable])[0]
        }
    }

    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let mut rng = OsRng;
    let config = ConfigBuilder::new_with_temp_dir()
        .rng(&mut rng)
        .deterministic_committee_size(NonZeroUsize::MIN)
        .build();
    let genesis_checkpoint = config.genesis.checkpoint();
    let genesis_contents = config.genesis.checkpoint_contents().clone();
    let forked_at_checkpoint = genesis_checkpoint.data().sequence_number;
    let chain_identifier = (*genesis_checkpoint.digest()).into();
    let services = ServiceManager::open(
        temp.path(),
        "localnet".to_owned(),
        forked_at_checkpoint,
        chain_identifier,
    )
    .expect("service manager should open");
    let gql_server = crate::test_support::absent_objects_gql_server().await;
    // Package listings read the remote's object floor, which the fork clamps at the fork
    // checkpoint.
    mock_available_range(&gql_server, 0).await;
    let mut store = ForkStore::new_for_testing_with_remote(
        temp.path().to_path_buf(),
        gql_server.uri(),
        forked_at_checkpoint,
        services.local_store(),
    );
    store
        .save_checkpoint(&genesis_checkpoint, &genesis_contents)
        .expect("genesis checkpoint should be saved");
    store.update_objects(
        config
            .genesis
            .objects()
            .iter()
            .map(|object| (object.id(), object.clone()))
            .collect(),
        vec![],
    );
    let service = sui_rpc_api::RpcService::new(Arc::new(store.clone()));
    let sim = Simulacrum::new_from_custom_state(
        KeyStore::from_network_config(&config),
        genesis_checkpoint,
        config.genesis.sui_system_object(),
        chain_identifier,
        &config,
        store,
        rng,
    );

    let (sender, key) = {
        let (sender, key) = sim
            .keystore()
            .accounts()
            .next()
            .expect("at least one account");
        (*sender, key.copy())
    };
    let mut module = vec![];
    move_binary_format::file_format::empty_module()
        .serialize_with_version(
            sim.protocol_config().move_binary_format_version(),
            &mut module,
        )
        .unwrap();
    let publisher = Publisher {
        sender,
        key,
        gas: find_gas_coin(&config, sender).id(),
        modules: vec![module],
        deps: vec![MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID],
    };

    let (checkpoint_sender, checkpoint_receiver) = tokio::sync::broadcast::channel(16);
    let context = Arc::new(
        crate::context::Context::new(sim, services, checkpoint_sender, &Registry::new())
            .await
            .expect("service-backed context should initialize"),
    );

    let ((lineage, cap), first) = context
        .run_with_new_checkpoint(|sim| publisher.publish(sim))
        .await;
    let (together, second) = context
        .run_with_new_checkpoint(|sim| [(); 3].map(|()| publisher.publish(sim).0))
        .await;
    let mut checkpoints = vec![first.sequence_number, second.sequence_number];
    let mut published = vec![(first.sequence_number, lineage, 1)];
    published.extend(together.map(|id| (second.sequence_number, id, 1)));
    let mut latest = lineage;
    for version in 2..=5 {
        let (upgraded, created) = context
            .run_with_new_checkpoint(|sim| publisher.upgrade(sim, latest, cap))
            .await;
        latest = upgraded;
        checkpoints.push(created.sequence_number);
        published.push((created.sequence_number, lineage, version));
    }
    published.sort();

    ForkWithPublishes {
        service,
        lineage,
        published,
        checkpoints,
        _context: context,
        _checkpoint_receiver: checkpoint_receiver,
        _temp: temp,
        _gql_server: gql_server,
    }
}

impl ForkWithPublishes {
    /// The highest indexed checkpoint.
    fn tip(&self) -> u64 {
        *self.checkpoints.last().unwrap()
    }

    /// The keys of the versions of `lineage`, in version order.
    fn lineage_versions(&self) -> Vec<PublishKey> {
        self.published
            .iter()
            .copied()
            .filter(|(_, original_id, _)| *original_id == self.lineage)
            .collect()
    }

    /// A `ListPackageVersions` request for one page with `package_id` as given, over the
    /// checkpoint range `[start, end)`, `limit` at a time in `ordering`, after `after` and before
    /// `before`.
    fn request(
        &self,
        package_id: Option<ObjectID>,
        [start, end]: [Option<u64>; 2],
        ordering: sui_rpc_api::proto::sui::rpc::v2::Ordering,
        limit: u32,
        after: Option<&ListedVersion>,
        before: Option<&ListedVersion>,
    ) -> sui_rpc_api::proto::sui::rpc::v2::ListPackageVersionsRequest {
        let mut options = sui_rpc_api::proto::sui::rpc::v2::QueryOptions::default();
        options.ordering = Some(ordering as i32);
        options.limit = Some(limit);
        options.after = after.map(|version| version.cursor.clone());
        options.before = before.map(|version| version.cursor.clone());
        let mut request = sui_rpc_api::proto::sui::rpc::v2::ListPackageVersionsRequest::default();
        request.package_id = package_id.map(|id| id.to_string());
        request.start_checkpoint = start;
        request.end_checkpoint = end;
        request.options = Some(options);
        request
    }

    /// The page `request` lists, and how it ends.
    async fn page(
        &self,
        request: sui_rpc_api::proto::sui::rpc::v2::ListPackageVersionsRequest,
    ) -> (
        Vec<ListedVersion>,
        sui_rpc_api::proto::sui::rpc::v2::QueryEndReason,
    ) {
        use sui_rpc_api::proto::sui::rpc::v2::move_package_service_server::MovePackageService;

        let response = self
            .service
            .list_package_versions(tonic::Request::new(request.clone()))
            .await
            .unwrap_or_else(|error| panic!("{request:?}: {error:?}"))
            .into_inner();
        let listed = response
            .versions
            .into_iter()
            .map(|version| ListedVersion {
                key: (
                    version.checkpoint.unwrap(),
                    version.original_id.unwrap().parse().unwrap(),
                    version.version.unwrap(),
                ),
                cursor: version.cursor.unwrap(),
            })
            .collect();
        (listed, response.end.unwrap().reason())
    }

    /// Page through the listing `request` starts to its end: each next page continues after the
    /// last version of the one before it (before it, descending), keeping the request's range and
    /// its other cursor. Every full page ends with `ITEM_LIMIT`. Returns every listed version and
    /// how the last page ends.
    async fn walk(
        &self,
        mut request: sui_rpc_api::proto::sui::rpc::v2::ListPackageVersionsRequest,
    ) -> (
        Vec<ListedVersion>,
        sui_rpc_api::proto::sui::rpc::v2::QueryEndReason,
    ) {
        use sui_rpc_api::proto::sui::rpc::v2::Ordering;
        use sui_rpc_api::proto::sui::rpc::v2::QueryEndReason;

        let options = request.options.as_ref().unwrap();
        let limit = options.limit.unwrap() as usize;
        let descending = options.ordering == Some(Ordering::Descending as i32);
        let mut listed = vec![];
        loop {
            let (page, end) = self.page(request.clone()).await;
            if page.len() < limit {
                listed.extend(page);
                return (listed, end);
            }
            assert_eq!(end, QueryEndReason::ItemLimit, "{request:?}");
            let next = Some(page.last().unwrap().cursor.clone());
            let options = request.options.as_mut().unwrap();
            if descending {
                options.before = next;
            } else {
                options.after = next;
            }
            listed.extend(page);
            assert!(
                listed.len() <= self.published.len(),
                "{request:?} lists more versions than were published"
            );
        }
    }

    /// How an ascending listing of `range` that reads to its end stops: at the ledger tip when the
    /// range reaches past the highest indexed checkpoint, else at the range's end.
    fn ascending_end(
        &self,
        [_, end]: [Option<u64>; 2],
    ) -> sui_rpc_api::proto::sui::rpc::v2::QueryEndReason {
        use sui_rpc_api::proto::sui::rpc::v2::QueryEndReason;

        if end.is_none_or(|end| end > self.tip() + 1) {
            QueryEndReason::LedgerTip
        } else {
            QueryEndReason::CheckpointBound
        }
    }

    /// List `range` ascending, one page from after each version, `limit` 1 and 2, and reverse at
    /// the page's first version: a descending listing before it holds exactly the versions below
    /// the page, newest first, and with the page and the versions after it makes the whole
    /// listing, each version once. `published` is every version the listing can hold, in key
    /// order.
    async fn check_reversal_of_ascending_pages(
        &self,
        package_id: Option<ObjectID>,
        published: &[PublishKey],
        range: [Option<u64>; 2],
    ) {
        use sui_rpc_api::proto::sui::rpc::v2::Ordering;
        use sui_rpc_api::proto::sui::rpc::v2::QueryEndReason;

        let whole = expected_keys(published, range, Ordering::Ascending, None, None);
        for limit in [1, 2] {
            let request =
                |ordering, after: Option<&ListedVersion>, before: Option<&ListedVersion>| {
                    self.request(package_id, range, ordering, limit, after, before)
                };
            let (all, end) = self.walk(request(Ordering::Ascending, None, None)).await;
            assert_eq!(listed_keys(&all), whole, "{range:?} limit {limit}");
            assert_eq!(end, self.ascending_end(range), "{range:?} limit {limit}");

            for held in &all {
                let case = format!("{range:?} limit {limit} after {:?}", held.key);
                let (page, page_end) = self
                    .page(request(Ordering::Ascending, Some(held), None))
                    .await;
                let expected_page: Vec<_> =
                    expected_keys(published, range, Ordering::Ascending, Some(held.key), None)
                        .into_iter()
                        .take(limit as usize)
                        .collect();
                assert_eq!(listed_keys(&page), expected_page, "{case}");
                let expected_end = if page.len() == limit as usize {
                    QueryEndReason::ItemLimit
                } else {
                    self.ascending_end(range)
                };
                assert_eq!(page_end, expected_end, "{case}");
                let (Some(first), Some(last)) = (page.first(), page.last()) else {
                    continue;
                };

                let (below, below_end) = self
                    .walk(request(Ordering::Descending, None, Some(first)))
                    .await;
                assert_eq!(
                    listed_keys(&below),
                    expected_keys(
                        published,
                        range,
                        Ordering::Descending,
                        None,
                        Some(first.key)
                    ),
                    "{case}"
                );
                assert_eq!(below_end, QueryEndReason::CheckpointBound, "{case}");

                let (above, _) = self
                    .walk(request(Ordering::Ascending, Some(last), None))
                    .await;
                let mut listing: Vec<_> = listed_keys(&below).into_iter().rev().collect();
                listing.extend(listed_keys(&page));
                listing.extend(listed_keys(&above));
                assert_eq!(listing, whole, "{case}");
            }
        }
    }

    /// List `range` descending, one page from before each version, `limit` 1 and 2, and reverse
    /// at the page's first version, its highest: an ascending listing after it holds exactly the
    /// versions above the page, oldest first, and with the page and the versions below it makes
    /// the whole listing, each version once. An ascending listing after the page's last version,
    /// its lowest, lists the rest of the page again: a cursor names a position, not a direction.
    /// `published` is as for [`Self::check_reversal_of_ascending_pages`].
    async fn check_reversal_of_descending_pages(
        &self,
        package_id: Option<ObjectID>,
        published: &[PublishKey],
        range: [Option<u64>; 2],
    ) {
        use sui_rpc_api::proto::sui::rpc::v2::Ordering;
        use sui_rpc_api::proto::sui::rpc::v2::QueryEndReason;

        let whole = expected_keys(published, range, Ordering::Ascending, None, None);
        for limit in [1, 2] {
            let request =
                |ordering, after: Option<&ListedVersion>, before: Option<&ListedVersion>| {
                    self.request(package_id, range, ordering, limit, after, before)
                };
            let (all, end) = self.walk(request(Ordering::Descending, None, None)).await;
            assert_eq!(
                listed_keys(&all),
                expected_keys(published, range, Ordering::Descending, None, None),
                "{range:?} limit {limit}"
            );
            assert_eq!(
                end,
                QueryEndReason::CheckpointBound,
                "{range:?} limit {limit}"
            );

            for held in &all {
                let case = format!("{range:?} limit {limit} before {:?}", held.key);
                let (page, page_end) = self
                    .page(request(Ordering::Descending, None, Some(held)))
                    .await;
                let expected_page: Vec<_> =
                    expected_keys(published, range, Ordering::Descending, None, Some(held.key))
                        .into_iter()
                        .take(limit as usize)
                        .collect();
                assert_eq!(listed_keys(&page), expected_page, "{case}");
                let expected_end = if page.len() == limit as usize {
                    QueryEndReason::ItemLimit
                } else {
                    QueryEndReason::CheckpointBound
                };
                assert_eq!(page_end, expected_end, "{case}");
                let (Some(first), Some(last)) = (page.first(), page.last()) else {
                    continue;
                };

                let (above, above_end) = self
                    .walk(request(Ordering::Ascending, Some(first), None))
                    .await;
                assert_eq!(
                    listed_keys(&above),
                    expected_keys(published, range, Ordering::Ascending, Some(first.key), None),
                    "{case}"
                );
                assert_eq!(above_end, self.ascending_end(range), "{case}");

                let (from_last, _) = self
                    .walk(request(Ordering::Ascending, Some(last), None))
                    .await;
                let mut rest: Vec<_> = listed_keys(&page[..page.len() - 1])
                    .into_iter()
                    .rev()
                    .collect();
                rest.extend(listed_keys(&above));
                assert_eq!(listed_keys(&from_last), rest, "{case}");

                let (below, _) = self
                    .walk(request(Ordering::Descending, None, Some(last)))
                    .await;
                let mut listing: Vec<_> = listed_keys(&below).into_iter().rev().collect();
                listing.extend(listed_keys(&page).into_iter().rev());
                listing.extend(listed_keys(&above));
                assert_eq!(listing, whole, "{case}");
            }
        }
    }

    /// Take cursors from pages of `mint_range`, `limit` 1 and 2, and reverse at each page's first
    /// version under `reverse_range`: descending before the first version of each ascending
    /// page, and ascending after the first version of each descending page. The reversed listing
    /// holds the versions of `reverse_range` beyond that version, whether the cursor lies inside
    /// `reverse_range` or outside it. `published` is as for
    /// [`Self::check_reversal_of_ascending_pages`].
    async fn check_reversal_under_another_range(
        &self,
        package_id: Option<ObjectID>,
        published: &[PublishKey],
        mint_range: [Option<u64>; 2],
        reverse_range: [Option<u64>; 2],
    ) {
        use sui_rpc_api::proto::sui::rpc::v2::Ordering;

        for limit in [1, 2] {
            let case = format!("minted under {mint_range:?}, reversed under {reverse_range:?}");
            let case = format!("{case}, limit {limit}");
            let mint = |ordering, after: Option<&ListedVersion>, before: Option<&ListedVersion>| {
                self.request(package_id, mint_range, ordering, limit, after, before)
            };
            let reverse =
                |ordering, after: Option<&ListedVersion>, before: Option<&ListedVersion>| {
                    self.request(package_id, reverse_range, ordering, limit, after, before)
                };
            let (minted, _) = self.walk(mint(Ordering::Ascending, None, None)).await;
            assert_eq!(
                listed_keys(&minted),
                expected_keys(published, mint_range, Ordering::Ascending, None, None),
                "{case}"
            );

            for held in &minted {
                let (page, _) = self.page(mint(Ordering::Ascending, Some(held), None)).await;
                if let Some(first) = page.first() {
                    let (reversed, _) = self
                        .walk(reverse(Ordering::Descending, None, Some(first)))
                        .await;
                    assert_eq!(
                        listed_keys(&reversed),
                        expected_keys(
                            published,
                            reverse_range,
                            Ordering::Descending,
                            None,
                            Some(first.key)
                        ),
                        "{case}: descending before {:?}",
                        first.key
                    );
                }

                let (page, _) = self
                    .page(mint(Ordering::Descending, None, Some(held)))
                    .await;
                if let Some(first) = page.first() {
                    let (reversed, _) = self
                        .walk(reverse(Ordering::Ascending, Some(first), None))
                        .await;
                    assert_eq!(
                        listed_keys(&reversed),
                        expected_keys(
                            published,
                            reverse_range,
                            Ordering::Ascending,
                            Some(first.key),
                            None
                        ),
                        "{case}: ascending after {:?}",
                        first.key
                    );
                }
            }
        }
    }
}

/// The lineage of a package published and upgraded in the fork, reversed after an ascending page
/// that starts after a cursor: a descending listing before the page's first version lists exactly
/// the versions below the page, newest first, with or without a checkpoint range.
#[tokio::test]
async fn test_list_package_versions_reversed_from_an_ascending_page_lists_the_versions_below_it() {
    use sui_rpc_api::proto::sui::rpc::v2::Ordering;

    let fork = fork_with_publishes().await;
    let lineage = fork.lineage_versions();
    let versions = |listed: &[ListedVersion]| {
        listed
            .iter()
            .map(|version| version.key.2)
            .collect::<Vec<_>>()
    };

    // The page after version 2 holds versions 3 and 4, and the listing before version 3 holds
    // versions 2 and 1.
    let request = |ordering, after: Option<&ListedVersion>, before: Option<&ListedVersion>| {
        fork.request(Some(fork.lineage), [None, None], ordering, 2, after, before)
    };
    let (first_page, _) = fork.page(request(Ordering::Ascending, None, None)).await;
    assert_eq!(versions(&first_page), [1, 2]);
    let (page, _) = fork
        .page(request(Ordering::Ascending, Some(&first_page[1]), None))
        .await;
    assert_eq!(versions(&page), [3, 4]);
    let (below, _) = fork
        .walk(request(Ordering::Descending, None, Some(&page[0])))
        .await;
    assert_eq!(versions(&below), [2, 1]);

    let c = &fork.checkpoints;
    // Every version; versions 2 to 5; versions 1 to 4; and versions 2 to 5 again, from a
    // checkpoint that publishes no version of the lineage to the checkpoint after the tip.
    for range in [
        [None, None],
        [Some(c[2]), None],
        [None, Some(c[5])],
        [Some(c[1]), Some(c[5] + 1)],
    ] {
        fork.check_reversal_of_ascending_pages(Some(fork.lineage), &lineage, range)
            .await;
    }
}

/// The lineage of a package published and upgraded in the fork, reversed after a descending page
/// that starts before a cursor: an ascending listing after the page's first version lists exactly
/// the versions above the page, oldest first, and one after the page's last version lists the
/// rest of the page again, with or without a checkpoint range.
#[tokio::test]
async fn test_list_package_versions_reversed_from_a_descending_page_lists_the_versions_above_it() {
    use sui_rpc_api::proto::sui::rpc::v2::Ordering;

    let fork = fork_with_publishes().await;
    let lineage = fork.lineage_versions();
    let versions = |listed: &[ListedVersion]| {
        listed
            .iter()
            .map(|version| version.key.2)
            .collect::<Vec<_>>()
    };

    // The page before version 5 holds versions 4 and 3. The listing after version 4 holds
    // version 5, and the one after version 3 holds versions 4 and 5.
    let request = |ordering, after: Option<&ListedVersion>, before: Option<&ListedVersion>| {
        fork.request(Some(fork.lineage), [None, None], ordering, 2, after, before)
    };
    let (first_page, _) = fork.page(request(Ordering::Descending, None, None)).await;
    assert_eq!(versions(&first_page), [5, 4]);
    let (page, _) = fork
        .page(request(Ordering::Descending, None, Some(&first_page[0])))
        .await;
    assert_eq!(versions(&page), [4, 3]);
    let (above, _) = fork
        .walk(request(Ordering::Ascending, Some(&page[0]), None))
        .await;
    assert_eq!(versions(&above), [5]);
    let (from_last, _) = fork
        .walk(request(Ordering::Ascending, Some(&page[1]), None))
        .await;
    assert_eq!(versions(&from_last), [4, 5]);

    let c = &fork.checkpoints;
    for range in [
        [None, None],
        [Some(c[2]), None],
        [None, Some(c[5])],
        [Some(c[1]), Some(c[5] + 1)],
    ] {
        fork.check_reversal_of_descending_pages(Some(fork.lineage), &lineage, range)
            .await;
    }
}

/// The lineage of a package published and upgraded in the fork, reversed from a cursor taken
/// under a different checkpoint range than the reversed request's: the reversed listing holds the
/// versions of the new range beyond the cursor, whether the cursor lies inside that range or
/// outside it.
#[tokio::test]
async fn test_list_package_versions_reversed_under_another_checkpoint_range_lists_the_intersection()
{
    let fork = fork_with_publishes().await;
    let lineage = fork.lineage_versions();
    let c = &fork.checkpoints;
    // Every version; versions 1 and 2; versions 1 to 3; versions 2 to 5; versions 3 and 4;
    // versions 2 to 5 again, to the checkpoint after the tip; and an empty range.
    let ranges = [
        [None, None],
        [None, Some(c[3])],
        [None, Some(c[4])],
        [Some(c[2]), None],
        [Some(c[3]), Some(c[5])],
        [Some(c[1]), Some(c[5] + 1)],
        [Some(c[4]), Some(c[4])],
    ];
    for mint_range in ranges {
        for reverse_range in ranges {
            fork.check_reversal_under_another_range(
                Some(fork.lineage),
                &lineage,
                mint_range,
                reverse_range,
            )
            .await;
        }
    }
}

/// The fork reads a listing's package as of its highest indexed checkpoint, as a fullnode does. A
/// first page for a package it does not know, such as one a replica ahead of it published, is
/// `NotFound`. With a cursor, the cursor's `minted_at` decides: minted above the fork's tip, by
/// that replica, it is a cursor past the tip (ascending, an empty page ending with `LEDGER_TIP`;
/// descending, `ABOVE_LATEST_AVAILABLE` for the cursor at its minting height); minted at or below
/// the tip, it is stale or forged, and the request is `NotFound`. A package the fork knows lists
/// the same whatever its cursors' `minted_at`.
#[tokio::test]
async fn test_list_package_versions_of_an_unknown_package_is_decided_by_the_cursors_minting_height()
{
    use sui_rpc_api::grpc::v2::move_package_service::PackageVersionCursor;
    use sui_rpc_api::proto::sui::rpc::v2::Ordering;
    use sui_rpc_api::proto::sui::rpc::v2::QueryEndReason;
    use sui_rpc_api::proto::sui::rpc::v2::move_package_service_server::MovePackageService;

    let fork = fork_with_publishes().await;
    let tip = fork.tip();
    let ahead = tip + 5;
    let unknown = ObjectID::random();
    let cursor = |original_package_id, version, minted_at| ListedVersion {
        key: (0, original_package_id, version),
        cursor: PackageVersionCursor::Lineage {
            original_package_id,
            version,
            minted_at,
        }
        .encode(),
    };
    let service = &fork.service;
    let error = move |request: sui_rpc_api::proto::sui::rpc::v2::ListPackageVersionsRequest| async move {
        service
            .list_package_versions(tonic::Request::new(request))
            .await
            .unwrap_err()
    };

    for ordering in [Ordering::Ascending, Ordering::Descending] {
        let request = fork.request(Some(unknown), [None, None], ordering, 10, None, None);
        assert_eq!(
            error(request).await.code(),
            tonic::Code::NotFound,
            "{ordering:?}"
        );
    }

    // The lagging replica, in either order.
    let minted_ahead = cursor(unknown, 1, ahead);
    for (after, before) in [(Some(&minted_ahead), None), (None, Some(&minted_ahead))] {
        let (page, end) = fork
            .page(fork.request(
                Some(unknown),
                [None, None],
                Ordering::Ascending,
                10,
                after,
                before,
            ))
            .await;
        assert!(page.is_empty());
        assert_eq!(end, QueryEndReason::LedgerTip);

        let status = error(fork.request(
            Some(unknown),
            [None, None],
            Ordering::Descending,
            10,
            after,
            before,
        ))
        .await;
        assert_eq!(
            (status.code(), status.message()),
            (
                tonic::Code::OutOfRange,
                format!(
                    "requested data above latest available; cursor is at checkpoint {ahead}, \
                     and only checkpoints below {} are available",
                    tip + 1
                )
                .as_str()
            )
        );
    }

    // A stale or forged cursor.
    for minted_at in [0, tip] {
        let stale = cursor(unknown, 1, minted_at);
        for ordering in [Ordering::Ascending, Ordering::Descending] {
            for (after, before) in [(Some(&stale), None), (None, Some(&stale))] {
                let request =
                    fork.request(Some(unknown), [None, None], ordering, 10, after, before);
                assert_eq!(
                    error(request).await.code(),
                    tonic::Code::NotFound,
                    "{ordering:?} minted at {minted_at}"
                );
            }
        }
    }

    // A package published within the fork's height.
    let lineage = fork.lineage_versions();
    for minted_at in [0, tip, ahead] {
        for (ordering, after, before, expected, expected_end) in [
            (
                Ordering::Ascending,
                Some(cursor(fork.lineage, 2, minted_at)),
                None,
                lineage[2..].to_vec(),
                QueryEndReason::LedgerTip,
            ),
            (
                Ordering::Descending,
                None,
                Some(cursor(fork.lineage, 4, minted_at)),
                lineage[..3].iter().rev().copied().collect(),
                QueryEndReason::CheckpointBound,
            ),
        ] {
            let (page, end) = fork
                .page(fork.request(
                    Some(fork.lineage),
                    [None, None],
                    ordering,
                    10,
                    after.as_ref(),
                    before.as_ref(),
                ))
                .await;
            assert_eq!(
                listed_keys(&page),
                expected,
                "{ordering:?} minted at {minted_at}"
            );
            assert_eq!(end, expected_end, "{ordering:?} minted at {minted_at}");
        }
    }
}

/// The listing of every package published in the fork, reversed after an ascending page that
/// starts after a cursor: a descending listing before the page's first version lists exactly the
/// versions below the page, newest first, with or without a checkpoint range. One reversal crosses
/// a checkpoint boundary, and one stays within the checkpoint that publishes three packages.
#[tokio::test]
async fn test_list_all_package_versions_reversed_from_an_ascending_page_lists_the_versions_below_it()
 {
    use sui_rpc_api::proto::sui::rpc::v2::Ordering;

    let fork = fork_with_publishes().await;
    let c = &fork.checkpoints;
    let p = &fork.published;
    assert_eq!([p[1].0, p[2].0, p[3].0], [c[1]; 3]);
    let request = |ordering, after: Option<&ListedVersion>, before: Option<&ListedVersion>| {
        fork.request(None, [None, None], ordering, 1, after, before)
    };
    let (all, _) = fork.walk(request(Ordering::Ascending, None, None)).await;
    assert_eq!(listed_keys(&all), *p);

    // The page after the first checkpoint's only publish starts the second checkpoint, and the
    // listing before it is that publish.
    let (page, _) = fork
        .page(request(Ordering::Ascending, Some(&all[0]), None))
        .await;
    assert_eq!(listed_keys(&page), [p[1]]);
    let (below, _) = fork
        .walk(request(Ordering::Descending, None, Some(&page[0])))
        .await;
    assert_eq!(listed_keys(&below), [p[0]]);

    // The page after the second checkpoint's first publish is its middle one, and the listing
    // before it starts with the second checkpoint's first publish.
    let (page, _) = fork
        .page(request(Ordering::Ascending, Some(&all[1]), None))
        .await;
    assert_eq!(listed_keys(&page), [p[2]]);
    let (below, _) = fork
        .walk(request(Ordering::Descending, None, Some(&page[0])))
        .await;
    assert_eq!(listed_keys(&below), [p[1], p[0]]);

    // Every publish; from the second checkpoint; through the second checkpoint; and the second
    // checkpoint's publishes with the first two upgrades.
    for range in [
        [None, None],
        [Some(c[1]), None],
        [None, Some(c[2])],
        [Some(c[1]), Some(c[4])],
    ] {
        fork.check_reversal_of_ascending_pages(None, &fork.published, range)
            .await;
    }
}

/// The listing of every package published in the fork, reversed after a descending page that
/// starts before a cursor: an ascending listing after the page's first version lists exactly the
/// versions above the page, oldest first, and one after the page's last version lists the rest of
/// the page again, with or without a checkpoint range. One reversal crosses a checkpoint boundary,
/// and one stays within the checkpoint that publishes three packages.
#[tokio::test]
async fn test_list_all_package_versions_reversed_from_a_descending_page_lists_the_versions_above_it()
 {
    use sui_rpc_api::proto::sui::rpc::v2::Ordering;

    let fork = fork_with_publishes().await;
    let c = &fork.checkpoints;
    let p = &fork.published;
    assert_eq!([p[1].0, p[2].0, p[3].0], [c[1]; 3]);
    let request = |ordering, after: Option<&ListedVersion>, before: Option<&ListedVersion>| {
        fork.request(None, [None, None], ordering, 1, after, before)
    };
    let (all, _) = fork.walk(request(Ordering::Ascending, None, None)).await;
    assert_eq!(listed_keys(&all), *p);

    // The page before the third checkpoint's publish is the second checkpoint's last one, and
    // the listing after it starts at the third checkpoint.
    let (page, _) = fork
        .page(request(Ordering::Descending, None, Some(&all[4])))
        .await;
    assert_eq!(listed_keys(&page), [p[3]]);
    let (above, _) = fork
        .walk(request(Ordering::Ascending, Some(&page[0]), None))
        .await;
    assert_eq!(listed_keys(&above), p[4..]);

    // The page before the second checkpoint's last publish is its middle one, and the listing
    // after it starts with the second checkpoint's last publish.
    let (page, _) = fork
        .page(request(Ordering::Descending, None, Some(&all[3])))
        .await;
    assert_eq!(listed_keys(&page), [p[2]]);
    let (above, _) = fork
        .walk(request(Ordering::Ascending, Some(&page[0]), None))
        .await;
    assert_eq!(listed_keys(&above), p[3..]);

    for range in [
        [None, None],
        [Some(c[1]), None],
        [None, Some(c[2])],
        [Some(c[1]), Some(c[4])],
    ] {
        fork.check_reversal_of_descending_pages(None, &fork.published, range)
            .await;
    }
}

/// The listing of every package published in the fork, reversed from a cursor taken under a
/// different checkpoint range than the reversed request's: the reversed listing holds the
/// versions of the new range beyond the cursor, whether the cursor lies inside that range or
/// outside it.
#[tokio::test]
async fn test_list_all_package_versions_reversed_under_another_checkpoint_range_lists_the_intersection()
 {
    let fork = fork_with_publishes().await;
    let c = &fork.checkpoints;
    // Every publish; from the second checkpoint; through the second checkpoint; the second
    // checkpoint alone; the second checkpoint with the first two upgrades; the upgrades, to the
    // checkpoint after the tip; and an empty range.
    let ranges = [
        [None, None],
        [Some(c[1]), None],
        [None, Some(c[2])],
        [Some(c[1]), Some(c[2])],
        [Some(c[1]), Some(c[4])],
        [Some(c[2]), Some(c[5] + 1)],
        [Some(c[3]), Some(c[3])],
    ];
    for mint_range in ranges {
        for reverse_range in ranges {
            fork.check_reversal_under_another_range(
                None,
                &fork.published,
                mint_range,
                reverse_range,
            )
            .await;
        }
    }
}

/// The seed load records every system package live at the fork checkpoint, so
/// `ListSystemPackages` there lists each at its version, though nothing else fetched them. The fork
/// has indexed nothing past the fork checkpoint, so a request without `at_checkpoint` gets the
/// same answer.
#[tokio::test]
async fn test_list_system_packages_at_the_fork_checkpoint() {
    use sui_rpc_api::RpcService;
    use sui_rpc_api::proto::sui::rpc::v2::ListSystemPackagesRequest;
    use sui_rpc_api::proto::sui::rpc::v2::PackageVersion;
    use sui_rpc_api::proto::sui::rpc::v2::move_package_service_server::MovePackageService;

    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let server = MockServer::start().await;
    let fork_checkpoint = 100;
    mock_available_range(&server, 10).await;
    let packages = system_package_objects(&[
        (ObjectID::from_single_byte(1), 1),
        (ObjectID::from_single_byte(2), 3),
        (ObjectID::from_single_byte(3), 2),
    ]);
    mock_system_packages(&server, fork_checkpoint, &packages).await;
    let (store, _services) = data_store_with_remote(temp.path(), server.uri(), fork_checkpoint);
    let manifest = SeedManifest {
        network: "custom".to_owned(),
        checkpoint: fork_checkpoint,
        addresses: vec![],
        entries: vec![],
    };
    load_seed_objects(&store, &manifest).expect("seed load should fetch the system packages");

    let service = RpcService::new(Arc::new(store));
    let listed = service
        .list_system_packages(tonic::Request::new(ListSystemPackagesRequest::new(
            fork_checkpoint,
        )))
        .await
        .unwrap()
        .into_inner()
        .system_packages;
    // The fork's indexes hold no history before the fork checkpoint, so no publish checkpoint is
    // known.
    let expected: Vec<_> = packages
        .iter()
        .map(|package| {
            PackageVersion::new(&package.id().into(), package.version().value())
                .with_original_id(package.id().to_canonical_string(true))
        })
        .collect();
    assert_eq!(listed, expected);

    let latest = service
        .list_system_packages(tonic::Request::new(ListSystemPackagesRequest::default()))
        .await
        .unwrap()
        .into_inner()
        .system_packages;
    assert_eq!(latest, expected);
}

/// `0x1`, `0x2` and `0x3` exist at every checkpoint, so a remote answering with none for one of
/// them fails the seed load, which stays incomplete, rather than seeding a fork that lacks it.
/// Other system packages may be absent at the fork checkpoint.
#[tokio::test]
async fn test_seed_load_fails_without_a_core_system_package() {
    let fork_checkpoint = 100;
    let manifest = SeedManifest {
        network: "custom".to_owned(),
        checkpoint: fork_checkpoint,
        addresses: vec![],
        entries: vec![],
    };
    for missing in 1..=3u8 {
        let temp = tempfile::tempdir().expect("failed to create tempdir");
        let server = MockServer::start().await;
        let present: Vec<_> = (1..=3u8)
            .filter(|id| *id != missing)
            .map(|id| (ObjectID::from_single_byte(id), 1))
            .collect();
        mock_system_packages(&server, fork_checkpoint, &system_package_objects(&present)).await;
        let (store, _services) = data_store_with_remote(temp.path(), server.uri(), fork_checkpoint);

        let error = load_seed_objects(&store, &manifest).unwrap_err();
        let missing_id = ObjectID::from_single_byte(missing);
        assert!(
            format!("{error:#}").contains(&format!("no system package {missing_id}")),
            "{error:#}"
        );
        assert!(!store.local_store().seed_load_complete().unwrap());
        assert!(!store.local_store().system_packages_loaded().unwrap());
    }
}

/// A fork directory whose seed load committed without the system packages loads them on its next
/// start, so `ListSystemPackages` at the fork checkpoint lists every one rather than only those the
/// fork fetched on demand. Before that start the fork fetched `0x2` on demand and then wrote newer
/// `0x2` and `0x3` locally, `0x3` without ever fetching it: both local versions stay current, and
/// both packages are still recorded at their fork-checkpoint versions. Later starts do not fetch
/// the packages again.
#[tokio::test]
async fn test_resumed_fork_loads_the_system_packages_its_seed_lacked() {
    use sui_rpc_api::RpcService;
    use sui_rpc_api::proto::sui::rpc::v2::ListSystemPackagesRequest;
    use sui_rpc_api::proto::sui::rpc::v2::PackageVersion;
    use sui_rpc_api::proto::sui::rpc::v2::move_package_service_server::MovePackageService;

    let temp = tempfile::tempdir().expect("failed to create tempdir");
    let server = MockServer::start().await;
    let fork_checkpoint = 100;
    mock_available_range(&server, 10).await;
    let packages = system_package_objects(&[
        (ObjectID::from_single_byte(1), 1),
        (ObjectID::from_single_byte(2), 3),
        (ObjectID::from_single_byte(3), 2),
    ]);
    mock_system_packages(&server, fork_checkpoint, &packages).await;
    let manifest = SeedManifest {
        network: "custom".to_owned(),
        checkpoint: fork_checkpoint,
        addresses: vec![],
        entries: vec![],
    };

    {
        let (store, services) = data_store_with_remote(temp.path(), server.uri(), fork_checkpoint);
        store.local_store().restore_seed_objects(&[]).unwrap();
        store
            .local_store()
            .save_live_object_if_current(&packages[1])
            .unwrap();
        let local_versions = [
            (ObjectID::from_single_byte(2), 4),
            (ObjectID::from_single_byte(3), 5),
        ];
        let upgraded: BTreeMap<_, _> = system_package_objects(&local_versions)
            .into_iter()
            .map(|package| (package.id(), package))
            .collect();
        store
            .local_store()
            .commit_local_object_diff_for_testing(&upgraded, &[])
            .unwrap();
        assert!(!store.local_store().system_packages_loaded().unwrap());

        load_seed_objects(&store, &manifest).expect("a resumed load should fetch the packages");
        assert!(store.local_store().system_packages_loaded().unwrap());
        for (id, fork_version, local_version) in [
            (ObjectID::from_single_byte(2), 3, 4),
            (ObjectID::from_single_byte(3), 2, 5),
        ] {
            assert_eq!(
                store
                    .local_store()
                    .get_latest_object_status(id)
                    .unwrap()
                    .map(|(version, _)| version),
                Some(SequenceNumber::from_u64(local_version)),
                "{id}",
            );
            assert_eq!(
                RpcIndexes::get_object_version_at_checkpoint(
                    store.local_store().reader(),
                    id,
                    fork_checkpoint,
                )
                .unwrap(),
                Some(SequenceNumber::from_u64(fork_version)),
                "{id}",
            );
        }

        let listed = RpcService::new(Arc::new(store.clone()))
            .list_system_packages(tonic::Request::new(ListSystemPackagesRequest::new(
                fork_checkpoint,
            )))
            .await
            .unwrap()
            .into_inner()
            .system_packages;
        let expected: Vec<_> = packages
            .iter()
            .map(|package| {
                PackageVersion::new(&package.id().into(), package.version().value())
                    .with_original_id(package.id().to_canonical_string(true))
            })
            .collect();
        assert_eq!(listed, expected);
        drop(services);
    }

    let (store, _services) = data_store_with_remote(
        temp.path(),
        "http://localhost:1".to_owned(),
        fork_checkpoint,
    );
    load_seed_objects(&store, &manifest).expect("loaded system packages must not be refetched");
}

#[tokio::test]
async fn test_advance_clock_executes_and_persists() {
    let (mut sim, _config, _temp, _gql_server) = test_simulacrum().await;
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

#[tokio::test]
async fn test_transfer_sui_executes_and_persists() {
    let (mut sim, config, _temp, _gql_server) = test_simulacrum().await;

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
fn test_owned_objects_reads_seeded_index() {
    let (_temp, store) = data_store();
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = make_gas_object(object_id, 1, Owner::AddressOwner(owner));

    // The seed load writes the owner index; `owned_objects` joins those rows
    // against current canonical state. Retracting a row when the object later
    // moves is the embedded indexer's job — it reads each executed checkpoint
    // as a diff and deletes at the prior owner key — so nothing here has to
    // reconcile.
    store.local_store().restore_seed_objects(&[object]).unwrap();

    let owner_objects: Vec<_> = SimulatorStore::owned_objects(&store, owner).collect();
    assert_eq!(owner_objects.len(), 1);
    assert_eq!(owner_objects[0].id(), object_id);
}

#[test]
fn test_owned_objects_tracks_consensus_address_owner_writes() {
    let (_temp, store) = data_store();
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

    // The seed load collapses ConsensusAddressOwner into the address-owner
    // index kind, so a consensus-owned object is reachable by its address.
    store.local_store().restore_seed_objects(&[object]).unwrap();

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
}

#[test]
fn test_read_child_object_uses_highest_local_version_within_bound() {
    let (_temp, store) = data_store();
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

/// Bounded child reads must see versions the in-flight checkpoint staged but has not yet sealed, or
/// execution of a later transaction in the same checkpoint would resolve dynamic fields against
/// pre-checkpoint state.
#[test]
fn test_read_child_object_sees_staged_versions_within_bound() {
    let (_temp, mut store) = data_store();
    let parent = ObjectID::random();
    let child_id = ObjectID::random();
    let child_v5 = make_gas_object(child_id, 5, Owner::ObjectOwner(parent.into()));
    let child_v7 = make_gas_object(child_id, 7, Owner::ObjectOwner(parent.into()));

    store.update_objects(BTreeMap::from([(child_id, child_v5.clone())]), vec![]);
    store.update_objects(BTreeMap::from([(child_id, child_v7.clone())]), vec![]);

    // Both bounds resolve from the overlay alone — the dummy remote would
    // surface as an error if either read fell through.
    assert_eq!(
        store
            .read_child_object_fallible(&parent, &child_id, SequenceNumber::from_u64(6))
            .expect("bounded read between staged versions should not error"),
        Some(child_v5),
    );
    assert_eq!(
        store
            .read_child_object_fallible(&parent, &child_id, SequenceNumber::from_u64(7))
            .expect("bounded read at the staged tip should not error"),
        Some(child_v7),
    );
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

    let (store, _services) = data_store_with_remote(temp.path(), server.uri(), checkpoint);
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
    let (_temp, store) = data_store();
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
    let (_temp, mut store) = data_store();
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
    let (_temp, mut store) = data_store();
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
    let (_temp, mut store) = data_store();
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
    let (_temp, mut store) = data_store();
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
    let (_temp, store) = data_store();
    let owner = SuiAddress::random_for_testing_only();
    let other_owner = SuiAddress::random_for_testing_only();
    let first_id = ObjectID::random();
    let second_id = ObjectID::random();
    let other_id = ObjectID::random();
    let first = make_gas_object(first_id, 1, Owner::AddressOwner(owner));
    let second = make_gas_object(second_id, 1, Owner::AddressOwner(owner));
    let other = make_gas_object(other_id, 1, Owner::AddressOwner(other_owner));

    store
        .local_store()
        .restore_seed_objects(&[first, second, other])
        .unwrap();

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
    let (_temp, store) = data_store();
    let owner = SuiAddress::random_for_testing_only();
    let object_id = ObjectID::random();
    let object = make_gas_object(object_id, 1, Owner::AddressOwner(owner));
    store.local_store().restore_seed_objects(&[object]).unwrap();

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

/// A store or remote failure during a child read must surface as an error: swallowed into
/// `Ok(None)`, it would reach Move execution as "child not found" and be durably committed as a
/// wrong result.
#[tokio::test]
async fn test_read_child_object_propagates_store_errors() {
    let (_temp, store) = data_store();
    let parent = ObjectID::random();
    let child = ObjectID::random();

    let err = RuntimeObjectResolver::read_child_object(
        &store,
        &parent,
        &child,
        SequenceNumber::from_u64(5),
    )
    .expect_err("an unreachable remote must surface as an error, not as a missing child");
    assert!(
        err.to_string().contains(&child.to_string()),
        "error should name the child object: {err}",
    );
}
