// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Seed resolution for the initial owned-object index.
//!
//! Address seeds resolve lightweight object metadata at the fork checkpoint, while
//! explicit object seeds also cache the full object BCS through the existing object query path.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Error;
use anyhow::bail;
use itertools::Itertools as _;
use serde::Deserialize;
use serde::Serialize;
use tracing::warn;

use move_core_types::language_storage::StructTag;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::digests::ObjectDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Object;
use sui_types::object::Owner;

use crate::DataStore;
use crate::ObjectKey;
use crate::ObjectRead as _;
use crate::VersionQuery;
use crate::filesystem::OwnedObjectEntry;
use crate::gql::AddressOwnedObject;
use crate::gql::GraphQLClient;

/// CLI seed input before it has been resolved against the upstream chain.
#[derive(Clone, Debug, Default)]
pub struct SeedInput {
    /// Addresses whose owned objects should seed the initial owned-object index.
    pub addresses: Vec<SuiAddress>,
    /// Object IDs to fetch and seed when they are address-owned.
    pub object_ids: Vec<ObjectID>,
}

/// Object metadata used to seed the initial owned-object index.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct SeedEntry {
    pub(crate) object_id: ObjectID,
    pub(crate) version: SequenceNumber,
    pub(crate) digest: ObjectDigest,
    pub(crate) owner: SuiAddress,
    pub(crate) object_type: StructTag,
    pub(crate) balance: Option<u64>,
}

/// Durable manifest for pre-fork seed metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct SeedManifest {
    pub(crate) network: String,
    pub(crate) checkpoint: CheckpointSequenceNumber,
    pub(crate) entries: Vec<SeedEntry>,
}

impl SeedEntry {
    fn from_object(object: &Object) -> Option<Self> {
        let Owner::AddressOwner(owner) = &object.owner else {
            return None;
        };

        Some(Self {
            object_id: object.id(),
            version: object.version(),
            digest: object.digest(),
            owner: *owner,
            object_type: object.struct_tag()?,
            balance: object.as_coin_maybe().map(|coin| coin.value()),
        })
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
            object_id: object.object_id,
            version: object.version,
            digest: object.digest,
            owner: object.owner,
            object_type: object.object_type,
            balance: object.balance,
        }
    }
}

impl From<&SeedEntry> for OwnedObjectEntry {
    fn from(entry: &SeedEntry) -> Self {
        Self {
            owner: entry.owner,
            object_id: entry.object_id,
            version: entry.version,
            object_type: entry.object_type.clone(),
            balance: entry.balance,
        }
    }
}

/// Reject seed inputs that would overwrite or reinterpret an existing manifest.
pub(crate) fn ensure_seed_policy(data_store: &DataStore, input: &SeedInput) -> Result<(), Error> {
    if data_store.local().seed_manifest_exists() && !input.is_empty() {
        bail!(
            "A seed manifest already exists at {}. To fork the same checkpoint with different seeds, use a different --data-dir.",
            data_store.local().seed_manifest_path().display(),
        );
    }
    Ok(())
}

/// Initialize the durable owned-object index from the seed manifest when it is safe to do so.
pub(crate) fn initialize_owned_index_from_seed(
    data_store: &DataStore,
    manifest: &SeedManifest,
) -> Result<(), Error> {
    if data_store.local().owned_object_index_exists() {
        return Ok(());
    }

    if let Some(checkpoint) = data_store.get_highest_verified_checkpoint()?
        && checkpoint.data().sequence_number > data_store.forked_at_checkpoint()
    {
        bail!(
            "seed manifest exists but the owned-object index is missing while local checkpoints have advanced past the fork checkpoint; refusing to rebuild stale seed state",
        );
    }

    let entries: Vec<_> = manifest
        .entries
        .iter()
        .map(OwnedObjectEntry::from)
        .collect();
    data_store.local().write_owned_object_entries(&entries)
}

