// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::static_programmable_transactions::{
    linkage::resolved_linkage::ResolvedLinkage, loading::ast as L, spanned::Spanned,
};
use indexmap::IndexSet;
use move_vm_types::values::VectorSpecialization;
use std::cell::OnceCell;
use sui_types::base_types::{ObjectID, ObjectRef};

//**************************************************************************************************
// AST Nodes
//**************************************************************************************************

#[derive(Debug)]
pub struct Transaction {
    /// Gathered BCS bytes from Pure inputs
    pub bytes: IndexSet<Vec<u8>>,
    // All input objects
    pub objects: Vec<ObjectInput>,
    /// All pure inputs
    pub pure: Vec<PureInput>,
    /// All receiving inputs
    pub receiving: Vec<ReceivingInput>,
    pub commands: Commands,
}

/// The original index into the `input` vector of the transaction, before the inputs were split
/// into their respective categories (objects, pure, or receiving).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InputIndex(pub u16);

#[derive(Debug)]
pub struct ObjectInput {
    pub original_input_index: InputIndex,
    pub arg: ObjectArg,
    pub ty: Type,
}

pub type ByteIndex = usize;

#[derive(Debug)]
pub struct PureInput {
    pub original_input_index: InputIndex,
    // A index into `byte` table of BCS bytes
    pub byte_index: ByteIndex,
    // the type that the BCS bytes will be deserialized into
    pub ty: Type,
    // Information about where this constraint came from
    pub constraint: BytesConstraint,
}

#[derive(Debug)]
pub struct ReceivingInput {
    pub original_input_index: InputIndex,
    pub object_ref: ObjectRef,
    pub ty: Type,
    // Information about where this constraint came from
    pub constraint: BytesConstraint,
}

pub type Commands = Vec<(Command, ResultType)>;

pub type ObjectArg = L::ObjectArg;

pub type Type = L::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Information for a given constraint for input bytes
pub struct BytesConstraint {
    /// The command that first added this constraint
    pub command: u16,
    /// The argument in that command
    pub argument: u16,
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

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Location {
    TxContext,
    GasCoin,
    ObjectInput(u16),
    PureInput(u16),
    ReceivingInput(u16),
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
