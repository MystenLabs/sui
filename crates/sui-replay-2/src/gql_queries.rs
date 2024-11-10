// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_sdk_types::{Address, EpochId, MovePackage as SdkMovePackage, TransactionEffects, Version};

use base64ct::Encoding;
use cynic::QueryBuilder;
use sui_graphql_client::{
    error::Error,
    query_types::{schema, Base64, PageInfo, TransactionBlock},
    Client, Page, PaginationFilter,
};

// ===========================================================================
// PackagesVersions
// ===========================================================================

#[derive(cynic::QueryFragment, Debug)]
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
) -> Result<Page<(Option<SdkMovePackage>, Version, Option<EpochId>)>, Error> {
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
                .map(|b| bcs::from_bytes::<SdkMovePackage>(&b))
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
