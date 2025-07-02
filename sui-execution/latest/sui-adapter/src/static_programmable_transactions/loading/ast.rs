// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::static_programmable_transactions::linkage::resolved_linkage::{
    ResolvedLinkage, RootedLinkage,
};
use indexmap::IndexSet;
use move_binary_format::file_format::{AbilitySet, CodeOffset, FunctionDefinitionIndex};
use move_core_types::{
    account_address::AccountAddress,
    identifier::IdentStr,
    language_storage::{ModuleId, StructTag},
};
use std::rc::Rc;
use sui_types::{
    Identifier, TypeTag,
    base_types::{ObjectID, ObjectRef, RESOLVED_TX_CONTEXT, SequenceNumber, TxContextKind},
};

//**************************************************************************************************
// AST Nodes
//**************************************************************************************************

#[derive(Debug)]
pub struct Transaction {
    pub inputs: Inputs,
    pub commands: Commands,
}

pub type Inputs = Vec<(InputArg, InputType)>;

pub type Commands = Vec<Command>;

#[derive(Debug)]
pub enum InputArg {
    Pure(Vec<u8>),
    Receiving(ObjectRef),
    Object(ObjectArg),
}

#[derive(Debug)]
pub enum ObjectArg {
    ImmObject(ObjectRef),
    OwnedObject(ObjectRef),
    SharedObject {
        id: ObjectID,
        initial_shared_version: SequenceNumber,
        mutable: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Type {
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Address,
    Signer,
    Vector(Rc<Vector>),
    Datatype(Rc<Datatype>),
    Reference(/* is mut */ bool, Rc<Type>),
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Vector {
    pub abilities: AbilitySet,
    pub element_type: Type,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Datatype {
    pub abilities: AbilitySet,
    pub module: ModuleId,
    pub name: Identifier,
    pub type_arguments: Vec<Type>,
}

#[derive(Debug, Clone)]
pub enum InputType {
    Bytes,
    Fixed(Type),
}

#[derive(Debug)]
pub enum Command {
    MoveCall(Box<MoveCall>),
    TransferObjects(Vec<Argument>, Argument),
    SplitCoins(Argument, Vec<Argument>),
    MergeCoins(Argument, Vec<Argument>),
    MakeMoveVec(/* T for vector<T> */ Option<Type>, Vec<Argument>),
    Publish(Vec<Vec<u8>>, Vec<ObjectID>, ResolvedLinkage),
    Upgrade(
        Vec<Vec<u8>>,
        Vec<ObjectID>,
        ObjectID,
        Argument,
        ResolvedLinkage,
    ),
}

#[derive(Debug)]
pub struct LoadedFunctionInstantiation {
    pub parameters: Vec<Type>,
    pub return_: Vec<Type>,
}

#[derive(Debug)]
pub struct LoadedFunction {
    pub storage_id: ModuleId,
    pub runtime_id: ModuleId,
    pub name: Identifier,
    pub type_arguments: Vec<Type>,
    pub signature: LoadedFunctionInstantiation,
    pub tx_context: TxContextKind,
    pub linkage: RootedLinkage,
    pub instruction_length: CodeOffset,
    pub definition_index: FunctionDefinitionIndex,
}

#[derive(Debug)]
pub struct MoveCall {
    pub function: LoadedFunction,
    pub arguments: Vec<Argument>,
}

pub use sui_types::transaction::Argument;

//**************************************************************************************************
// impl
//**************************************************************************************************

impl ObjectArg {
    pub fn id(&self) -> ObjectID {
        match self {
            ObjectArg::ImmObject(oref) | ObjectArg::OwnedObject(oref) => oref.0,
            ObjectArg::SharedObject { id, .. } => *id,
        }
    }

    pub fn is_mutable(&self) -> bool {
        match self {
            ObjectArg::ImmObject(_) => false,
            ObjectArg::OwnedObject(_) => true,
            ObjectArg::SharedObject { mutable, .. } => *mutable,
        }
    }
}

impl Type {
    pub fn abilities(&self) -> AbilitySet {
        match self {
            Type::Bool
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::U128
            | Type::U256
            | Type::Address => AbilitySet::PRIMITIVES,
            Type::Signer => AbilitySet::SIGNER,
            Type::Reference(_, _) => AbilitySet::REFERENCES,
            Type::Vector(v) => v.abilities,
            Type::Datatype(dt) => dt.abilities,
        }
    }

    pub fn is_tx_context(&self) -> TxContextKind {
        let (is_mut, inner) = match self {
            Type::Reference(is_mut, inner) => (*is_mut, inner),
            _ => return TxContextKind::None,
        };
        let Type::Datatype(dt) = &**inner else {
            return TxContextKind::None;
        };
        if dt.qualified_ident() == RESOLVED_TX_CONTEXT {
            if is_mut {
                TxContextKind::Mutable
            } else {
                TxContextKind::Immutable
            }
        } else {
            TxContextKind::None
        }
    }
    pub fn all_addresses(&self) -> IndexSet<AccountAddress> {
        match self {
            Type::Bool
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::U128
            | Type::U256
            | Type::Address
            | Type::Signer => IndexSet::new(),
            Type::Vector(v) => v.element_type.all_addresses(),
            Type::Reference(_, inner) => inner.all_addresses(),
            Type::Datatype(dt) => dt.all_addresses(),
        }
    }
}

impl Datatype {
    pub fn qualified_ident(&self) -> (&AccountAddress, &IdentStr, &IdentStr) {
        (
            self.module.address(),
            self.module.name(),
            self.name.as_ident_str(),
        )
    }

    pub fn all_addresses(&self) -> IndexSet<AccountAddress> {
        let mut addresses = IndexSet::new();
        addresses.insert(*self.module.address());
        for arg in &self.type_arguments {
            addresses.extend(arg.all_addresses());
        }
        addresses
    }
}

//**************************************************************************************************
// Traits
//**************************************************************************************************

impl TryFrom<Type> for TypeTag {
    type Error = &'static str;
    fn try_from(ty: Type) -> Result<Self, Self::Error> {
        Ok(match ty {
            Type::Bool => TypeTag::Bool,
            Type::U8 => TypeTag::U8,
            Type::U16 => TypeTag::U16,
            Type::U32 => TypeTag::U32,
            Type::U64 => TypeTag::U64,
            Type::U128 => TypeTag::U128,
            Type::U256 => TypeTag::U256,
            Type::Address => TypeTag::Address,
            Type::Signer => TypeTag::Signer,
            Type::Vector(inner) => {
                let Vector { element_type, .. } = &*inner;
                TypeTag::Vector(Box::new(element_type.clone().try_into()?))
            }
            Type::Datatype(dt) => {
                let dt: &Datatype = &dt;
                TypeTag::Struct(Box::new(dt.try_into()?))
            }
            Type::Reference(_, _) => return Err("unexpected reference type"),
        })
    }
}

impl TryFrom<&Datatype> for StructTag {
    type Error = &'static str;

    fn try_from(dt: &Datatype) -> Result<Self, Self::Error> {
        let Datatype {
            module,
            name,
            type_arguments,
            ..
        } = dt;
        Ok(StructTag {
            address: *module.address(),
            module: module.name().to_owned(),
            name: name.to_owned(),
            type_params: type_arguments
                .iter()
                .map(|t| t.clone().try_into())
                .collect::<Result<Vec<TypeTag>, _>>()?,
        })
    }
}

//**************************************************************************************************
// Tests
//**************************************************************************************************

#[test]
fn enum_size() {
    assert_eq!(std::mem::size_of::<Type>(), 16);
}
