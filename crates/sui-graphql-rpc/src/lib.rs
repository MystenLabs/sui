// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod commands;
pub mod config;
pub mod server;

mod context_data;
mod error;
mod extensions;
mod functional_group;
mod types;

use async_graphql::*;
use types::owner::ObjectOwner;

use crate::types::query::Query;

pub fn schema_sdl_export() -> String {
    let schema = Schema::build(Query, EmptyMutation, EmptySubscription)
        .register_output_type::<ObjectOwner>()
        .finish();
    schema.sdl()
}
