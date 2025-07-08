// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::static_programmable_transactions::{
    linkage::resolved_linkage::ResolvedLinkage, loading::ast as L, spanned::Spanned,
};
use move_vm_types::values::VectorSpecialization;
use std::{cell::OnceCell, collections::BTreeMap, fmt};
use sui_types::base_types::ObjectID;

//**************************************************************************************************
// AST Nodes
//**************************************************************************************************

#[derive(Debug)]
pub struct Transaction {
    pub inputs: Inputs,
    pub commands: Commands,
}

pub type Inputs = Vec<(InputArg, InputType)>;

pub type Commands = Vec<(Command, ResultType)>;

pub type InputArg = L::InputArg;

pub type ObjectArg = L::ObjectArg;

pub type Type = L::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BytesUsage {
    /// The bytes are copied
    Copied,
    /// The bytes are immutably borrowed, which means they are created once and then dropped
    ByImmRef,
    /// The bytes are mutably borrowed, which "fixes" the type
    ByMutRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Information for a given constraint for input bytes
pub struct BytesConstraint {
    /// The command that first added this constraint
    pub command: u16,
    /// The argument in that command
    pub argument: u16,
    /// The type of usage for this bytes constraint
    pub usage: BytesUsage,
}

#[derive(Debug)]
pub enum InputType {
    /// A series of BCS bytes, and all types that this must satisfy
    Bytes(BTreeMap<Type, BytesConstraint>),
    /// A fixed type--the type is known and "fixed" at input
    Fixed(Type),
}
pub type ResultType = Vec<Type>;

pub type Command = Spanned<Command_>;

#[derive(Debug)]
pub enum Command_ {
    MoveCall(Box<MoveCall>),
    TransferObjects(Vec<Argument>, Argument),
    SplitCoins(/* Coin<T> */ Type, Argument, Vec<Argument>),
    MergeCoins(/* Coin<T> */ Type, Argument, Vec<Argument>),
    MakeMoveVec(/* T for vector<T> */ Type, Vec<Argument>),
    Publish(Vec<Vec<u8>>, Vec<ObjectID>, ResolvedLinkage),
    Upgrade(
        Vec<Vec<u8>>,
        Vec<ObjectID>,
        ObjectID,
        Argument,
        ResolvedLinkage,
    ),
}

pub type LoadedFunctionInstantiation = L::LoadedFunctionInstantiation;

pub type LoadedFunction = L::LoadedFunction;

#[derive(Debug)]
pub struct MoveCall {
    pub function: LoadedFunction,
    pub arguments: Vec<Argument>,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Location {
    TxContext,
    GasCoin,
    Input(u16),
    Result(u16, u16),
}

// Non borrowing usage of locations, moving or copying
#[derive(Clone, Debug)]
pub enum Usage {
    Move(Location),
    Copy {
        location: Location,
        /// Was this location borrowed at the time of copying?
        /// Initially empty and populated by `memory_safety`
        borrowed: OnceCell<bool>,
    },
}

pub type Argument = Spanned<Argument_>;
pub type Argument_ = (Argument__, Type);

#[derive(Clone, Debug)]
pub enum Argument__ {
    /// Move or copy a value
    Use(Usage),
    /// Borrow a value, i.e. `&x` or `&mut x`
    Borrow(/* mut */ bool, Location),
    /// Read a value from a reference, i.e. `*&x`
    Read(Usage),
    /// Freeze a mutable reference, making an `&t` from `&mut t`
    Freeze(Usage),
}

//**************************************************************************************************
// impl
//**************************************************************************************************

impl Usage {
    pub fn new_move(location: Location) -> Usage {
        Usage::Move(location)
    }

    pub fn new_copy(location: Location) -> Usage {
        Usage::Copy {
            location,
            borrowed: OnceCell::new(),
        }
    }

    pub fn location(&self) -> Location {
        match self {
            Usage::Move(location) => *location,
            Usage::Copy { location, .. } => *location,
        }
    }
}

impl Argument__ {
    pub fn new_move(location: Location) -> Self {
        Self::Use(Usage::new_move(location))
    }

    pub fn new_copy(location: Location) -> Self {
        Self::Use(Usage::new_copy(location))
    }

    pub fn location(&self) -> Location {
        match self {
            Self::Use(usage) | Self::Read(usage) => usage.location(),
            Self::Borrow(_, location) => *location,
            Self::Freeze(usage) => usage.location(),
        }
    }
}

//**************************************************************************************************
// traits
//**************************************************************************************************

impl TryFrom<Type> for VectorSpecialization {
    type Error = &'static str;

    fn try_from(value: Type) -> Result<Self, Self::Error> {
        Ok(match value {
            Type::U8 => VectorSpecialization::U8,
            Type::U16 => VectorSpecialization::U16,
            Type::U32 => VectorSpecialization::U32,
            Type::U64 => VectorSpecialization::U64,
            Type::U128 => VectorSpecialization::U128,
            Type::U256 => VectorSpecialization::U256,
            Type::Address => VectorSpecialization::Address,
            Type::Bool => VectorSpecialization::Bool,
            Type::Signer | Type::Vector(_) | Type::Datatype(_) => VectorSpecialization::Container,
            Type::Reference(_, _) => return Err("unexpected reference in vector specialization"),
        })
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Location::TxContext => write!(f, "TxContext"),
            Location::GasCoin => write!(f, "GasCoin"),
            Location::Input(idx) => write!(f, "Input({idx})"),
            Location::Result(result_idx, nested_idx) => {
                write!(f, "Result({result_idx}, {nested_idx})")
            }
        }
    }
}
