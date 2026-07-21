// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Fork manifest and seed resolution for seed-bounded owned-object tracking.
//!
//! The manifest is written for every initialized fork directory. Address and explicit object
//! seeds resolve lightweight object-ref metadata at the fork checkpoint. Full object BCS is
//! saved into `sui-rpc-store` during startup so address-owned RPC indexes are bounded by
//! seed input plus local execution.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Error;
use anyhow::bail;
use itertools::Itertools as _;
use serde::Deserialize;
use serde::Serialize;
use tracing::warn;

use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::ForkStore;
use crate::gql::AddressOwnedObject;
use crate::gql::GraphQLClient;
use crate::gql::ObjectSeedMetadata;
use crate::metadata::ForkMetadataStore;

/// CLI seed input before it has been resolved against the upstream chain.
#[derive(Clone, Debug, Default)]
pub struct SeedInput {
    /// Addresses whose owned objects should be recorded in the seed manifest.
    pub addresses: Vec<SuiAddress>,
    /// Object IDs to fetch and seed when they are owned by an address.
    pub object_ids: Vec<ObjectID>,
}

/// Object reference used to seed lazy owned-object index initialization.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct SeedEntry {
    pub(crate) object_ref: ObjectRef,
}

/// Durable manifest for fork metadata and optional pre-fork seed metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct SeedManifest {
    pub(crate) network: String,
    pub(crate) checkpoint: CheckpointSequenceNumber,
    /// Addresses whose owned objects were enumerated *completely* at the fork
    /// checkpoint to produce this manifest. Seeding one of these addresses is
    /// the same full scan the lazy read-time inventory initialization would
    /// run, so saving the manifest also marks their owner inventories
    /// complete. Absent in manifests written before this field existed
    /// (`serde(default)`), in which case reads fall back to lazy
    /// initialization.
    #[serde(default)]
    pub(crate) addresses: Vec<SuiAddress>,
    pub(crate) entries: Vec<SeedEntry>,
}

impl SeedManifest {
    fn empty(network: String, checkpoint: CheckpointSequenceNumber) -> Self {
        Self {
            network,
            checkpoint,
            addresses: Vec::new(),
            entries: Vec::new(),
        }
    }
}

impl SeedInput {
    /// Return true when no addresses or objects were requested for seeding.
    pub fn is_empty(&self) -> bool {
        self.addresses.is_empty() && self.object_ids.is_empty()
    }
}

impl From<AddressOwnedObject> for SeedEntry {
    fn from(object: AddressOwnedObject) -> Self {
        Self {
            object_ref: object.object_ref,
        }
    }
}

/// Reject seed inputs that would overwrite or reinterpret an existing manifest.
pub(crate) fn ensure_seed_policy(
    local: &ForkMetadataStore,
    input: &SeedInput,
) -> Result<(), Error> {
    if local.seed_manifest_exists() && !input.is_empty() {
        bail!(
            "A seed manifest already exists at {}. To fork the same checkpoint with different seeds, use a different --data-dir.",
            local.seed_manifest_path().display(),
        );
    }
    Ok(())
}

/// Ensure an existing fork manifest matches the requested network and checkpoint.
pub(crate) fn ensure_seed_manifest_matches(
    manifest: &SeedManifest,
    network: &str,
    checkpoint: Option<CheckpointSequenceNumber>,
) -> Result<(), Error> {
    if manifest.network != network {
        bail!(
            "Seed manifest network {} does not match requested network {}. Use a different --data-dir.",
            manifest.network,
            network,
        );
    }

    if let Some(checkpoint) = checkpoint
        && manifest.checkpoint != checkpoint
    {
        bail!(
            "Seed manifest checkpoint {} does not match requested checkpoint {}. Use a different --data-dir.",
            manifest.checkpoint,
            checkpoint,
        );
    }

    Ok(())
}

