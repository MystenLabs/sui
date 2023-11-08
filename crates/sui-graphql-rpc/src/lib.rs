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
pub mod test_infra;
mod types;
pub mod utils;

use async_graphql::*;
use types::owner::ObjectOwner;

use crate::types::query::Query;

pub fn schema_sdl_export() -> String {
    let schema = Schema::build(Query, EmptyMutation, EmptySubscription)
        .register_output_type::<ObjectOwner>()
        .finish();
    schema.sdl()
}
