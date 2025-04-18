// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{ops::Deref, rc::Rc};

use move_binary_format::file_format::AbilitySet;
use move_core_types::{
    account_address::AccountAddress, identifier::IdentStr, language_storage::ModuleId,
};
use sui_types::{
    base_types::{ObjectID, TxContextKind, RESOLVED_TX_CONTEXT},
    transaction::CallArg,
    Identifier,
};

//**************************************************************************************************
// AST Nodes
//**************************************************************************************************

pub struct Transaction {
    pub inputs: Inputs,
    pub commands: Commands,
}

pub type Inputs = Vec<(CallArg, InputType)>;

pub type Commands = Vec<Command>;

#[derive(Clone)]
pub enum InputType {
    Bytes,
    Fixed(Type),
}

#[derive(Clone, Debug)]
pub struct Type(pub Rc<Type_>);

#[derive(Debug)]
pub enum Type_ {
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
    Reference(/* is mut */ bool, Type),
}

#[derive(Debug)]
pub struct Vector {
    pub abilities: AbilitySet,
    pub element_type: Type,
}

#[derive(Debug)]
pub struct Datatype {
    pub abilities: AbilitySet,
    pub module: ModuleId,
    pub name: Identifier,
    pub type_arguments: Vec<Type>,
}

pub enum Command {
    MoveCall(Box<MoveCall>),
    TransferObjects(Vec<Argument>, Argument),
    SplitCoins(Argument, Vec<Argument>),
    MergeCoins(Argument, Vec<Argument>),
    MakeMoveVec(/* T for vector<T> */ Option<Type>, Vec<Argument>),
    Publish(Vec<Vec<u8>>, Vec<ObjectID>),
    Upgrade(Vec<Vec<u8>>, Vec<ObjectID>, ObjectID, Argument),
}

pub struct LoadedFunctionInstantiation {
    pub parameters: Vec<Type>,
    pub return_: Vec<Type>,
}

pub struct LoadedFunction {
    pub storage_id: ModuleId,
    pub runtime_id: ModuleId,
    pub name: Identifier,
    pub type_arguments: Vec<Type>,
    pub signature: LoadedFunctionInstantiation,
    pub tx_context: TxContextKind,
}

pub struct MoveCall {
    pub function: LoadedFunction,
    pub arguments: Vec<Argument>,
}

pub use sui_types::transaction::Argument;

//**************************************************************************************************
// impl
//**************************************************************************************************

impl Type_ {
    pub fn abilities(&self) -> AbilitySet {
        match self {
            Type_::Bool
            | Type_::U8
            | Type_::U16
            | Type_::U32
            | Type_::U64
            | Type_::U128
            | Type_::U256
            | Type_::Address => AbilitySet::PRIMITIVES,
            Type_::Signer => AbilitySet::SIGNER,
            Type_::Reference(_, _) => AbilitySet::REFERENCES,
            Type_::Vector(v) => v.abilities,
            Type_::Datatype(dt) => dt.abilities,
        }
    }

    pub fn is_tx_context(ty: &Type) -> TxContextKind {
        let (is_mut, inner) = match &*ty.0 {
            Type_::Reference(is_mut, inner) => (*is_mut, inner),
            _ => return TxContextKind::None,
        };
        let Type_::Datatype(dt) = &*inner.0 else {
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
}

impl Datatype {
    pub fn qualified_ident(&self) -> (&AccountAddress, &IdentStr, &IdentStr) {
        (
            self.module.address(),
            self.module.name(),
            self.name.as_ident_str(),
        )
    }
}

//**************************************************************************************************
// Traits
//**************************************************************************************************

impl Deref for Type {
    type Target = Type_;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

//**************************************************************************************************
// Tests
//**************************************************************************************************

#[test]
fn enum_size() {
    assert_eq!(std::mem::size_of::<Type>(), 16);
}
