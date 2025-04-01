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
    Bytes(/* all types that this must satisfy */ BTreeSet<Type>),
    Fixed(Type),
}
pub type ArgumentTypes = Vec<Type>;
pub type ResultType = Vec<Type>;

pub enum Command {
    MoveCall(Box<MoveCall>),
    TransferObjects(Vec<Argument>, Argument),
    SplitCoins(/* Coin<T> */ Type, Argument, Vec<Argument>),
    MergeCoins(/* Coin<T> */ Type, Argument, Vec<Argument>),
    MakeMoveVec(/* T for vector<T> */ Type, Vec<Argument>),
    Publish(Vec<Vec<u8>>, Vec<ObjectID>),
    Upgrade(Vec<Vec<u8>>, Vec<ObjectID>, ObjectID, Argument),
}

pub struct LoadedFunction {
    pub storage_id: ModuleId,
    pub runtime_id: ModuleId,
    pub name: Identifier,
    pub type_arguments: Vec<Type>,
    pub signature: LoadedFunctionInstantiation,
}

pub struct MoveCall {
    pub function: LoadedFunction,
    pub arguments: Vec<Argument>,
}

#[derive(Copy, Clone)]
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
