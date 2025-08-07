// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use cynic::QueryBuilder;
use sui_data_store::{ObjectKey, ObjectStore, VersionQuery};
use sui_pg_db::Db;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    object::Object,
};
use tracing::info;

use crate::rpc::objects::{download_packages, insert_package_into_db};
use crate::server::{
    insert_kv_object_into_db, insert_obj_info_into_db, insert_obj_version_into_db,
};
use crate::store::ForkingStore;

#[derive(Parser, Clone, Debug)]
pub struct InitialAccounts {
    /// Specific accounts to track ownership for
    #[clap(long, value_delimiter = ',')]
    pub accounts: Vec<SuiAddress>,
}

impl InitialAccounts {
    /// Process all initial accounts: fetch their owned objects, extract package dependencies,
    /// and insert everything into the store and database.
    pub async fn process(
        &self,
        context: &crate::context::Context,
        graphql_endpoint: &Network,
        at_checkpoint: u64,
    ) -> Result<()> {
        if self.accounts.is_empty() {
            return Ok(());
        }

        let mut sim = context.simulacrum.write().await;
        let store = sim.store_mut();
        let db_writer = &context.db_writer;

        // 1. Fetch object IDs owned by all accounts using GraphQL
        let mut all_object_ids = Vec::new();
        let mut package_ids = BTreeSet::new();
        for addr in &self.accounts {
            info!("Fetching object IDs for account {}", addr);
            let object_ids = fetch_owned_object_ids(graphql_endpoint, *addr).await?;
            info!("Found {} objects for account {}", object_ids.len(), addr);

            // if object_ids.is_empty() {
            //     // TODO: this is a simple hack where we think that a given addr is a package
            //     // instead of an account. We should do proper type checking here.
            //     package_ids.insert(ObjectID::from(*addr));
            //     all_object_ids.extend([ObjectID::from(*addr)]);
            //     continue;
            // }
            all_object_ids.extend(object_ids);
        }

        all_object_ids.extend(self.accounts.iter().map(|addr| ObjectID::from(*addr)));

        if all_object_ids.is_empty() {
            info!("No objects found for any initial accounts");
            return Ok(());
        }

        // 2. Fetch actual objects using rpc_data_store (with caching)
        let object_keys: Vec<ObjectKey> = all_object_ids
            .iter()
            .map(|id| ObjectKey {
                object_id: *id,
                version_query: VersionQuery::AtCheckpoint(at_checkpoint),
            })
            .collect();

        let fetched_objects = store
            .get_rpc_data_store()
            .get_objects(&object_keys)
            .context("Failed to fetch objects from RPC data store")?;

        let all_objects: Vec<Object> = fetched_objects
            .into_iter()
            .flatten()
            .map(|(obj, _version)| obj)
            .collect();

        info!("Fetched {} objects from RPC data store", all_objects.len());

        // 3. Extract all package IDs from object types (including nested generics)
        package_ids.extend(extract_package_ids_from_objects(&all_objects));
        info!(
            "Found {} unique package dependencies from object types",
            package_ids.len()
        );

        // 4. Download packages from RPC data store (also uses caching)
        let packages = download_packages(package_ids, store, &at_checkpoint).await?;
        info!("Downloaded {} packages", packages.len());

        // 5. Insert objects into ForkingStore
        let objects_map: BTreeMap<ObjectID, Object> =
            all_objects.iter().map(|o| (o.id(), o.clone())).collect();
        store.update_objects(objects_map, vec![]);

        // 6. Insert objects into obj_versions, kv_objects, and obj_info tables
        for object in &all_objects {
            insert_obj_version_into_db(db_writer, object, at_checkpoint as i64).await?;
            insert_kv_object_into_db(db_writer, object).await?;
            insert_obj_info_into_db(db_writer, object, at_checkpoint as i64).await?;
        }

        // 7. Insert packages into ForkingStore
        let packages_map: BTreeMap<ObjectID, Object> =
            packages.iter().map(|o| (o.id(), o.clone())).collect();
        store.update_objects(packages_map, vec![]);

        // 8. Insert packages into kv_packages table
        if !packages.is_empty() {
            info!("Inserting {} packages into the database", packages.len());
            insert_package_into_db(db_writer, &packages, at_checkpoint).await?;
        }

        info!(
            "Successfully processed {} objects and {} packages for initial accounts",
            all_objects.len(),
            packages.len()
        );

        Ok(())
    }
}

