// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_sdk_types::{
    Address, EpochId, MovePackage as MovePackageSdk, TransactionDigest as SdkTransactionDigest,
    TransactionEffects, Version,
};

use crate::errors::ReplayError;
use base64ct::Encoding;
use chrono::DateTime as ChronoDateTime;
use cynic::QueryBuilder;
use sui_graphql_client::{
    error::Error,
    query_types::{schema, Base64, BigInt, DateTime, PageInfo},
    Client, Page, PaginationFilter,
};
use sui_types::digests::TransactionDigest;

// "Output" types
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpochData {
    pub epoch_id: u64,
    pub start_timestamp: u64,
    pub rgp: u64,
    pub protocol_version: u64,
    pub last_tx_digest: TransactionDigest,
}

// GQL Queries

// ===========================================================================
// PackagesVersions
// ===========================================================================

#[derive(cynic::QueryFragment)]
#[cynic(
    schema = "rpc",
    graphql_type = "Query",
    variables = "PackageVersionsArgs"
)]
pub struct PackageVersionsWithEpochDataQuery {
    #[arguments(address: $address, after: $after, first: $first, last: $last, before: $before, filter:$filter)]
    pub package_versions: MovePackageConnection,
}

#[derive(cynic::QueryVariables, Debug)]
pub struct PackageVersionsArgs<'a> {
    pub address: Address,
    pub after: Option<&'a str>,
    pub first: Option<i32>,
    pub last: Option<i32>,
    pub before: Option<&'a str>,
    pub filter: Option<MovePackageVersionFilter>,
}

#[derive(cynic::InputObject, Debug)]
#[cynic(schema = "rpc", graphql_type = "MovePackageVersionFilter")]
pub struct MovePackageVersionFilter {
    pub after_version: Option<u64>,
    pub before_version: Option<u64>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema = "rpc", graphql_type = "MovePackage")]
pub struct MovePackage {
    pub version: u64,
    pub package_bcs: Option<Base64>,
    pub previous_transaction_block: Option<TransactionBlock>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema = "rpc", graphql_type = "MovePackageConnection")]
pub struct MovePackageConnection {
    pub nodes: Vec<MovePackage>,
    pub page_info: PageInfo,
}

/// Return a (Option<MovePackage>, version, Option<EpochId>) tuple for each version of the
/// package at address. The Option<EpochId> is the epoch in which this package was published.
pub async fn package_versions_for_replay(
    client: &Client,
    address: Address,
    pagination_filter: PaginationFilter,
    after_version: Option<u64>,
    before_version: Option<u64>,
) -> Result<Page<(Option<MovePackageSdk>, Version, Option<EpochId>)>, Error> {
    let (after, before, first, last) = client.pagination_filter(pagination_filter).await;
    let operation = PackageVersionsWithEpochDataQuery::build(PackageVersionsArgs {
        address,
        after: after.as_deref(),
        before: before.as_deref(),
        first,
        last,
        filter: Some(MovePackageVersionFilter {
            after_version,
            before_version,
        }),
    });

    let response = client.run_query(&operation).await?;

    if let Some(errors) = response.errors {
        return Err(Error::graphql_error(errors));
    }

    if let Some(packages) = response.data {
        let pc = packages.package_versions;
        let page_info = pc.page_info;
        let data = pc
            .nodes
            .into_iter()
            .map(|p| (p.package_bcs, p.version, p.previous_transaction_block))
            .collect::<Vec<_>>();

        let mut output = vec![];

        for (bcs, version, previous_transaction_block) in data {
            let bcs = bcs
                .as_ref()
                .map(|b| base64ct::Base64::decode_vec(b.0.as_str()))
                .transpose()?;
            let package = bcs
                .map(|b| bcs::from_bytes::<MovePackageSdk>(&b))
                .transpose()?;

            let effects = previous_transaction_block.and_then(|x| x.effects);
            let effects = effects.and_then(|x| x.bcs);
            let bcs = effects
                .map(|x| base64ct::Base64::decode_vec(x.0.as_str()))
                .transpose()?;
            let effects = bcs
                .map(|b| bcs::from_bytes::<TransactionEffects>(&b))
                .transpose()?;
            let epoch = effects.map(|e| match e {
                TransactionEffects::V1(e) => e.epoch,
                TransactionEffects::V2(e) => e.epoch,
            });

            output.push((package, version, epoch));
        }

        Ok(Page::new(page_info, output))
    } else {
        Ok(Page::new_empty())
    }
}