/// Load or create the seed manifest for the current fork directory.
pub(crate) async fn prepare_seed_manifest(
    store: &ForkStore,
    network: String,
    input: &SeedInput,
) -> Result<SeedManifest, Error> {
    if store.metadata().seed_manifest_exists() {
        if !input.is_empty() {
            bail!(
                "A seed manifest already exists at {}. To fork the same checkpoint with different seeds, use a different --data-dir.",
                store.metadata().seed_manifest_path().display(),
            );
        }
        let manifest = store.metadata().read_seed_manifest()?;
        ensure_seed_manifest_matches(&manifest, &network, Some(store.forked_at_checkpoint()))?;
        return Ok(manifest);
    }

    let manifest = if input.is_empty() {
        SeedManifest::empty(network, store.forked_at_checkpoint())
    } else {
        resolve_seeds(input, network, store).await?
    };
    store.metadata().write_seed_manifest(&manifest)?;
    Ok(manifest)
}

/// Materialize every manifest entry into the rpc-store, then mark the
/// manifest's fully-scanned addresses as having a complete owner inventory.
///
/// An address seed is resolved with the same complete owned-objects scan that
/// lazy read-time inventory initialization would run, so once its entries are
/// saved (with index rows) the address's inventory *is* initialized;
/// recording the marker keeps owner-scoped reads from re-running the
/// identical remote scan. Markers are written only after every entry is
/// durably saved — a crash in between simply falls back to lazy
/// initialization. Explicit object-id seeds never mark their owners: they are
/// not a complete scan.
pub(crate) fn save_seed_manifest_objects(
    store: &ForkStore,
    manifest: &SeedManifest,
) -> Result<(), Error> {
    let object_refs: Vec<_> = manifest
        .entries
        .iter()
        .map(|entry| entry.object_ref)
        .collect();
    store.save_address_owned_seed_objects(&object_refs)?;

    for address in &manifest.addresses {
        store
            .metadata()
            .mark_address_owner_inventory_complete(*address)?;
    }
    Ok(())
}

