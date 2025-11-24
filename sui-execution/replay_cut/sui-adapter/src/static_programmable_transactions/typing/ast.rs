// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::static_programmable_transactions::{
    linkage::resolved_linkage::ResolvedLinkage, loading::ast as L, spanned::Spanned,
};
use indexmap::IndexSet;
use move_core_types::{account_address::AccountAddress, u256::U256};
use move_vm_types::values::VectorSpecialization;
use std::{cell::OnceCell, vec};
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
    /// All Withdrawal inputs
    pub withdrawals: Vec<WithdrawalInput>,
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

#[derive(Debug)]
pub struct WithdrawalInput {
    pub original_input_index: InputIndex,
    /// The full type `sui::funds_accumulator::Withdrawal<T>`
    pub ty: Type,
    pub owner: AccountAddress,
    /// This amount is verified to be <= the max for the type described by the `T` in `ty`
    pub amount: U256,
}

pub type Commands = Vec<Command>;

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
pub struct Command_ {
    /// The command
    pub command: Command__,
    /// The type of the return values of the command
    pub result_type: ResultType,
    /// Markers to drop unused results from the command. These are inferred based on any usage
    /// of the given result `Result(i,j)` after this command. This is leveraged by the borrow
    /// checker to remove unused references to allow potentially reuse of parent references.
    /// The value at result `j` is unused and can be dropped if `drop_value[j]` is true.
    pub drop_values: Vec</* drop value */ bool>,
    /// The set of object shared object IDs that are consumed by this command.
    /// After this command is executed, these objects must be either reshared or deleted.
    pub consumed_shared_objects: Vec<ObjectID>,
}

#[derive(Debug)]
pub enum Command__ {
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
    WithdrawalInput(u16),
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

impl Transaction {
    pub fn types(&self) -> impl Iterator<Item = &Type> {
        let pure_types = self.pure.iter().map(|p| &p.ty);
        let object_types = self.objects.iter().map(|o| &o.ty);
        let receiving_types = self.receiving.iter().map(|r| &r.ty);
        let command_types = self.commands.iter().flat_map(command_types);
        pure_types
            .chain(object_types)
            .chain(receiving_types)
            .chain(command_types)
    }
}

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

impl Command__ {
    pub fn arguments(&self) -> Vec<&Argument> {
        match self {
            Command__::MoveCall(mc) => mc.arguments.iter().collect(),
            Command__::TransferObjects(objs, addr) => {
                objs.iter().chain(std::iter::once(addr)).collect()
            }
            Command__::SplitCoins(_, coin, amounts) => {
                std::iter::once(coin).chain(amounts).collect()
            }
            Command__::MergeCoins(_, target, sources) => {
                std::iter::once(target).chain(sources).collect()
            }
            Command__::MakeMoveVec(_, elems) => elems.iter().collect(),
            Command__::Publish(_, _, _) => vec![],
            Command__::Upgrade(_, _, _, arg, _) => vec![arg],
        }
    }

    pub fn types(&self) -> Box<dyn Iterator<Item = &Type> + '_> {
        match self {
            Command__::TransferObjects(args, arg) => {
                Box::new(std::iter::once(arg).chain(args.iter()).map(argument_type))
            }
            Command__::SplitCoins(ty, arg, args) | Command__::MergeCoins(ty, arg, args) => {
                Box::new(
                    std::iter::once(arg)
                        .chain(args.iter())
                        .map(argument_type)
                        .chain(std::iter::once(ty)),
                )
            }
            Command__::MakeMoveVec(ty, args) => {
                Box::new(args.iter().map(argument_type).chain(std::iter::once(ty)))
            }
            Command__::MoveCall(call) => Box::new(
                call.arguments
                    .iter()
                    .map(argument_type)
                    .chain(call.function.type_arguments.iter())
                    .chain(call.function.signature.parameters.iter())
                    .chain(call.function.signature.return_.iter()),
            ),
            Command__::Upgrade(_, _, _, arg, _) => {
                Box::new(std::iter::once(arg).map(argument_type))
            }
            Command__::Publish(_, _, _) => Box::new(std::iter::empty()),
        }
    }
}

//**************************************************************************************************
// Standalone functions
//**************************************************************************************************

pub fn command_types(cmd: &Command) -> impl Iterator<Item = &Type> {
    let result_types = cmd.value.result_type.iter();
    let command_types = cmd.value.command.types();
    result_types.chain(command_types)
}

pub fn argument_type(arg: &Argument) -> &Type {
    &arg.value.1
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
