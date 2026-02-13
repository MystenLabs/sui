// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use anyhow::{Context, Result, anyhow};
use clap::Args;
use cynic::QueryBuilder;
use tracing::info;

use sui_data_store::{ObjectKey, VersionQuery};
use sui_types::base_types::{ObjectID, SuiAddress};

use crate::store::ForkingStore;

#[derive(Args, Clone, Debug, Default)]
pub struct InitialAccounts {
    /// Addresses whose owned objects should be prefetched at startup.
    #[clap(long, value_delimiter = ',')]
    pub accounts: Vec<SuiAddress>,
}

impl InitialAccounts {
    pub async fn prefetch_owned_objects(
        &self,
        store: &ForkingStore,
        graphql_endpoint: &str,
        at_checkpoint: u64,
    ) -> Result<()> {
        if self.accounts.is_empty() {
            return Ok(());
        }

        let mut all_object_ids = BTreeSet::new();
        for owner in &self.accounts {
            info!("Prefetching owned objects for {}", owner);
            let owned_ids = fetch_owned_object_ids(graphql_endpoint, *owner).await?;
            info!("Found {} owned object IDs for {}", owned_ids.len(), owner);
            all_object_ids.extend(owned_ids);
        }

        if all_object_ids.is_empty() {
            info!("No owned objects found for startup accounts");
            return Ok(());
        }

        let object_keys: Vec<_> = all_object_ids
            .into_iter()
            .map(|object_id| ObjectKey {
                object_id,
                version_query: VersionQuery::AtCheckpoint(at_checkpoint),
            })
            .collect();

        let fetched_objects = store
            .get_objects(&object_keys)
            .context("Failed to prefetch owned objects from object store")?;

        let fetched = fetched_objects.iter().flatten().count();
        let requested = object_keys.len();
        info!(
            "Startup object prefetch completed at checkpoint {}: fetched {}/{} objects",
            at_checkpoint, fetched, requested
        );

        Ok(())
    }
}

#[cynic::schema("rpc")]
mod schema {}

#[derive(cynic::QueryVariables, Debug)]
struct AddressVariable {
    address: SuiAddressScalar,
    after: Option<String>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "Query", variables = "AddressVariable")]
struct AddressQuery {
    #[arguments(address: $address)]
    address: Option<ObjectsQuery>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "Address", variables = "AddressVariable")]
struct ObjectsQuery {
    #[arguments(after: $after)]
    objects: Option<MoveObjectConnection>,
}

#[derive(cynic::QueryFragment, Debug)]
struct MoveObjectConnection {
    edges: Vec<MoveObjectEdge>,
    page_info: PageInfo,
}

#[derive(cynic::QueryFragment, Debug)]
struct PageInfo {
    end_cursor: Option<String>,
    has_next_page: bool,
}

#[derive(cynic::QueryFragment, Debug)]
struct MoveObjectEdge {
    node: MoveObject,
}

#[derive(cynic::QueryFragment, Debug)]
struct MoveObject {
    address: SuiAddressScalar,
}

#[derive(cynic::Scalar, Debug, Clone)]
#[cynic(graphql_type = "SuiAddress")]
struct SuiAddressScalar(String);

/// Fetch all owned object IDs for an address with GraphQL pagination.
async fn fetch_owned_object_ids(
    graphql_endpoint: &str,
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
            .post(graphql_endpoint)
            .json(&query)
            .send()
            .await
            .context("Failed to send GraphQL request for owned objects")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());
            anyhow::bail!(
                "GraphQL owned objects request failed with status {}: {}",
                status,
                error_text
            );
        }

        let graphql_response: cynic::GraphQlResponse<AddressQuery> = response
            .json()
            .await
            .context("Failed to parse GraphQL response for owned objects")?;

        if let Some(errors) = &graphql_response.errors {
            let messages: Vec<String> = errors.iter().map(|e| e.message.clone()).collect();
            anyhow::bail!(
                "GraphQL errors while loading owned objects: {}",
                messages.join(", ")
            );
        }

        let data = graphql_response
            .data
            .ok_or_else(|| anyhow!("No data in GraphQL owned objects response"))?;
        let address_data = data
            .address
            .ok_or_else(|| anyhow!("Address not found in GraphQL owned objects response"))?;
        let objects = address_data
            .objects
            .ok_or_else(|| anyhow!("Owned objects connection missing in GraphQL response"))?;

        for edge in objects.edges {
            let object_id = ObjectID::from_hex_literal(&edge.node.address.0)
                .context("Failed to parse object ID from GraphQL owned objects response")?;
            all_object_ids.push(object_id);
        }

        has_next_page = objects.page_info.has_next_page;
        cursor = objects.page_info.end_cursor;
    }

    Ok(all_object_ids)
}
