// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use types::owner::ObjectOwner;

pub mod commands;
pub mod server;
pub mod types;

use crate::types::query::Query;

pub fn schema_sdl_export() -> String {
    let schema = Schema::build(Query, EmptyMutation, EmptySubscription)
        .register_output_type::<ObjectOwner>()
        .finish();
    schema.sdl()
}
