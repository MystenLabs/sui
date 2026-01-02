// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Interface;

use crate::api::scalars::id::Id;
use crate::api::types::address::Address;
use crate::api::types::checkpoint::Checkpoint;
use crate::api::types::epoch::Epoch;
use crate::api::types::move_package::MovePackage;
use crate::api::types::object::Object;
use crate::api::types::transaction::Transaction;

#[derive(Interface)]
#[graphql(name = "Node", field(name = "id", ty = "Id"))]
pub(crate) enum Node {
    Address(Box<Address>),
    Checkpoint(Box<Checkpoint>),
    Epoch(Box<Epoch>),
    MovePackage(Box<MovePackage>),
    Object(Box<Object>),
    Transaction(Box<Transaction>),
}
