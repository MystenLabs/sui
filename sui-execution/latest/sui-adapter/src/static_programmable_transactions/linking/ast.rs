// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::static_programmable_transactions::{
    linkage::resolved_linkage::{ExecutableLinkage, ResolvedLinkage},
    loading::ast::PackagePayload,
};
use move_binary_format::file_format::Visibility;
use sui_types::{
    Identifier, TypeTag,
    base_types::ObjectID,
    transaction::{Argument, CallArg},
};

/// Names are defining-ID based, but type instantiations have not yet been validated by the VM.
#[derive(Debug)]
pub struct Transaction {
    pub inputs: Vec<CallArg>,
    pub commands: Vec<Command>,
    pub unified_linkage: ExecutableLinkage,
}

#[derive(Debug)]
pub struct MoveCall {
    pub package: ObjectID,
    pub module: Identifier,
    pub function: Identifier,
    pub visibility: Visibility,
    pub type_arguments: Vec<TypeTag>,
    pub arguments: Vec<Argument>,
}

#[derive(Debug)]
pub enum Command {
    MoveCall(Box<MoveCall>),
    TransferObjects(Vec<Argument>, Argument),
    SplitCoins(Argument, Vec<Argument>),
    MergeCoins(Argument, Vec<Argument>),
    MakeMoveVec(Option<TypeTag>, Vec<Argument>),
    Publish(PackagePayload, Vec<ObjectID>, ResolvedLinkage),
    Upgrade(
        PackagePayload,
        Vec<ObjectID>,
        ObjectID,
        Argument,
        ResolvedLinkage,
    ),
}
