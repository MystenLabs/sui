// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Interface;

use crate::api::scalars::id::Id;
use crate::api::types::address::Address;

#[derive(Interface)]
#[graphql(name = "Node", field(name = "id", ty = "Id"))]
pub(crate) enum Node {
    Address(Box<Address>),
}
