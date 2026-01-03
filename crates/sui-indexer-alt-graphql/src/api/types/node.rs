// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Interface;

use crate::api::scalars::id::Id;
use crate::api::types::address::Address;
use crate::api::types::checkpoint::Checkpoint;
use crate::api::types::dynamic_field::DynamicField;
use crate::api::types::epoch::Epoch;
use crate::api::types::move_object::MoveObject;
use crate::api::types::move_package::MovePackage;
use crate::api::types::object::Object;
use crate::api::types::transaction::Transaction;

/// An interface implemented by types that can be uniquely identified by a globally unique `ID`, following the GraphQL Global Object Identification specification.
#[derive(Interface)]
#[graphql(
    name = "Node",
    field(
        name = "id",
        ty = "Id",
        desc = "The node's globally unique identifier, which can be passed to `Query.node` to refetch it."
    )
)]
pub(crate) enum Node {
    Address(Box<Address>),
    Checkpoint(Box<Checkpoint>),
    DynamicField(Box<DynamicField>),
    Epoch(Box<Epoch>),
    MoveObject(Box<MoveObject>),
    MovePackage(Box<MovePackage>),
    Object(Box<Object>),
    Transaction(Box<Transaction>),
}
