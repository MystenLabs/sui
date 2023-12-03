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
use fastcrypto::encoding::{Base64, Encoding};
use mutation::Mutation;
use serde::Deserialize;
use types::owner::ObjectOwner;

use crate::types::query::Query;

pub fn schema_sdl_export() -> String {
    let schema = Schema::build(Query, Mutation, EmptySubscription)
        .register_output_type::<ObjectOwner>()
        .finish();
    schema.sdl()
}

pub fn deserialize_tx_data<'a, T>(tx_bytes: String) -> Result<T>
where
    T: Deserialize<'a>,
{
    bcs::from_bytes(
        &Base64::decode(&tx_bytes)
            .map_err(|e| {
                error::Error::Client(format!(
                    "Unable to deserialize transaction bytes from Base64: {e}"
                ))
            })
            .extend()?,
    )
    .map_err(|e| {
        error::Error::Client(format!(
            "Unable to deserialize transaction bytes as BCS: {e}"
        ))
    })
    .extend()
}
