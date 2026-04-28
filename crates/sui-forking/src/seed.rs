// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Seed resolution for the initial owned-object index.
//!
//! Address seeds resolve lightweight object metadata at the fork checkpoint, while
//! explicit object seeds also cache the full object BCS through the existing object query path.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Context as _;
use anyhow::Error;
use anyhow::anyhow;
use anyhow::bail;
use itertools::Itertools as _;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use tracing::warn;

use move_core_types::language_storage::StructTag;
use sui_types::SUI_FRAMEWORK_ADDRESS;
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
use crate::gql::GraphQLClient;

const ADDRESS_OBJECTS_PAGE_SIZE: i32 = 50;
const ADDRESS_OBJECTS_QUERY: &str = r#"
query($sequenceNumber: UInt53, $address: SuiAddress!, $first: Int, $after: String) {
  checkpoint(sequenceNumber: $sequenceNumber) {
    query {
      address(address: $address) {
        objects(first: $first, after: $after) {
          nodes {
            address
            version
            digest
            owner {
              __typename
              ... on AddressOwner { address { address } }
            }
            contents {
              type { repr }
              json
            }
          }
          pageInfo { hasNextPage endCursor }
        }
      }
    }
  }
}
"#;

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

#[derive(Deserialize)]
struct AddressNode {
    address: String,
}

#[derive(Deserialize)]
struct AddressObjectsAddress {
    objects: MoveObjectConnection,
}

#[derive(Deserialize)]
struct AddressObjectsCheckpoint {
    query: Option<AddressObjectsScopedQuery>,
}

#[derive(Deserialize)]
struct AddressObjectsQuery {
    checkpoint: Option<AddressObjectsCheckpoint>,
}

#[derive(Deserialize)]
struct AddressObjectsScopedQuery {
    address: Option<AddressObjectsAddress>,
}

#[derive(Deserialize)]
struct MoveObjectConnection {
    nodes: Vec<MoveObjectNode>,
    #[serde(rename = "pageInfo")]
    page_info: PageInfo,
}

#[derive(Deserialize)]
struct MoveObjectContents {
    #[serde(rename = "type")]
    object_type: Option<MoveTypeNode>,
    json: Option<Value>,
}

#[derive(Deserialize)]
struct MoveObjectNode {
    address: String,
    version: Option<u64>,
    digest: Option<String>,
    owner: Option<OwnerNode>,
    contents: Option<MoveObjectContents>,
}

#[derive(Deserialize)]
struct MoveTypeNode {
    repr: String,
}

#[derive(Deserialize)]
struct OwnerNode {
    #[serde(rename = "__typename")]
    typename: String,
    address: Option<AddressNode>,
}

#[derive(Deserialize)]
struct PageInfo {
    #[serde(rename = "hasNextPage")]
    has_next_page: bool,
    #[serde(rename = "endCursor")]
    end_cursor: Option<String>,
}

