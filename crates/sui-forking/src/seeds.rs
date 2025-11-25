// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use cynic::QueryBuilder;
use fastcrypto::encoding::{Base64 as CryptoBase64, Encoding};
use std::{collections::BTreeSet, path::PathBuf};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    object::Object,
};

#[derive(Parser, Clone, Debug)]
pub struct InitialSeeds {
    /// Specific accounts to track ownership for
    #[clap(long, value_delimiter = ',')]
    pub accounts: Vec<SuiAddress>,
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
    pub cursor: String,
    pub node: MoveObject,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct MoveObject {
    pub object_bcs: Option<Base64>,
}

#[derive(cynic::Scalar, Debug, Clone)]
#[cynic(graphql_type = "Base64")]
pub struct Base64(pub String);

#[derive(cynic::Scalar, Debug, Clone)]
#[cynic(graphql_type = "SuiAddress")]
pub struct SuiAddressScalar(pub String);

/// Fetch all owned objects for a given address using GraphQL with pagination
pub async fn fetch_owned_objects(
    graphql_endpoint: &Network,
    address: SuiAddress,
) -> Result<Vec<Object>> {
    let client = reqwest::Client::new();
    let mut all_objects = Vec::new();
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
            if let Some(bcs_data) = edge.node.object_bcs {
                let bytes = CryptoBase64::decode(&bcs_data.0)
                    .context("Failed to decode base64 object data")?;
                let obj: Object =
                    bcs::from_bytes(&bytes).context("Failed to deserialize object from BCS")?;
                all_objects.push(obj);
            }
        }

        has_next_page = objects_connection.page_info.has_next_page;
        cursor = objects_connection.page_info.end_cursor;
    }

    Ok(all_objects)
}
