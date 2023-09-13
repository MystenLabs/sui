// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod commands;
pub mod server;

mod context_data;
mod error;
mod extensions;
mod types;

use crate::types::query::Query;
use async_graphql::*;
use types::owner::ObjectOwner;

pub fn schema_sdl_export() -> String {
    let schema = Schema::build(Query, EmptyMutation, EmptySubscription)
        .register_output_type::<ObjectOwner>()
        .finish();
    schema.sdl()
}
