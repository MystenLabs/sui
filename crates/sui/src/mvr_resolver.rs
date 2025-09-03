// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Error;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use sui_protocol_config::Chain;
use sui_sdk::apis::ReadApi;
use sui_types::{base_types::ObjectID, digests::ChainIdentifier};

const MVR_RESOLVER_MAINNET_URL: &str = "https://mainnet.mvr.mystenlabs.com";
const MVR_RESOLVER_TESTNET_URL: &str = "https://testnet.mvr.mystenlabs.com";

#[derive(Debug, Serialize)]
pub struct MvrResolver {
    pub names: BTreeSet<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResolvedNames {
    pub resolution: BTreeMap<String, PackageId>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PackageId {
    pub package_id: ObjectID,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NamesRequest {
    pub names: BTreeSet<String>,
}

impl MvrResolver {
    /// Call this function before calling resolve_names to avoid making a call to the resolver if
    /// not needed.
    pub fn should_resolve(&self) -> bool {
        !self.names.is_empty()
    }

    /// Given a set of MVR names, resolve them to their corresponding package IDs. Note that this
    /// API will error if the resolved list length does not match with the given input.
    pub async fn resolve_names(&self, read_api: &ReadApi) -> Result<ResolvedNames, Error> {
        if self.names.is_empty() {
            return Ok(ResolvedNames {
                resolution: BTreeMap::new(),
            });
        }

        let request = reqwest::Client::new();
        let (url, chain) = mvr_req_url(read_api).await?;
        let json_body = json!(NamesRequest {
            names: self.names.clone()
        });
        let response = request
            .post(format!("{url}/v1/resolution/bulk"))
            .header("Content-Type", "application/json")
            .json(&json_body)
            .send()
            .await?;

        let resolved_addresses: ResolvedNames = response.json().await?;

        anyhow::ensure!(
            resolved_addresses.resolution.len() == self.names.len(),
            "Could not find package id for {} for {chain} enviroment",
            self.names
                .difference(
                    &resolved_addresses
                        .resolution
                        .keys()
                        .cloned()
                        .collect::<BTreeSet<_>>()
                )
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );

        Ok(resolved_addresses)
    }
}

/// Based on the chain id of the current set environment, return the correct MVR URL to use for
/// resolution.
async fn mvr_req_url(read_api: &ReadApi) -> Result<(&'static str, &'static str), Error> {
    let chain_id = read_api.get_chain_identifier().await?;
    let chain = ChainIdentifier::from_chain_short_id(&chain_id);

    if let Some(chain) = chain {
        let chain = chain.chain();
        match chain {
            Chain::Mainnet => Ok((MVR_RESOLVER_MAINNET_URL, "mainnet")),
            Chain::Testnet => Ok((MVR_RESOLVER_TESTNET_URL, "testnet")),
            Chain::Unknown => {
                anyhow::bail!("Unsupported chain identifier: {:?}", chain);
            }
        }
    } else {
        anyhow::bail!(
            "Unsupported chain: {chain_id}. Only mainnet/testnet are supported for \
            MVR resolution",
        )
    }
}