impl MoveObjectNode {
    fn try_into_seed_entry(self) -> Result<Option<SeedEntry>, Error> {
        let Some(owner) = self.owner else {
            return Ok(None);
        };
        if owner.typename != "AddressOwner" {
            return Ok(None);
        }
        let owner = owner
            .address
            .ok_or_else(|| anyhow!("address-owned seed entry is missing owner address"))?
            .address
            .parse::<SuiAddress>()
            .with_context(|| format!("invalid seed owner for object {}", self.address))?;
        let contents = self
            .contents
            .ok_or_else(|| anyhow!("seed object {} is missing contents", self.address))?;
        let object_type = contents
            .object_type
            .ok_or_else(|| anyhow!("seed object {} is missing type", self.address))?
            .repr
            .parse::<StructTag>()
            .with_context(|| format!("invalid seed object type for {}", self.address))?;
        let object_id = self
            .address
            .parse::<ObjectID>()
            .with_context(|| format!("invalid seed object id {}", self.address))?;
        let digest = self
            .digest
            .ok_or_else(|| anyhow!("seed object {object_id} is missing digest"))?
            .parse::<ObjectDigest>()
            .with_context(|| format!("invalid seed object digest for {object_id}"))?;
        let version = self
            .version
            .ok_or_else(|| anyhow!("seed object {object_id} is missing version"))?;
        let balance = coin_balance_from_json(&object_type, contents.json.as_ref());

        Ok(Some(SeedEntry {
            object_id,
            version: SequenceNumber::from_u64(version),
            digest,
            owner,
            object_type,
            balance,
        }))
    }
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

fn coin_balance_from_json(object_type: &StructTag, json: Option<&Value>) -> Option<u64> {
    if object_type.address != SUI_FRAMEWORK_ADDRESS
        || object_type.module.as_ident_str().as_str() != "coin"
        || object_type.name.as_ident_str().as_str() != "Coin"
    {
        return None;
    }

    let balance = json?.get("balance")?;
    balance
        .as_u64()
        .or_else(|| balance.as_str().and_then(|value| value.parse().ok()))
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
    let mut entries = Vec::new();
    let mut cursor = None;

    loop {
        let response = gql
            .run_raw_query::<AddressObjectsQuery>(
                ADDRESS_OBJECTS_QUERY,
                json!({
                    "sequenceNumber": checkpoint,
                    "address": address.to_string(),
                    "first": ADDRESS_OBJECTS_PAGE_SIZE,
                    "after": cursor,
                }),
            )
            .await
            .with_context(|| format!("failed to query owned objects for {address}"))?;

        let data = response.data.ok_or_else(|| {
            anyhow!(
                "missing data in address objects query response for {address}: {:?}",
                response.errors,
            )
        })?;
        let checkpoint_data = data
            .checkpoint
            .ok_or_else(|| anyhow!("checkpoint {checkpoint} not found for address seeding"))?;
        let scoped_query = checkpoint_data
            .query
            .ok_or_else(|| anyhow!("missing checkpoint-scoped query for address seeding"))?;
        let Some(address_data) = scoped_query.address else {
            return Ok(entries);
        };

        for node in address_data.objects.nodes {
            if let Some(entry) = node.try_into_seed_entry()? {
                entries.push(entry);
            }
        }

        if !address_data.objects.page_info.has_next_page {
            break;
        }
        cursor = address_data.objects.page_info.end_cursor;
    }

    Ok(entries)
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
    use sui_types::gas_coin::GasCoin;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::body_partial_json;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    use super::*;
    use crate::Node;

    fn address_objects_response(
        nodes: Vec<serde_json::Value>,
        has_next_page: bool,
        end_cursor: Option<&str>,
    ) -> serde_json::Value {
        json!({
            "data": {
                "checkpoint": {
                    "query": {
                        "address": {
                            "objects": {
                                "nodes": nodes,
                                "pageInfo": {
                                    "hasNextPage": has_next_page,
                                    "endCursor": end_cursor,
                                }
                            }
                        }
                    }
                }
            }
        })
    }

    fn address_owned_node(object: &Object, owner: SuiAddress) -> serde_json::Value {
        json!({
            "address": object.id().to_string(),
            "version": object.version().value(),
            "digest": object.digest().to_string(),
            "owner": {
                "__typename": "AddressOwner",
                "address": { "address": owner.to_string() },
            },
            "contents": {
                "type": { "repr": object.struct_tag().unwrap().to_canonical_string(true) },
                "json": { "balance": "123" },
            }
        })
    }

    fn consensus_owned_node(object: &Object, owner: SuiAddress) -> serde_json::Value {
        json!({
            "address": object.id().to_string(),
            "version": object.version().value(),
            "digest": object.digest().to_string(),
            "owner": {
                "__typename": "ConsensusAddressOwner",
                "address": { "address": owner.to_string() },
            },
            "contents": {
                "type": { "repr": object.struct_tag().unwrap().to_canonical_string(true) },
                "json": { "balance": "456" },
            }
        })
    }

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

    #[test]
    fn coin_balance_from_json_reads_string_balance() {
        let ty = GasCoin::type_();
        let json = json!({
            "id": "0x1",
            "balance": "42",
        });

        assert_eq!(coin_balance_from_json(&ty, Some(&json)), Some(42));
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
    async fn resolve_address_seed_paginates_and_skips_consensus_owned_objects() {
        let server = MockServer::start().await;
        let owner = SuiAddress::random_for_testing_only();
        let first = Object::with_id_owner_version_for_testing(
            ObjectID::random(),
            SequenceNumber::from_u64(7),
            Owner::AddressOwner(owner),
        );
        let second = Object::with_id_owner_version_for_testing(
            ObjectID::random(),
            SequenceNumber::from_u64(8),
            Owner::ConsensusAddressOwner {
                start_version: SequenceNumber::from_u64(8),
                owner,
            },
        );

        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_partial_json(json!({
                "variables": { "after": null }
            })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(address_objects_response(
                    vec![address_owned_node(&first, owner)],
                    true,
                    Some("cursor-1"),
                )),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_partial_json(json!({
                "variables": { "after": "cursor-1" }
            })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(address_objects_response(
                    vec![consensus_owned_node(&second, owner)],
                    false,
                    None,
                )),
            )
            .mount(&server)
            .await;

        let gql = GraphQLClient::new(Node::Custom(server.uri()), "test")
            .expect("graphql client should build");
        let entries = resolve_address_seed(&gql, owner, 10)
            .await
            .expect("address seed should resolve");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].object_id, first.id());
        assert_eq!(entries[0].version, first.version());
        assert_eq!(entries[0].balance, Some(123));
    }
}
