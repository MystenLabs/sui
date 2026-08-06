// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Fork manifest and seed resolution for seed-bounded index reads.
//!
//! Seeding happens once, at fork creation, in two steps. Resolution enumerates the requested
//! addresses and objects against GraphQL pinned at the fork checkpoint and records the resulting
//! object references in an immutable manifest, and that enumeration is the part that must be
//! complete and must happen while the enumeration is still possible, because nothing the fork does
//! afterwards reconstructs it. The load then hydrates those references and hands them to
//! `sui-rpc-store`'s `Restore` pipelines, which is where the fork's whole pre-fork derived-index
//! surface comes from.
//!
//! Everything downstream is bounded by that. An owner, parent, or type outside the seed set is not
//! indexed, and reads for it come back empty rather than reaching for the remote at read time.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Context as _;
use anyhow::Error;
use anyhow::bail;
use itertools::Itertools as _;
use serde::Deserialize;
use serde::Serialize;
use tracing::warn;

use move_core_types::language_storage::TypeTag;
use sui_types::accumulator_root::AccumulatorValue;
use sui_types::balance::Balance;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::ForkStore;
use crate::gql::AddressOwnedObject;
use crate::gql::ObjectSeedMetadata;
use crate::metadata::MetadataStore;
use crate::remote::RemoteSource;

/// Objects hydrated per remote round-trip. The load commits as one batch regardless, and this only
/// bounds the size of an individual GraphQL query.
const HYDRATE_CHUNK: usize = 50;

/// CLI seed input before it has been resolved against the upstream chain.
#[derive(Clone, Debug, Default)]
pub struct SeedInput {
    /// Addresses whose owned objects should be recorded in the seed manifest.
    pub addresses: BTreeSet<SuiAddress>,
    /// Object IDs to fetch and seed when they are owned by an address.
    pub object_ids: BTreeSet<ObjectID>,
}

/// Object reference recorded in the manifest and hydrated by the seed load.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct SeedEntry {
    pub(crate) object_ref: ObjectRef,
}

