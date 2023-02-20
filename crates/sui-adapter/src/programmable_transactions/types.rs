// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::file_format::AbilitySet;
use move_core_types::language_storage::StructTag;
use move_vm_types::loaded_data::runtime_types::Type;
use serde::Deserialize;
use sui_types::{
    base_types::SuiAddress,
    coin::Coin,
    error::{ExecutionError, ExecutionErrorKind},
    object::{Data, MoveObject, Object, Owner},
};

pub enum Value {
    Object(ObjectValue),
    Raw(ValueType, Vec<u8>),
}

pub struct ObjectValue {
    pub type_: StructTag,
    pub has_public_transfer: bool,
    // None for objects created this transaction
    pub owner: Option<Owner>,
    // true if it has been used in a public Move call
    // In other words, false if all usages have been with non-Move comamnds or
    // entry Move functions
    pub used_in_public_move_call: bool,
    pub contents: ObjectContents,
}

pub enum ObjectContents {
    Coin(Coin),
    Raw(Vec<u8>),
}

pub enum ValueType {
    Any,
    Loaded { ty: Type, abilities: AbilitySet },
}

impl Value {}

impl ObjectValue {
    pub fn from_object(object: &Object) -> Result<Self, ExecutionError> {
        let Object { data, owner, .. } = object;
        match data {
            Data::Package(_) => invariant_violation!("Expected a Move object"),
            Data::Move(move_object) => Self::from_move_object(*owner, move_object),
        }
    }

    pub fn from_move_object(owner: Owner, object: &MoveObject) -> Result<Self, ExecutionError> {
        let type_ = object.type_.clone();
        let has_public_transfer = object.has_public_transfer();
        let contents = object.contents();
        let owner = Some(owner);
        let used_in_public_move_call = false;
        let contents = if Coin::is_coin(&type_) {
            ObjectContents::Coin(Coin::from_bcs_bytes(contents)?)
        } else {
            ObjectContents::Raw(contents.to_vec())
        };
        Ok(Self {
            type_,
            has_public_transfer,
            owner,
            used_in_public_move_call,
            contents,
        })
    }

    pub fn coin(type_: StructTag, coin: Coin) -> Result<Self, ExecutionError> {
        assert_invariant!(
            Coin::is_coin(&type_),
            "Cannot make a coin without a coin type"
        );
        Ok(Self {
            type_,
            has_public_transfer: true,
            owner: None,
            used_in_public_move_call: false,
            contents: ObjectContents::Coin(coin),
        })
    }

    pub fn ensure_public_transfer_eligible(&self) -> Result<(), ExecutionError> {
        if !matches!(self.owner, None | Some(Owner::AddressOwner(_))) {
            return Err(ExecutionErrorKind::InvalidTransferObject.into());
        }
        if !self.has_public_transfer {
            return Err(ExecutionErrorKind::InvalidTransferObject.into());
        }
        Ok(())
    }
}

impl ObjectContents {}
impl ValueType {}

pub trait TryFromValue: Sized {
    fn try_from_value(value: Value) -> Result<Self, ExecutionError>;
}

impl TryFromValue for Value {
    fn try_from_value(value: Value) -> Result<Self, ExecutionError> {
        Ok(value)
    }
}

impl TryFromValue for ObjectValue {
    fn try_from_value(value: Value) -> Result<Self, ExecutionError> {
        match value {
            Value::Object(o) => Ok(o),
            Value::Raw(ty, _) => {
                if let ValueType::Loaded { abilities, .. } = ty {
                    assert_invariant!(!abilities.has_key(), "Raw values should not be objects")
                }
                panic!("expected object")
            }
        }
    }
}

impl TryFromValue for SuiAddress {
    fn try_from_value(value: Value) -> Result<Self, ExecutionError> {
        try_from_value_prim(&value, Type::Address)
    }
}

impl TryFromValue for u64 {
    fn try_from_value(value: Value) -> Result<Self, ExecutionError> {
        try_from_value_prim(&value, Type::U64)
    }
}

fn try_from_value_prim<'a, T: Deserialize<'a>>(
    value: &'a Value,
    expected_ty: Type,
) -> Result<T, ExecutionError> {
    match value {
        Value::Object(_obj) => panic!("expected raw"),
        Value::Raw(ValueType::Any, bytes) => {
            let Ok(val) = bcs::from_bytes(bytes) else {
                panic!("invalid pure arg")
            };
            Ok(val)
        }
        Value::Raw(ValueType::Loaded { ty, .. }, bytes) => {
            if ty == &expected_ty {
                panic!("type mismatch")
            }
            let res = bcs::from_bytes(bytes);
            assert_invariant!(
                res.is_ok(),
                "Move values should be able to deserialize into their type"
            );
            Ok(res.unwrap())
        }
    }
}