/// Extract all package IDs from object types, including nested generics like `Coin<ZEN>`
fn extract_package_ids_from_objects(objects: &[Object]) -> BTreeSet<ObjectID> {
    objects
        .iter()
        .filter_map(|o| o.struct_tag())
        .flat_map(|tag| tag.all_addresses())
        .map(ObjectID::from)
        .collect()
}

// ======= TEMP IMPLEMENTATION HACKED TOGETHER TO UNDERSTAND HOW THIS WORKS =======

/// Configuration for the network to fork from
#[derive(Clone, Debug)]
pub enum Network {
    Mainnet,
    Testnet,
    Devnet,
    Custom(String),
}

impl Network {
    pub fn graphql_url(&self) -> String {
        match self {
            Network::Mainnet => "https://graphql.mainnet.sui.io/graphql".to_string(),
            Network::Testnet => "https://graphql.testnet.sui.io/graphql".to_string(),
            Network::Devnet => "https://graphql.devnet.sui.io/graphql".to_string(),
            Network::Custom(url) => url.clone(),
        }
    }
}

impl std::str::FromStr for Network {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "mainnet" => Ok(Network::Mainnet),
            "testnet" => Ok(Network::Testnet),
            "devnet" => Ok(Network::Devnet),
            _ => Ok(Network::Custom(s.to_string())),
        }
    }
}

// Register the schema which was loaded in the build.rs call.
#[cynic::schema("rpc")]
mod schema {}

#[derive(cynic::QueryVariables, Debug)]
pub struct AddressVariable {
    pub address: SuiAddressScalar,
    pub after: Option<String>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "Query", variables = "AddressVariable")]
pub struct AddressQuery {
    #[arguments(address: $address)]
    pub address: Option<ObjectsQuery>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "Address", variables = "AddressVariable")]
pub struct ObjectsQuery {
    #[arguments(after: $after)]
    pub objects: Option<MoveObjectConnection>,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct MoveObjectConnection {
    pub edges: Vec<MoveObjectEdge>,
    pub page_info: PageInfo,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct PageInfo {
    pub end_cursor: Option<String>,
    pub has_next_page: bool,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct MoveObjectEdge {
    pub node: MoveObject,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct MoveObject {
    /// The object's address/ID
    pub address: SuiAddressScalar,
}

#[derive(cynic::Scalar, Debug, Clone)]
#[cynic(graphql_type = "SuiAddress")]
pub struct SuiAddressScalar(pub String);

/// Fetch all owned object IDs for a given address using GraphQL with pagination.
/// The actual objects are then fetched via rpc_data_store which provides caching.
async fn fetch_owned_object_ids(
    graphql_endpoint: &Network,
    address: SuiAddress,
) -> Result<Vec<ObjectID>> {
    let client = reqwest::Client::new();
    let mut all_object_ids = Vec::new();
    let mut cursor: Option<String> = None;
    let mut has_next_page = true;

    while has_next_page {
        let query = AddressQuery::build(AddressVariable {
            after: cursor.clone(),
            address: SuiAddressScalar(address.to_string()),
        });

        let response = client
            .post(graphql_endpoint.graphql_url())
            .json(&query)
            .send()
            .await
            .context("Failed to send GraphQL request")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());
            anyhow::bail!(
                "GraphQL request failed with status {}: {}",
                status,
                error_text
            );
        }

        let graphql_response: cynic::GraphQlResponse<AddressQuery> = response
            .json()
            .await
            .context("Failed to parse GraphQL response")?;

        if let Some(errors) = &graphql_response.errors {
            let error_messages: Vec<String> = errors.iter().map(|e| e.message.clone()).collect();
            anyhow::bail!("GraphQL errors: {}", error_messages.join(", "));
        }

        let data = graphql_response
            .data
            .ok_or_else(|| anyhow!("No data in GraphQL response"))?;

        let address_data = data
            .address
            .ok_or_else(|| anyhow!("Address not found in response"))?;

        let objects_connection = address_data
            .objects
            .ok_or_else(|| anyhow!("No objects connection in response"))?;

        for edge in objects_connection.edges {
            let object_id = ObjectID::from_hex_literal(&edge.node.address.0)
                .context("Failed to parse object ID from GraphQL response")?;
            all_object_ids.push(object_id);
        }

        has_next_page = objects_connection.page_info.has_next_page;
        cursor = objects_connection.page_info.end_cursor;
    }

    Ok(all_object_ids)
}
