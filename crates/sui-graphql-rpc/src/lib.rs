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
use fastcrypto::encoding::{Base64, Encoding};
use mutation::Mutation;
use serde::de::DeserializeOwned;
use types::owner::IOwner;

use crate::types::query::Query;

pub fn schema_sdl_export() -> String {
    let schema = Schema::build(Query, Mutation, EmptySubscription)
        .register_output_type::<IOwner>()
        .finish();
    schema.sdl()
}

pub fn deserialize_tx_data<T>(tx_bytes: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    bcs::from_bytes(
        &Base64::decode(tx_bytes)
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
