// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::ReplayError;
use base64ct::Encoding;
use chrono::DateTime as ChronoDateTime;
use cynic::QueryBuilder;
use sui_graphql_client::{
    error::Error,
    query_types::{schema, Base64, BigInt, DateTime, PageInfo},
    Client, Page, PaginationFilter,
};
use sui_sdk_types::{Address, EpochId, Object, TransactionEffects, Version};
use sui_types::base_types::ObjectID;

// "Output" types
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpochData {
    pub epoch_id: u64,
    pub start_timestamp: u64,
    pub rgp: u64,
    pub protocol_version: u64,
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
    pub bcs: Option<Base64>,
    pub previous_transaction_block: Option<TransactionBlock>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema = "rpc", graphql_type = "MovePackageConnection")]
pub struct MovePackageConnection {
    pub nodes: Vec<MovePackage>,
    pub page_info: PageInfo,
}

/// Return a (`Option<MovePackage>`, version, `Option<EpochId>`) tuple for each version of the
/// package at address. The `Option<EpochId>` is the epoch in which this package was published.
pub async fn package_versions_for_replay(
    client: &Client,
    address: Address,
    pagination_filter: PaginationFilter,
    after_version: Option<u64>,
    before_version: Option<u64>,
) -> Result<Page<(Option<Object>, Version, Option<EpochId>)>, Error> {
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
            .map(|p| (p.bcs, p.version, p.previous_transaction_block))
            .collect::<Vec<_>>();

        let mut output = vec![];

        for (bcs, version, previous_transaction_block) in data {
            let bcs = bcs
                .as_ref()
                .map(|b| base64ct::Base64::decode_vec(b.0.as_str()))
                .transpose()?;
            let package = bcs.map(|b| bcs::from_bytes::<Object>(&b)).transpose()?;

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
            .ok_or(ReplayError::MissingDataForEpoch {
                data: "RGP".to_string(),
                epoch: epoch_id,
            })?
            .0
            .parse::<u64>()
            .map_err(ReplayError::from)?;

        let protocol_version = epoch.protocol_configs.unwrap().protocol_version;

        Ok(EpochData {
            epoch_id,
            start_timestamp,
            rgp,
            protocol_version,
        })
    }
}

// ===========================================================================
// Dynamic Fields
// ===========================================================================

#[derive(cynic::QueryFragment)]
#[cynic(schema = "rpc", graphql_type = "Query", variables = "DynamicFieldArgs")]
pub struct DynamicFieldAtVersionQuery {
    #[arguments(address: $address, rootVersion: $root_version)]
    pub owner: Option<OwnerData>,
}

#[derive(cynic::QueryVariables, Debug)]
pub struct DynamicFieldArgs {
    pub address: Address,
    pub root_version: Option<u64>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema = "rpc", graphql_type = "Owner")]
pub struct OwnerData {
    pub as_object: Option<GqlObject>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema = "rpc", graphql_type = "Object")]
pub struct GqlObject {
    pub bcs: Option<Base64>,
}

/// Return the dynamic field given a parent version.
/// Retrieve the object with at most (and closest to) the version of the parent.
pub async fn dynamic_field_at_version(
    client: &Client,
    obj: ObjectID,
    root_version: u64,
) -> Result<Option<Object>, ReplayError> {
    let operation = DynamicFieldAtVersionQuery::build(DynamicFieldArgs {
        address: Address::from_bytes(obj.into_bytes()).map_err(|err| {
            ReplayError::GenericError {
                err: format!("{:?}", err),
            }
        })?,
        root_version: Some(root_version),
    });

    let response = client
        .run_query(&operation)
        .await
        .map_err(|err| ReplayError::GenericError {
            err: format!("{:?}", err),
        })?;

    if response.errors.is_some() {
        return Err(ReplayError::GenericError {
            err: "Error in dynamic_field_at_version".to_string(),
        });
    }

    if let Some(dyn_field) = response.data {
        let bcs = dyn_field
            .owner
            .and_then(|obj| obj.as_object)
            .and_then(|obj| obj.bcs);
        if let Some(bcs) = bcs {
            let bcs = base64ct::Base64::decode_vec(bcs.0.as_str()).map_err(|err| {
                ReplayError::GenericError {
                    err: format!("{:?}", err),
                }
            })?;
            let obj = bcs::from_bytes::<Object>(&bcs).map_err(|err| ReplayError::GenericError {
                err: format!("{:?}", err),
            })?;
            return Ok(Some(obj));
        }
    }
    Ok(None)
}