// ===========================================================================
// Epochs
// ===========================================================================

#[derive(cynic::QueryFragment)]
#[cynic(schema = "rpc", graphql_type = "Query", variables = "EpochsArgs")]
pub struct EpochsQuery {
    #[arguments(after: $after, first: $first, last: $last, before: $before)]
    pub epochs: EpochConnection,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct EpochConnection {
    pub nodes: Vec<Epoch>,
    pub page_info: PageInfo,
}

#[derive(cynic::QueryVariables, Debug)]
pub struct EpochsArgs<'a> {
    pub after: Option<&'a str>,
    pub first: Option<i32>,
    pub last: Option<i32>,
    pub before: Option<&'a str>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema = "rpc", graphql_type = "Epoch")]
pub struct Epoch {
    pub epoch_id: EpochId,
    pub start_timestamp: DateTime,
    pub end_timestamp: Option<DateTime>,
    pub reference_gas_price: Option<BigInt>,
    pub protocol_configs: Option<ProtocolConfigs>,
    // we are only interested in the last tx, which should be the
    // EndOfEpochTransaction type
    #[arguments(last: 1)]
    pub transaction_blocks: TransactionBlockConnection,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema = "rpc", graphql_type = "TransactionBlockConnection")]
pub struct TransactionBlockConnection {
    pub nodes: Vec<TransactionBlock>,
    pub page_info: PageInfo,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema = "rpc", graphql_type = "TransactionBlock")]
pub struct TransactionBlock {
    pub bcs: Option<Base64>,
    pub effects: Option<TransactionBlockEffects>,
    pub digest: Option<String>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema = "rpc", graphql_type = "TransactionBlockEffects")]
pub struct TransactionBlockEffects {
    pub bcs: Option<Base64>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema = "rpc", graphql_type = "ProtocolConfigs")]
pub struct ProtocolConfigs {
    pub protocol_version: u64,
}

pub async fn epochs(
    client: &Client,
    pagination_filter: PaginationFilter,
) -> Result<Page<Epoch>, Error> {
    let (after, before, first, last) = client.pagination_filter(pagination_filter).await;
    let operation = EpochsQuery::build(EpochsArgs {
        after: after.as_deref(),
        before: before.as_deref(),
        first,
        last,
    });

    let response = client.run_query(&operation).await?;

    if let Some(errors) = response.errors {
        return Err(Error::graphql_error(errors));
    }

    if let Some(epochs) = response.data {
        Ok(Page::new(epochs.epochs.page_info, epochs.epochs.nodes))
    } else {
        Ok(Page::new_empty())
    }
}

impl TryFrom<Epoch> for EpochData {
    type Error = ReplayError;

    fn try_from(epoch: Epoch) -> Result<Self, Self::Error> {
        let epoch_id = epoch.epoch_id;
        let start_timestamp = ChronoDateTime::parse_from_rfc3339(&epoch.start_timestamp.0)
            .map_err(|e| ReplayError::GenericError {
                err: format!("{:?}", e),
            })?
            .timestamp_millis()
            .try_into()
            .map_err(|_| ReplayError::DateTimeConversionError)?;

        let rgp = epoch
            .reference_gas_price
            .ok_or_else(|| ReplayError::MissingRGPForEpoch { epoch: epoch_id })?
            .0
            .parse::<u64>()
            .map_err(|e| ReplayError::from(e))?;

        let protocol_version = epoch.protocol_configs.unwrap().protocol_version;
        if epoch.transaction_blocks.nodes.is_empty() {
            return Err(ReplayError::GenericError {
                err: format!("Epoch {epoch_id} has no transaction blocks"),
            });
        }
        let last_tx_digest = SdkTransactionDigest::from_base58(
            epoch.transaction_blocks.nodes[0].digest.as_ref().unwrap(),
        )
        .map_err(|e| ReplayError::GenericError {
            err: format!("No transaction digest for epoch {epoch_id}. {:?}", e),
        })?;
        let last_tx_digest =
            TransactionDigest::try_from(last_tx_digest.as_bytes()).map_err(|e| {
                ReplayError::FailedToParseDigest {
                    digest: last_tx_digest.to_string(),
                    err: format!("{:?}", e),
                }
            })?;

        Ok(EpochData {
            epoch_id,
            start_timestamp,
            rgp,
            protocol_version,
            last_tx_digest,
        })
    }
}

