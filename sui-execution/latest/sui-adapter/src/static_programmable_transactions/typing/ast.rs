// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::static_programmable_transactions::loading::ast as L;
use std::collections::BTreeMap;
use sui_types::{base_types::ObjectID, transaction::CallArg};

pub struct Transaction {
    pub inputs: Inputs,
    pub commands: Commands,
}

pub type Inputs = Vec<(CallArg, InputType)>;

pub type Commands = Vec<(Command, ResultType)>;

pub type Type = L::Type;

pub enum InputType {
    Bytes(
        /* all types that this must satisfy */
        BTreeMap<Type, /* command, arg idx */ (u16, u16)>,
    ),
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

pub type LoadedFunction = L::LoadedFunction;

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
    Read(Location),
}