/// Durable manifest for fork metadata and optional pre-fork seed metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct SeedManifest {
    pub(crate) network: String,
    pub(crate) checkpoint: CheckpointSequenceNumber,
    /// Addresses that were fully enumerated to produce this manifest. Nothing reads the list, and
    /// it exists as a record for whoever inspects the fork directory of which addresses the
    /// seeding used.
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
pub(crate) fn ensure_seed_policy(local: &MetadataStore, input: &SeedInput) -> Result<(), Error> {
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

/// Load every manifest entry into the rpc-store with its full derived-index surface, once, before
/// the fork executes anything.
///
/// The manifest holds object references rather than objects, so each entry is fetched by id and
/// version and handed to the restore pipelines, which rebuild the owned-object index and the rest
/// of the derived surface. The load runs at most once per fork directory, because `Balance`
/// accumulates through a merge operator and a second pass would double-count every seeded coin,
/// and it commits its own completion marker atomically with the rows to make that unrepeatable
/// rather than merely unlikely.
pub(crate) fn load_seed_objects(store: &ForkStore, manifest: &SeedManifest) -> Result<(), Error> {
    if store.local_store().seed_load_complete()? {
        return Ok(());
    }

    let object_refs: Vec<_> = manifest
        .entries
        .iter()
        .map(|entry| entry.object_ref)
        .collect();

    let mut objects = Vec::with_capacity(object_refs.len());
    for chunk in object_refs.chunks(HYDRATE_CHUNK) {
        objects.extend(store.fetch_seed_objects(chunk)?);
    }

    store.local_store().restore_seed_objects(&objects)
}

async fn resolve_address_seed(
    remote: &RemoteSource,
    address: SuiAddress,
) -> Result<Vec<SeedEntry>, Error> {
    let mut entries: Vec<SeedEntry> = remote
        .address_owned_objects_at_fork(address)
        .await?
        .into_iter()
        .map(SeedEntry::from)
        .collect();
    entries.extend(resolve_address_balance_seed(remote, address).await?);
    Ok(entries)
}

/// Resolve the accumulator balance fields belonging to `address`.
///
/// An address balance lives in a dynamic field under the accumulator root rather than in an object
/// the address owns, so the owned-object enumeration above never surfaces it, and seeding an
/// address without this step would establish its coins while silently leaving its balance at zero.
/// Local execution does maintain address balances, so a withdrawal would then apply a delta to a
/// baseline that was never seeded.
///
/// The field's id is derivable from `(address, coin type)`, so the only thing that has to come
/// from the remote is which coin types to derive for, which is the one part nothing local can
/// know. Each derived id is then resolved like any other object reference and seeded as an
/// ordinary object, so the stock `Balance` restore pipeline picks it up through its
/// accumulator-root arm without this crate writing a balance row itself.
async fn resolve_address_balance_seed(
    remote: &RemoteSource,
    address: SuiAddress,
) -> Result<Vec<SeedEntry>, Error> {
    let checkpoint = remote.forked_at_checkpoint();
    let coin_types = remote
        .address_balance_coin_types_at_fork(address)
        .await
        .with_context(|| format!("failed to resolve address balances for {address}"))?;
    if coin_types.is_empty() {
        return Ok(Vec::new());
    }

    let mut field_ids = Vec::with_capacity(coin_types.len());
    for coin_type in &coin_types {
        match accumulator_field_id(address, coin_type) {
            Ok(field_id) => field_ids.push(field_id),
            // A coin type the accumulator cannot key on is not a failure of the
            // seed: it just has no field to fetch.
            Err(e) => warn!(%address, %coin_type, "skipping address balance seed: {e:#}"),
        }
    }

    let refs = remote
        .object_refs_at_fork(&field_ids)
        .await
        .with_context(|| format!("failed to resolve accumulator fields for {address}"))?;

    let mut entries = Vec::new();
    for (field_id, object_ref) in field_ids.iter().zip_eq(refs) {
        match object_ref {
            Some(object_ref) => entries.push(SeedEntry { object_ref }),
            // The balances connection reported a coin type, but no field exists
            // at the fork checkpoint. Nothing to seed, and nothing wrong.
            None => warn!(
                %address,
                %field_id,
                checkpoint,
                "address balance field not found at fork checkpoint",
            ),
        }
    }
    Ok(entries)
}

/// Derive the object id of the accumulator field holding `address`'s balance of `coin_type`.
///
/// `coin_type` arrives as the inner type (`0x2::sui::SUI`), while the accumulator keys its fields
/// on the wrapped `0x2::balance::Balance<T>`, so the id is derived from the wrapped form.
fn accumulator_field_id(address: SuiAddress, coin_type: &str) -> Result<ObjectID, Error> {
    let inner: TypeTag = coin_type
        .parse()
        .with_context(|| format!("invalid coin type {coin_type}"))?;
    Ok(*AccumulatorValue::get_field_id(address, &Balance::type_tag(inner))?.inner())
}

/// Resolve the requested object ids against the remote source at the fork checkpoint.
async fn resolve_object_seeds(
    remote: &RemoteSource,
    object_ids: &[ObjectID],
) -> Result<Vec<SeedEntry>, Error> {
    if object_ids.is_empty() {
        return Ok(Vec::new());
    }

    let checkpoint = remote.forked_at_checkpoint();
    let objects = remote.object_seed_metadata_at_fork(object_ids).await?;
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

/// Resolve the requested addresses and object ids against the remote source at the fork checkpoint.
async fn resolve_seeds(
    input: &SeedInput,
    network: String,
    store: &ForkStore,
) -> Result<SeedManifest, Error> {
    let checkpoint = store.forked_at_checkpoint();
    let mut entries = BTreeMap::new();

    // Address seeds are ignored when the fork checkpoint is older
    // than the remote's ownership-enumeration window. The skipped addresses must NOT be
    // recorded in the manifest. The address list claims a complete scan, and
    // claiming one that never ran is worse than recording nothing.
    let addresses: Vec<SuiAddress> = if input.addresses.is_empty() {
        Vec::new()
    } else {
        let lowest_available = store.remote().lowest_available_checkpoint_objects()?;
        if checkpoint < lowest_available {
            warn!(
                addresses = ?input.addresses,
                checkpoint,
                lowest_available,
                "ignoring --address seeds: checkpoint {checkpoint} is older than the remote's \
                 object-ownership window (available from checkpoint {lowest_available}, roughly \
                 the last hour). If the object IDs are known, seed them directly with --object \
                 instead.",
            );
            Vec::new()
        } else {
            input.addresses.iter().copied().collect()
        }
    };
    for address in addresses.iter().copied() {
        let address_entries = resolve_address_seed(store.remote(), address).await?;
        if address_entries.is_empty() {
            warn!(%address, checkpoint, "address seed resolved no owned objects");
        }
        for entry in address_entries {
            entries.insert(entry.object_ref.0, entry);
        }
    }

    let remaining_object_ids: Vec<_> = input
        .object_ids
        .iter()
        .copied()
        .filter(|object_id| !entries.contains_key(object_id))
        .collect();
    for entry in resolve_object_seeds(store.remote(), &remaining_object_ids).await? {
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

    use crate::services::ServiceManager;

    use super::*;

    fn test_data_store_with_remote(
        root: &Path,
        gql_url: String,
        forked_at_checkpoint: CheckpointSequenceNumber,
    ) -> (ForkStore, ServiceManager) {
        let services = ServiceManager::open(
            root,
            "custom".to_owned(),
            forked_at_checkpoint,
            CheckpointDigest::new([9; 32]).into(),
        )
        .expect("service manager should open");
        let store = ForkStore::new_for_testing_with_remote(
            root.to_path_buf(),
            gql_url,
            forked_at_checkpoint,
            services.local_store(),
        );
        (store, services)
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

    #[tokio::test]
    async fn prepare_seed_manifest_writes_empty_manifest_without_seed_input() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (store, _services) =
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
        let (store, _services) = test_data_store_with_remote(temp.path(), server.uri(), 11);
        let err = prepare_seed_manifest(
            &store,
            "custom".to_owned(),
            &SeedInput {
                addresses: BTreeSet::new(),
                object_ids: BTreeSet::from([ObjectID::random()]),
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
        let (store, _services) = test_data_store_with_remote(temp.path(), server.uri(), 11);
        let manifest = prepare_seed_manifest(
            &store,
            "custom".to_owned(),
            &SeedInput {
                addresses: BTreeSet::new(),
                object_ids: BTreeSet::from([object.id()]),
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
        let (store, _services) = test_data_store_with_remote(temp.path(), server.uri(), 11);
        let manifest = prepare_seed_manifest(
            &store,
            "custom".to_owned(),
            &SeedInput {
                addresses: BTreeSet::new(),
                object_ids: BTreeSet::from([object.id()]),
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
    async fn prepare_seed_manifest_skips_address_seed_before_object_available_range() {
        // Fork checkpoint 11 is below the remote's ownership-enumeration
        // window (available from 12): address seeds are warned about and
        // ignored — not fatal — while explicit object seeds still resolve.
        let server = MockServer::start().await;
        let skipped_address = SuiAddress::random_for_testing_only();
        let owner = SuiAddress::random_for_testing_only();
        let object = Object::with_id_owner_version_for_testing(
            ObjectID::random(),
            SequenceNumber::from_u64(3),
            Owner::AddressOwner(owner),
        );

        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_string_contains("availableRange"))
            .respond_with(ResponseTemplate::new(200).set_body_json(available_range_response(12)))
            .mount(&server)
            .await;
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
        let (store, _services) = test_data_store_with_remote(temp.path(), server.uri(), 11);
        let manifest = prepare_seed_manifest(
            &store,
            "custom".to_owned(),
            &SeedInput {
                addresses: BTreeSet::from([skipped_address]),
                object_ids: BTreeSet::from([object.id()]),
            },
        )
        .await
        .expect("out-of-window address seed should be skipped, not fatal");

        // The skipped address is NOT recorded as fully scanned — recording it
        // would claim a complete owner enumeration that never ran — while the
        // object seed still lands.
        assert!(manifest.addresses.is_empty());
        assert_eq!(manifest.entries.len(), 1);
        assert_eq!(
            manifest.entries[0].object_ref,
            object.compute_object_reference()
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

    /// The owned-objects and balances queries take the same variables, so both
    /// mocks must discriminate on the selection set or the first-mounted one
    /// swallows the other's requests.
    async fn mock_address_objects(
        server: &MockServer,
        checkpoint: u64,
        owner: SuiAddress,
        objects: &[&Object],
    ) {
        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_string_contains("objects"))
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

    fn address_balances_response(balances: &[(&str, i128)]) -> serde_json::Value {
        json!({
            "data": {
                "checkpoint": {
                    "query": {
                        "address": {
                            "balances": {
                                "nodes": balances
                                    .iter()
                                    .map(|(coin_type, address_balance)| json!({
                                        "addressBalance": address_balance.to_string(),
                                        "coinType": { "repr": coin_type },
                                    }))
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

    async fn mock_address_balances(
        server: &MockServer,
        checkpoint: u64,
        owner: SuiAddress,
        balances: &[(&str, i128)],
    ) {
        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_string_contains("balances"))
            .and(body_partial_json(json!({
                "variables": {
                    "sequenceNumber": checkpoint,
                    "address": owner.to_string(),
                    "after": null,
                }
            })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(address_balances_response(balances)),
            )
            .mount(server)
            .await;
    }

    #[test]
    fn seed_manifest_without_addresses_field_deserializes_with_empty_addresses() {
        // Manifests written before the `addresses` field existed must keep
        // loading; they simply record no fully-scanned owners.
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
        mock_address_balances(&server, 11, owner, &[]).await;
        mock_address_balances(&server, 11, empty_owner, &[]).await;

        let temp = tempfile::tempdir().expect("tempdir");
        let (store, _services) = test_data_store_with_remote(temp.path(), server.uri(), 11);
        let manifest = prepare_seed_manifest(
            &store,
            "custom".to_owned(),
            &SeedInput {
                addresses: BTreeSet::from([owner, empty_owner]),
                object_ids: BTreeSet::new(),
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

    /// An address balance lives in a dynamic field under the accumulator root,
    /// not on the address, so the owned-object scan never reaches it. Seeding
    /// has to derive the field id from `(address, coin type)` and pull it in as
    /// an ordinary object, or the address seeds with its coins and a silently
    /// zero balance.
    #[tokio::test]
    async fn address_seed_pulls_in_the_accumulator_balance_field() {
        let server = MockServer::start().await;
        let owner = SuiAddress::random_for_testing_only();
        let coin_type = "0x2::sui::SUI";
        let field_id =
            accumulator_field_id(owner, coin_type).expect("field id should derive for SUI");
        let field = Object::with_id_owner_version_for_testing(
            field_id,
            SequenceNumber::from_u64(8),
            Owner::ObjectOwner(sui_types::SUI_ACCUMULATOR_ROOT_OBJECT_ID.into()),
        );

        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_string_contains("availableRange"))
            .respond_with(ResponseTemplate::new(200).set_body_json(available_range_response(0)))
            .mount(&server)
            .await;
        mock_address_objects(&server, 11, owner, &[]).await;
        mock_address_balances(&server, 11, owner, &[(coin_type, 5_000)]).await;
        // The derived id is resolved through the owner-agnostic ref lookup: the
        // field is object-owned, so the address-owned check that guards
        // user-named seeds would reject it.
        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_partial_json(json!({
                "variables": {
                    "keys": [{
                        "address": field_id.to_string(),
                        "atCheckpoint": 11,
                    }]
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {
                    "multiGetObjects": [{
                        "version": field.version().value(),
                        "digest": field.digest().to_string(),
                        "owner": { "__typename": "ObjectOwner" },
                    }]
                }
            })))
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().expect("tempdir");
        let (store, _services) = test_data_store_with_remote(temp.path(), server.uri(), 11);
        let manifest = prepare_seed_manifest(
            &store,
            "custom".to_owned(),
            &SeedInput {
                addresses: BTreeSet::from([owner]),
                object_ids: BTreeSet::new(),
            },
        )
        .await
        .expect("seed manifest should resolve");

        assert_eq!(manifest.entries.len(), 1, "{:?}", manifest.entries);
        assert_eq!(
            manifest.entries[0].object_ref,
            field.compute_object_reference(),
        );
    }

    /// A coin type reported with no accumulator balance has no field to seed;
    /// deriving and fetching one would be a wasted round-trip per coin type the
    /// address merely holds coins of.
    #[tokio::test]
    async fn address_seed_skips_coin_types_without_an_accumulator_balance() {
        let server = MockServer::start().await;
        let owner = SuiAddress::random_for_testing_only();

        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_string_contains("availableRange"))
            .respond_with(ResponseTemplate::new(200).set_body_json(available_range_response(0)))
            .mount(&server)
            .await;
        mock_address_objects(&server, 11, owner, &[]).await;
        mock_address_balances(&server, 11, owner, &[("0x2::sui::SUI", 0)]).await;

        let temp = tempfile::tempdir().expect("tempdir");
        let (store, _services) = test_data_store_with_remote(temp.path(), server.uri(), 11);
        let manifest = prepare_seed_manifest(
            &store,
            "custom".to_owned(),
            &SeedInput {
                addresses: BTreeSet::from([owner]),
                object_ids: BTreeSet::new(),
            },
        )
        .await
        .expect("seed manifest should resolve");

        // No multiGetObjects mock is mounted, so resolving a field here would
        // have failed the query rather than quietly returning nothing.
        assert!(manifest.entries.is_empty());
    }

    /// The accumulator keys on the wrapped `Balance<T>`, while the balances
    /// connection reports the inner `T`. Getting that wrapping wrong yields a
    /// plausible-looking id that simply never resolves.
    #[test]
    fn accumulator_field_id_keys_on_the_wrapped_balance_type() {
        let owner = SuiAddress::random_for_testing_only();
        let expected = AccumulatorValue::get_field_id(
            owner,
            &Balance::type_tag("0x2::sui::SUI".parse().unwrap()),
        )
        .expect("wrapped balance type should key");

        assert_eq!(
            accumulator_field_id(owner, "0x2::sui::SUI").unwrap(),
            *expected.inner(),
        );
        assert!(
            accumulator_field_id(owner, "not::a::type").is_err(),
            "an unparseable coin type must not derive an id",
        );
    }
}
