// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{identifier::Identifier, language_storage::ModuleId};
use move_vm_runtime::session::LoadedFunctionInstantiation;
use std::collections::BTreeSet;
use sui_types::{base_types::ObjectID, transaction::CallArg};

pub struct Transaction {
    pub inputs: Inputs,
    pub commands: Commands,
}

pub type Inputs = Vec<(CallArg, InputType)>;

pub type Commands = Vec<(Command, ResultType)>;

pub type Type = move_vm_types::loaded_data::runtime_types::Type;

pub enum InputType {
    BCSBytes(/* all types that this must satisfy */ BTreeSet<Type>),
    // receiving is essentially `forall a. Receiving<a>`
    Receiving,
    Fixed(Type),
}
pub type ArgumentTypes = Vec<Type>;
pub type ResultType = Vec<Type>;

pub enum Command {
    MoveCall(Box<MoveCall>),
    TransferObjects(Vec<Argument>, Argument),
    SplitCoins(Type, Argument, Vec<Argument>),
    MergeCoins(Type, Argument, Vec<Argument>),
    MakeMoveVec(Type, Vec<Argument>),
    Publish(Vec<Vec<u8>>, Vec<ObjectID>),
    Upgrade(Vec<Vec<u8>>, Vec<ObjectID>, ObjectID, Argument),
}

pub struct MoveCall {
    pub module: ModuleId,
    pub function: Identifier,
    pub type_arguments: Vec<Type>,
    pub arguments: Vec<Argument>,
    pub signature: LoadedFunctionInstantiation,
}

pub enum Location {
    GasCoin,
    Input(u16),
    Result(u16, u16),
}

pub enum Argument {
    Move(Location),
    Copy(Location),
    Borrow(/* mut */ bool, Location),
}
