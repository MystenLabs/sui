// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use sui_graphql_rpc_client as client;
pub mod commands;
pub mod config;
pub mod context_data;
pub(crate) mod data;
mod error;
pub mod examples;
pub mod extensions;
pub(crate) mod functional_group;
mod metrics;
mod mutation;
pub mod server;
pub mod test_infra;
mod types;

use async_graphql::*;
use mutation::Mutation;
use types::owner::IOwner;

use crate::types::query::Query;

pub fn schema_sdl_export() -> String {
    let schema = Schema::build(Query, Mutation, EmptySubscription)
        .register_output_type::<IOwner>()
        .finish();
    schema.sdl()
}
