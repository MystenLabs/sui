// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use graphql_client::GraphQLQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/schema.graphql",
    query_path = "src/queries/get_coins.graphql",
    response_derives = "Debug"
)]
pub struct GetCoins;

pub type SuiAddress = String;
pub type BigInt = String;
