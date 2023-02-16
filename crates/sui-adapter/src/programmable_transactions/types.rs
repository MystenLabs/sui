// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::file_format::AbilitySet;
use move_core_types::{identifier::IdentStr, language_storage::StructTag};
use move_vm_types::loaded_data::runtime_types::Type;
use serde::Deserialize;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    coin::Coin,
    error::{ExecutionError, ExecutionErrorKind},
    object::{Data, MoveObject, Object, Owner},
};

#[derive(Clone)]
pub enum Value {
    Object(ObjectValue),
    Raw(ValueType, Vec<u8>),
}

#[derive(Clone)]
pub struct ObjectValue {
    // None for objects created this transaction
    pub owner: Option<Owner>,
    pub type_: StructTag,
    pub has_public_transfer: bool,
    // true if it has been used in a public, non-entry Move call
    // In other words, false if all usages have been with non-Move comamnds or
    // entry Move functions
    pub used_in_non_entry_move_call: bool,
    pub contents: ObjectContents,
}

#[derive(Clone)]
pub enum ObjectContents {
    Coin(Coin),
    Raw(Vec<u8>),
}

#[derive(Clone)]
pub enum ValueType {
    Any,
    Loaded { ty: Type, abilities: AbilitySet },
}

#[derive(Clone, Copy)]
pub enum CommandKind<'a> {
    MoveCall {
        package: ObjectID,
        module: &'a IdentStr,
        function: &'a IdentStr,
    },
    TransferObjects,
    SplitCoin,
    MergeCoins,
    Publish,
}

impl Value {
    pub fn is_copyable(&self) -> bool {
        match self {
            Value::Object(_) => false,
            Value::Raw(ValueType::Any, _) => true,
            Value::Raw(ValueType::Loaded { abilities, .. }, _) => abilities.has_copy(),
        }
    }

    pub fn to_bcs_bytes(&self) -> Vec<u8> {
        match self {
            Value::Object(ObjectValue {
                contents: ObjectContents::Raw(bytes),
                ..
            })
            | Value::Raw(_, bytes) => bytes.clone(),
            Value::Object(ObjectValue {
                contents: ObjectContents::Coin(coin),
                ..
            }) => coin.to_bcs_bytes(),
        }
    }

    pub fn was_used_in_non_entry_move_call(&self) -> bool {
        match self {
            Value::Object(obj) => obj.used_in_non_entry_move_call,
            // Any is only used for Pure inputs, and if it was used by &mut it would have switched
            // to Loaded
            Value::Raw(ValueType::Any, _) => false,
            Value::Raw(ValueType::Loaded { .. }, _) => true,
        }
    }
}

impl ObjectValue {
    pub fn new(
        owner: Option<Owner>,
        type_: StructTag,
        has_public_transfer: bool,
        used_in_non_entry_move_call: bool,
        contents: &[u8],
    ) -> Result<Self, ExecutionError> {
        let contents = if Coin::is_coin(&type_) {
            ObjectContents::Coin(Coin::from_bcs_bytes(contents)?)
        } else {
            ObjectContents::Raw(contents.to_vec())
        };
        Ok(Self {
            owner,
            type_,
            has_public_transfer,
            used_in_non_entry_move_call,
            contents,
        })
    }

    pub fn from_object(object: &Object) -> Result<Self, ExecutionError> {
        let Object { data, owner, .. } = object;
        match data {
            Data::Package(_) => invariant_violation!("Expected a Move object"),
            Data::Move(move_object) => Self::from_move_object(*owner, move_object),
        }
    }

    pub fn from_move_object(owner: Owner, object: &MoveObject) -> Result<Self, ExecutionError> {
        Self::new(
            Some(owner),
            object.type_.clone(),
            object.has_public_transfer(),
            false,
            object.contents(),
        )
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
            used_in_non_entry_move_call: false,
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
            Value::Raw(_, _) => {
                todo!("support this for dev inspect")
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
