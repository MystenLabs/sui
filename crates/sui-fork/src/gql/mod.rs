// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod client;
mod queries;

pub use client::GraphQLClient;
pub(crate) use queries::address_owned_objects_query::AddressOwnedObject;
