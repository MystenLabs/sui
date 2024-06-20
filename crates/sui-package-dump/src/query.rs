// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use cynic::Operation;
use cynic::QueryBuilder;

#[cynic::schema("sui")]
mod schema {}

/// Query types related to GraphQL service limits.
pub(crate) mod limits {
    use super::*;

    pub(crate) fn build() -> Operation<Query> {
        Query::build(())
    }

    #[derive(cynic::QueryFragment, Debug)]
    pub(crate) struct Query {
        pub(crate) service_config: ServiceConfig,
    }

    #[derive(cynic::QueryFragment, Debug)]
    pub(crate) struct ServiceConfig {
        pub(crate) max_page_size: i32,
    }
}

/// Query types related to fetching packages.
pub(crate) mod packages {
    use super::*;

    pub(crate) fn build(
        first: i32,
        after: Option<String>,
        after_checkpoint: Option<i32>,
        before_checkpoint: Option<i32>,
    ) -> Operation<Query, Vars> {
        Query::build(Vars {
            first,
            after,
            after_checkpoint,
            before_checkpoint,
        })
    }

    #[derive(cynic::QueryVariables, Debug)]
    pub(crate) struct Vars {
        pub(crate) first: i32,
        pub(crate) after: Option<String>,
        pub(crate) after_checkpoint: Option<i32>,
        pub(crate) before_checkpoint: Option<i32>,
    }

    #[derive(cynic::QueryFragment, Debug)]
    #[cynic(variables = "Vars")]
    pub(crate) struct Query {
        pub(crate) checkpoint: Option<Checkpoint>,
        #[arguments(
            first: $first,
            after: $after,
            afterCheckpoint: $after_checkpoint,
            beforeCheckpoint: $before_checkpoint
        )]
        pub(crate) packages: MovePackageConnection,
    }

    #[derive(cynic::QueryFragment, Debug)]
    pub(crate) struct Checkpoint {
        pub(crate) sequence_number: i32,
    }

    #[derive(cynic::Scalar, Debug)]
    pub(crate) struct SuiAddress(pub String);

    #[derive(cynic::Scalar, Debug)]
    pub(crate) struct Base64(pub String);

    #[derive(cynic::QueryFragment, Debug)]
    pub(crate) struct MovePackageConnection {
        pub(crate) page_info: PageInfo,
        pub(crate) nodes: Vec<MovePackage>,
    }

    #[derive(cynic::QueryFragment, Debug)]
    pub(crate) struct PageInfo {
        pub(crate) has_next_page: bool,
        pub(crate) end_cursor: Option<String>,
    }

    #[derive(cynic::QueryFragment, Debug)]
    pub(crate) struct MovePackage {
        pub(crate) address: SuiAddress,
        pub(crate) bcs: Option<Base64>,
    }
}
