// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::static_programmable_transactions::loading::ast as L;
use std::{collections::BTreeMap, fmt};
use sui_types::base_types::ObjectID;

pub struct Transaction {
    pub inputs: Inputs,
    pub commands: Commands,
}

pub type Inputs = Vec<(InputArg, InputType)>;

pub type Commands = Vec<(Command, ResultType)>;

pub type InputArg = L::InputArg;

pub type ObjectArg = L::ObjectArg;

pub type Type = L::Type;

pub enum InputType {
    Bytes(
        /* all types that this must satisfy */
        BTreeMap<Type, /* command, arg idx */ (u16, u16)>,
    ),
    Fixed(Type),
}
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

pub type LoadedFunctionInstantiation = L::LoadedFunctionInstantiation;

pub type LoadedFunction = L::LoadedFunction;

pub struct MoveCall {
    pub function: LoadedFunction,
    pub arguments: Vec<Argument>,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Location {
    GasCoin,
    Input(u16),
    Result(u16, u16),
}

pub type Argument = (Argument_, Type);

#[derive(Copy, Clone)]
pub enum Argument_ {
    Move(Location),
    Copy(Location),
    Borrow(/* mut */ bool, Location),
    Read(Location),
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Location::GasCoin => write!(f, "GasCoin"),
            Location::Input(idx) => write!(f, "Input({idx})"),
            Location::Result(result_idx, nested_idx) => {
                write!(f, "Result({result_idx}, {nested_idx})")
            }
        }
    }
}
