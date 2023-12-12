// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod commands;
pub mod config;
pub mod server;

pub(crate) mod functional_group;

pub mod client;
pub mod context_data;
mod error;
pub mod examples;
pub mod extensions;
mod metrics;
mod mutation;
pub mod test_infra;
mod types;
pub mod utils;

use async_graphql::*;
use mutation::Mutation;
use types::owner::ObjectOwner;

use crate::types::query::Query;

pub fn schema_sdl_export() -> String {
    let schema = Schema::build(Query, Mutation, EmptySubscription)
        .register_output_type::<ObjectOwner>()
        .finish();
    schema.sdl()
}