/// Load or create the seed manifest for the current fork directory.
pub(crate) async fn prepare_seed_manifest(
    data_store: &DataStore,
    network: String,
    input: &SeedInput,
) -> Result<Option<SeedManifest>, Error> {
    if data_store.local().seed_manifest_exists() {
        if !input.is_empty() {
            bail!(
                "A seed manifest already exists at {}. To fork the same checkpoint with different seeds, use a different --data-dir.",
                data_store.local().seed_manifest_path().display(),
            );
        }
        return data_store.local().read_seed_manifest().map(Some);
    }

    if input.is_empty() {
        return Ok(None);
    }

    let manifest = resolve_seeds(input, network, data_store).await?;
    data_store.local().write_seed_manifest(&manifest)?;
    Ok(Some(manifest))
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

fn resolve_object_seeds(
    data_store: &DataStore,
    checkpoint: CheckpointSequenceNumber,
    object_ids: &[ObjectID],
) -> Result<Vec<SeedEntry>, Error> {
    if object_ids.is_empty() {
        return Ok(Vec::new());
    }

    let keys: Vec<_> = object_ids
        .iter()
        .map(|object_id| ObjectKey {
            object_id: *object_id,
            version_query: VersionQuery::AtCheckpoint(checkpoint),
        })
        .collect();
    let objects = data_store.gql().get_objects(&keys)?;
    let mut entries = Vec::new();

    for (object_id, object) in object_ids.iter().zip_eq(objects) {
        let Some((object, _)) = object else {
            warn!(%object_id, checkpoint, "object seed not found at fork checkpoint");
            continue;
        };
        data_store.local().write_object(&object)?;
        if let Some(entry) = SeedEntry::from_object(&object) {
            entries.push(entry);
        } else {
            warn!(
                %object_id,
                checkpoint,
                "object seed is not address-owned and will not be added to the owned-object index",
            );
        }
    }

    Ok(entries)
}

async fn resolve_seeds(
    input: &SeedInput,
    network: String,
    data_store: &DataStore,
) -> Result<SeedManifest, Error> {
    let checkpoint = data_store.forked_at_checkpoint();
    let mut entries = BTreeMap::new();

    if !input.addresses.is_empty() {
        ensure_address_seeding_available(data_store.gql(), checkpoint)?;
    }

    for address in dedupe_addresses(&input.addresses) {
        let address_entries = resolve_address_seed(data_store.gql(), address, checkpoint).await?;
        if address_entries.is_empty() {
            warn!(%address, checkpoint, "address seed resolved no owned objects");
        }
        for entry in address_entries {
            entries.insert(entry.object_id, entry);
        }
    }

    let remaining_object_ids: Vec<_> = dedupe_object_ids(&input.object_ids)
        .into_iter()
        .filter(|object_id| !entries.contains_key(object_id))
        .collect();
    for entry in resolve_object_seeds(data_store, checkpoint, &remaining_object_ids)? {
        entries.insert(entry.object_id, entry);
    }

    Ok(SeedManifest {
        network,
        checkpoint,
        entries: entries.into_values().collect(),
    })
}

#[cfg(test)]
mod tests {
    use fastcrypto::encoding::Base64 as FastCryptoBase64;
    use fastcrypto::encoding::Encoding as _;
    use serde_json::json;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    use super::*;

    fn object_response_body(object: &Object) -> serde_json::Value {
        json!({
            "data": {
                "multiGetObjects": [{
                    "address": object.id().to_string(),
                    "version": object.version().value(),
                    "objectBcs": FastCryptoBase64::from_bytes(
                        &bcs::to_bytes(object).expect("object should serialize"),
                    )
                    .encoded(),
                }]
            }
        })
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
    async fn prepare_seed_manifest_does_not_write_manifest_when_resolution_fails() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().expect("tempdir");
        let store =
            DataStore::new_for_testing_with_remote(temp.path().to_path_buf(), server.uri(), 11);
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

        assert!(
            err.to_string().contains("Failed to read response")
                || err.to_string().contains("Object bcs is None")
                || err.to_string().contains("Missing data")
        );
        assert!(!store.local().seed_manifest_exists());
    }

    #[tokio::test]
    async fn prepare_seed_manifest_fetches_explicit_object_and_caches_bcs() {
        let server = MockServer::start().await;
        let owner = SuiAddress::random_for_testing_only();
        let object = Object::with_id_owner_version_for_testing(
            ObjectID::random(),
            SequenceNumber::from_u64(3),
            Owner::AddressOwner(owner),
        );
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(object_response_body(&object)))
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().expect("tempdir");
        let store =
            DataStore::new_for_testing_with_remote(temp.path().to_path_buf(), server.uri(), 11);
        let manifest = prepare_seed_manifest(
            &store,
            "custom".to_owned(),
            &SeedInput {
                addresses: vec![],
                object_ids: vec![object.id()],
            },
        )
        .await
        .expect("seed manifest should resolve")
        .expect("manifest should exist");

        assert_eq!(manifest.entries.len(), 1);
        assert_eq!(manifest.entries[0].object_id, object.id());
        assert_eq!(
            store
                .local()
                .get_object_at_version(&object.id(), object.version().value())
                .expect("local object lookup should not fail")
                .unwrap(),
            object,
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
        let store =
            DataStore::new_for_testing_with_remote(temp.path().to_path_buf(), server.uri(), 11);
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
        assert!(!store.local().seed_manifest_exists());
        assert_eq!(
            server
                .received_requests()
                .await
                .expect("wiremock should record requests")
                .len(),
            1,
        );
    }
}