fn dedupe_addresses(addresses: &[SuiAddress]) -> Vec<SuiAddress> {
    addresses
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn dedupe_object_ids(object_ids: &[ObjectID]) -> Vec<ObjectID> {
    object_ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn ensure_address_seeding_available(
    gql: &GraphQLClient,
    checkpoint: CheckpointSequenceNumber,
) -> Result<(), Error> {
    let lowest_available = gql.get_lowest_available_checkpoint_objects()?;
    if checkpoint < lowest_available {
        bail!(
            "address seeding is unavailable at checkpoint {checkpoint}; object ownership enumeration is available starting at checkpoint {lowest_available}. Use --object for older checkpoints.",
        );
    }
    Ok(())
}

async fn resolve_address_seed(
    gql: &GraphQLClient,
    address: SuiAddress,
    checkpoint: CheckpointSequenceNumber,
) -> Result<Vec<SeedEntry>, Error> {
    Ok(gql
        .get_address_owned_objects_at_checkpoint(address, checkpoint)
        .await?
        .into_iter()
        .map(SeedEntry::from)
        .collect())
}

async fn resolve_object_seeds(
    gql: &GraphQLClient,
    checkpoint: CheckpointSequenceNumber,
    object_ids: &[ObjectID],
) -> Result<Vec<SeedEntry>, Error> {
    if object_ids.is_empty() {
        return Ok(Vec::new());
    }

    let objects = gql
        .get_object_seed_metadata_at_checkpoint(object_ids, checkpoint)
        .await?;
    let mut entries = Vec::new();

    for (object_id, object) in object_ids.iter().zip_eq(objects) {
        match object {
            ObjectSeedMetadata::Missing => {
                warn!(%object_id, checkpoint, "object seed not found at fork checkpoint");
            }
            ObjectSeedMetadata::NonAddressOwned => {
                warn!(
                    %object_id,
                    checkpoint,
                    "object seed is not owned by an address and will not be added to the seed manifest",
                );
            }
            ObjectSeedMetadata::AddressOwned(object) => entries.push(SeedEntry::from(object)),
        }
    }

    Ok(entries)
}

async fn resolve_seeds(
    input: &SeedInput,
    network: String,
    store: &ForkStore,
) -> Result<SeedManifest, Error> {
    let checkpoint = store.forked_at_checkpoint();
    let mut entries = BTreeMap::new();

    if !input.addresses.is_empty() {
        ensure_address_seeding_available(store.gql(), checkpoint)?;
    }

    let addresses = dedupe_addresses(&input.addresses);
    for address in addresses.iter().copied() {
        let address_entries = resolve_address_seed(store.gql(), address, checkpoint).await?;
        if address_entries.is_empty() {
            warn!(%address, checkpoint, "address seed resolved no owned objects");
        }
        for entry in address_entries {
            entries.insert(entry.object_ref.0, entry);
        }
    }

    let remaining_object_ids: Vec<_> = dedupe_object_ids(&input.object_ids)
        .into_iter()
        .filter(|object_id| !entries.contains_key(object_id))
        .collect();
    for entry in resolve_object_seeds(store.gql(), checkpoint, &remaining_object_ids).await? {
        entries.insert(entry.object_ref.0, entry);
    }

    Ok(SeedManifest {
        network,
        checkpoint,
        addresses,
        entries: entries.into_values().collect(),
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;
    use sui_types::base_types::SequenceNumber;
    use sui_types::digests::CheckpointDigest;
    use sui_types::object::Object;
    use sui_types::object::Owner;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::body_partial_json;
    use wiremock::matchers::body_string_contains;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    use crate::runtime::ForkRuntime;

    use super::*;

    fn test_data_store_with_remote(
        root: &Path,
        gql_url: String,
        forked_at_checkpoint: CheckpointSequenceNumber,
    ) -> (ForkStore, ForkRuntime) {
        let runtime = ForkRuntime::open(
            root,
            "custom".to_owned(),
            forked_at_checkpoint,
            CheckpointDigest::new([9; 32]).into(),
        )
        .expect("fork runtime should open");
        let store = ForkStore::new_for_testing_with_remote(
            root.to_path_buf(),
            gql_url,
            forked_at_checkpoint,
            runtime.local_store(),
        );
        (store, runtime)
    }

    fn object_seed_response_body(
        object: &Object,
        owner: SuiAddress,
        owner_type: &str,
    ) -> serde_json::Value {
        json!({
            "data": {
                "multiGetObjects": [{
                    "version": object.version().value(),
                    "digest": object.digest().to_string(),
                    "owner": {
                        "__typename": owner_type,
                        "address": { "address": owner.to_string() },
                    },
                }]
            }
        })
    }

    fn assert_object_seed_query_shape(query: &str) {
        assert!(query.contains("multiGetObjects"));
        assert!(query.contains("version"));
        assert!(query.contains("digest"));
        assert!(query.contains("... on AddressOwner"));
        assert!(query.contains("... on ConsensusAddressOwner"));
        assert!(!query.contains("objectBcs"));

        let object_selection_before_owner = query
            .split("multiGetObjects")
            .nth(1)
            .expect("query should include multiGetObjects")
            .split("owner")
            .next()
            .expect("query should include owner");
        assert!(
            !object_selection_before_owner
                .lines()
                .any(|line| line.trim() == "address"),
            "object seed query should not request Object.address",
        );
    }

    fn available_range_response(
        first_sequence_number: CheckpointSequenceNumber,
    ) -> serde_json::Value {
        json!({
            "data": {
                "serviceConfig": {
                    "availableRange": {
                        "first": {
                            "sequenceNumber": first_sequence_number,
                        }
                    }
                }
            }
        })
    }

    #[test]
    fn dedupe_object_ids_sorts_and_removes_duplicates() {
        let first = ObjectID::random();
        let second = ObjectID::random();
        let deduped = dedupe_object_ids(&[second, first, second]);

        assert_eq!(deduped.len(), 2);
        assert!(deduped[0] < deduped[1]);
    }

    #[tokio::test]
    async fn prepare_seed_manifest_writes_empty_manifest_without_seed_input() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (store, _runtime) =
            test_data_store_with_remote(temp.path(), "http://localhost:1".to_owned(), 11);

        let manifest = prepare_seed_manifest(&store, "custom".to_owned(), &SeedInput::default())
            .await
            .expect("empty seed manifest should be written");

        assert_eq!(
            manifest,
            SeedManifest {
                network: "custom".to_owned(),
                checkpoint: 11,
                addresses: Vec::new(),
                entries: Vec::new(),
            }
        );
        assert_eq!(store.metadata().read_seed_manifest().unwrap(), manifest);
    }

    #[tokio::test]
    async fn prepare_seed_manifest_does_not_write_manifest_when_resolution_fails() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().expect("tempdir");
        let (store, _runtime) = test_data_store_with_remote(temp.path(), server.uri(), 11);
        let err = prepare_seed_manifest(
            &store,
            "custom".to_owned(),
            &SeedInput {
                addresses: vec![],
                object_ids: vec![ObjectID::random()],
            },
        )
        .await
        .expect_err("seed resolution should fail");

        let err = format!("{err:?}");
        assert!(
            err.contains("failed to query object seeds")
                || err.contains("Failed to read response")
                || err.contains("Missing data")
        );
        assert!(!store.metadata().seed_manifest_exists());
    }

    #[tokio::test]
    async fn prepare_seed_manifest_fetches_explicit_object_metadata_without_caching_bcs() {
        let server = MockServer::start().await;
        let owner = SuiAddress::random_for_testing_only();
        let object = Object::with_id_owner_version_for_testing(
            ObjectID::random(),
            SequenceNumber::from_u64(3),
            Owner::AddressOwner(owner),
        );
        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_partial_json(json!({
                "variables": {
                    "keys": [{
                        "address": object.id().to_string(),
                        "atCheckpoint": 11,
                    }]
                }
            })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(object_seed_response_body(
                    &object,
                    owner,
                    "AddressOwner",
                )),
            )
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().expect("tempdir");
        let (store, _runtime) = test_data_store_with_remote(temp.path(), server.uri(), 11);
        let manifest = prepare_seed_manifest(
            &store,
            "custom".to_owned(),
            &SeedInput {
                addresses: vec![],
                object_ids: vec![object.id()],
            },
        )
        .await
        .expect("seed manifest should resolve");

        assert_eq!(manifest.entries.len(), 1);
        assert_eq!(
            manifest.entries[0].object_ref,
            object.compute_object_reference()
        );

        let requests = server
            .received_requests()
            .await
            .expect("wiremock should record requests");
        let request_body: serde_json::Value = requests[0]
            .body_json()
            .expect("request body should be json");
        let query = request_body
            .get("query")
            .and_then(serde_json::Value::as_str)
            .expect("query string should be present");
        assert_object_seed_query_shape(query);
    }

    #[tokio::test]
    async fn prepare_seed_manifest_fetches_explicit_consensus_address_owner_object() {
        let server = MockServer::start().await;
        let owner = SuiAddress::random_for_testing_only();
        let object = Object::with_id_owner_version_for_testing(
            ObjectID::random(),
            SequenceNumber::from_u64(3),
            Owner::ConsensusAddressOwner {
                start_version: SequenceNumber::from_u64(3),
                owner,
            },
        );
        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_partial_json(json!({
                "variables": {
                    "keys": [{
                        "address": object.id().to_string(),
                        "atCheckpoint": 11,
                    }]
                }
            })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(object_seed_response_body(
                    &object,
                    owner,
                    "ConsensusAddressOwner",
                )),
            )
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().expect("tempdir");
        let (store, _runtime) = test_data_store_with_remote(temp.path(), server.uri(), 11);
        let manifest = prepare_seed_manifest(
            &store,
            "custom".to_owned(),
            &SeedInput {
                addresses: vec![],
                object_ids: vec![object.id()],
            },
        )
        .await
        .expect("seed manifest should resolve");

        assert_eq!(manifest.entries.len(), 1);
        assert_eq!(
            manifest.entries[0].object_ref,
            object.compute_object_reference()
        );
    }

    #[tokio::test]
    async fn prepare_seed_manifest_rejects_address_seed_before_object_available_range() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(available_range_response(12)))
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().expect("tempdir");
        let (store, _runtime) = test_data_store_with_remote(temp.path(), server.uri(), 11);
        let err = prepare_seed_manifest(
            &store,
            "custom".to_owned(),
            &SeedInput {
                addresses: vec![SuiAddress::random_for_testing_only()],
                object_ids: vec![],
            },
        )
        .await
        .expect_err("address seed should fail before object available range");

        assert!(err.to_string().contains("address seeding is unavailable"));
        assert!(!store.metadata().seed_manifest_exists());
        assert_eq!(
            server
                .received_requests()
                .await
                .expect("wiremock should record requests")
                .len(),
            1,
        );
    }

    fn address_objects_response(objects: &[&Object]) -> serde_json::Value {
        json!({
            "data": {
                "checkpoint": {
                    "query": {
                        "address": {
                            "objects": {
                                "nodes": objects
                                    .iter()
                                    .map(|object| {
                                        json!({
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

    async fn mock_address_objects(
        server: &MockServer,
        checkpoint: u64,
        owner: SuiAddress,
        objects: &[&Object],
    ) {
        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_partial_json(json!({
                "variables": {
                    "sequenceNumber": checkpoint,
                    "address": owner.to_string(),
                    "after": null,
                }
            })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(address_objects_response(objects)),
            )
            .mount(server)
            .await;
    }

    #[test]
    fn seed_manifest_without_addresses_field_deserializes_with_empty_addresses() {
        // Manifests written before the `addresses` field existed must keep
        // loading; their owners simply fall back to lazy inventory init.
        let manifest: SeedManifest = serde_json::from_value(json!({
            "network": "testnet",
            "checkpoint": 42,
            "entries": [],
        }))
        .expect("pre-addresses manifest should deserialize");
        assert!(manifest.addresses.is_empty());
    }

    #[tokio::test]
    async fn prepare_seed_manifest_records_fully_scanned_addresses() {
        let server = MockServer::start().await;
        let owner = SuiAddress::random_for_testing_only();
        let empty_owner = SuiAddress::random_for_testing_only();
        let object = Object::with_id_owner_version_for_testing(
            ObjectID::random(),
            SequenceNumber::from_u64(3),
            Owner::AddressOwner(owner),
        );

        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_string_contains("availableRange"))
            .respond_with(ResponseTemplate::new(200).set_body_json(available_range_response(0)))
            .mount(&server)
            .await;
        mock_address_objects(&server, 11, owner, &[&object]).await;
        mock_address_objects(&server, 11, empty_owner, &[]).await;

        let temp = tempfile::tempdir().expect("tempdir");
        let (store, _runtime) = test_data_store_with_remote(temp.path(), server.uri(), 11);
        let manifest = prepare_seed_manifest(
            &store,
            "custom".to_owned(),
            &SeedInput {
                addresses: vec![owner, empty_owner],
                object_ids: vec![],
            },
        )
        .await
        .expect("seed manifest should resolve");

        // Both requested addresses are recorded as fully scanned — including
        // the one that owns nothing — and the manifest round-trips.
        let mut expected = vec![owner, empty_owner];
        expected.sort();
        assert_eq!(manifest.addresses, expected);
        assert_eq!(manifest.entries.len(), 1);
        assert_eq!(
            manifest.entries[0].object_ref,
            object.compute_object_reference()
        );
        assert_eq!(store.metadata().read_seed_manifest().unwrap(), manifest);
    }

    #[tokio::test]
    async fn save_seed_manifest_objects_marks_seeded_address_inventories_complete() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (store, _runtime) =
            test_data_store_with_remote(temp.path(), "http://localhost:1".to_owned(), 11);
        let owner = SuiAddress::random_for_testing_only();
        let empty_owner = SuiAddress::random_for_testing_only();
        let object = Object::with_id_owner_version_for_testing(
            ObjectID::random(),
            SequenceNumber::from_u64(3),
            Owner::AddressOwner(owner),
        );
        store
            .local_store()
            .save_object_version_only(&object)
            .unwrap();

        let manifest = SeedManifest {
            network: "custom".to_owned(),
            checkpoint: 11,
            addresses: vec![owner, empty_owner],
            entries: vec![SeedEntry {
                object_ref: object.compute_object_reference(),
            }],
        };
        save_seed_manifest_objects(&store, &manifest).expect("seed save should succeed");

        for address in [owner, empty_owner] {
            assert!(
                store
                    .metadata()
                    .address_owner_inventory_complete(address)
                    .unwrap(),
                "seeded address should be marked inventory-complete",
            );
        }

        // The GraphQL endpoint is unreachable, so these owner-scoped reads
        // only succeed if the marker prevents a second full owner scan.
        let infos: Vec<_> = store
            .owned_objects_iter(owner, None, None)
            .expect("seeded owner read should not re-scan the remote")
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].object_id, object.id());
        assert_eq!(
            store
                .owned_objects_iter(empty_owner, None, None)
                .expect("empty seeded owner read should not re-scan the remote")
                .count(),
            0,
        );
    }
}
