// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use move_binary_format::file_format::AbilitySet;
use move_core_types::{
    identifier::IdentStr,
    language_storage::{ModuleId, StructTag},
    resolver::{ModuleResolver, ResourceResolver},
};
use move_vm_types::loaded_data::runtime_types::Type;
use serde::Deserialize;
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    coin::Coin,
    error::{ExecutionError, ExecutionErrorKind},
    object::{Data, MoveObject, Object, Owner},
    storage::{ChildObjectResolver, ObjectChange, ParentSync, Storage},
};

pub trait StorageView<E: std::fmt::Debug>:
    ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage + ParentSync + ChildObjectResolver
{
}
impl<
        E: std::fmt::Debug,
        T: ResourceResolver<Error = E>
            + ModuleResolver<Error = E>
            + Storage
            + ParentSync
            + ChildObjectResolver,
    > StorageView<E> for T
{
}

pub struct ExecutionResults {
    pub object_changes: BTreeMap<ObjectID, ObjectChange>,
    pub user_events: Vec<(ModuleId, StructTag, Vec<u8>)>,
}

#[derive(Clone)]
pub struct InputObjectMetadata {
    pub id: ObjectID,
    pub is_mutable_input: bool,
    pub owner: Owner,
    pub version: SequenceNumber,
}

#[derive(Clone)]
pub struct InputValue {
    /// Used to remember the object ID and owner even if the value is taken
    pub object_metadata: Option<InputObjectMetadata>,
    pub inner: ResultValue,
}

#[derive(Clone)]
pub struct ResultValue {
    /// This is used primarily for values that have `copy` but not `drop` as they must have been
    /// copied after the last borrow, otherwise we cannot consider the last "copy" to be instead
    /// a "move" of the value.
    pub last_usage_kind: Option<UsageKind>,
    pub value: Option<Value>,
}

#[derive(Clone, Copy)]
pub enum UsageKind {
    BorrowImm,
    BorrowMut,
    Take,
    Clone,
}

#[derive(Clone)]
pub enum Value {
    Object(ObjectValue),
    Raw(RawValueType, Vec<u8>),
    /// Special cased empty vector generated from MakeMove that can be used to populate any type
    EmptyVec,
}

#[derive(Clone)]
pub struct ObjectValue {
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
pub enum RawValueType {
    Any,
    Loaded {
        ty: Type,
        abilities: AbilitySet,
        used_in_non_entry_move_call: bool,
    },
}

#[derive(PartialEq, Eq)]
pub enum ValueType {
    AnyPrimitive,
    AnyVec,
    Loaded { ty: Type, abilities: AbilitySet },
    Object(StructTag),
}

#[derive(Clone, Copy)]
pub enum CommandKind<'a> {
    MoveCall {
        package: ObjectID,
        module: &'a IdentStr,
        function: &'a IdentStr,
    },
    MakeMoveVec,
    TransferObjects,
    SplitCoin,
    MergeCoins,
    Publish,
}

impl InputValue {
    pub fn new_object(object_metadata: InputObjectMetadata, value: ObjectValue) -> Self {
        InputValue {
            object_metadata: Some(object_metadata),
            inner: ResultValue::new(Value::Object(value)),
        }
    }

    pub fn new_raw(ty: RawValueType, value: Vec<u8>) -> Self {
        InputValue {
            object_metadata: None,
            inner: ResultValue::new(Value::Raw(ty, value)),
        }
    }
}

impl ResultValue {
    pub fn new(value: Value) -> Self {
        Self {
            last_usage_kind: None,
            value: Some(value),
        }
    }
}

impl Value {
    pub fn is_copyable(&self) -> bool {
        match self {
            Value::Object(_) => false,
            Value::Raw(RawValueType::Any, _) => true,
            Value::Raw(RawValueType::Loaded { abilities, .. }, _) => abilities.has_copy(),
            Value::EmptyVec => false,
        }
    }

    pub fn to_bcs_bytes(&self) -> Vec<u8> {
        match self {
            Value::Object(obj_value) => obj_value.to_bcs_bytes(),
            Value::Raw(_, bytes) => bytes.clone(),
            // BCS layout for any empty vector should be the same
            Value::EmptyVec => bcs::to_bytes::<Vec<u8>>(&vec![]).unwrap(),
        }
    }

    pub fn was_used_in_non_entry_move_call(&self) -> bool {
        match self {
            Value::Object(obj) => obj.used_in_non_entry_move_call,
            // Any is only used for Pure inputs, and if it was used by &mut it would have switched
            // to Loaded
            Value::Raw(RawValueType::Any, _) => false,
            Value::Raw(
                RawValueType::Loaded {
                    used_in_non_entry_move_call,
                    ..
                },
                _,
            ) => *used_in_non_entry_move_call,
            // EmptyVec is generated only by MakeMoveVec and cannot be polluted by
            // public move calls. If it is used by &mut, it would have switched to Loaded
            Value::EmptyVec => false,
        }
    }

    pub fn type_(&self) -> ValueType {
        match self {
            Value::Object(obj) => ValueType::Object(obj.type_.clone()),
            Value::Raw(RawValueType::Loaded { ty, abilities, .. }, _) => ValueType::Loaded {
                ty: ty.clone(),
                abilities: *abilities,
            },
            Value::Raw(RawValueType::Any, _) => ValueType::AnyPrimitive,
            Value::EmptyVec => ValueType::AnyVec,
        }
    }
}

impl ObjectValue {
    pub fn new(
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
            type_,
            has_public_transfer,
            used_in_non_entry_move_call,
            contents,
        })
    }

    pub fn from_object(object: &Object) -> Result<Self, ExecutionError> {
        let Object { data, .. } = object;
        match data {
            Data::Package(_) => invariant_violation!("Expected a Move object"),
            Data::Move(move_object) => Self::from_move_object(move_object),
        }
    }

    pub fn from_move_object(object: &MoveObject) -> Result<Self, ExecutionError> {
        Self::new(
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
            used_in_non_entry_move_call: false,
            contents: ObjectContents::Coin(coin),
        })
    }

    pub fn ensure_public_transfer_eligible(&self) -> Result<(), ExecutionError> {
        if !self.has_public_transfer {
            return Err(ExecutionErrorKind::InvalidTransferObject.into());
        }
        Ok(())
    }

    pub fn to_bcs_bytes(&self) -> Vec<u8> {
        match &self.contents {
            ObjectContents::Raw(bytes) => bytes.clone(),
            ObjectContents::Coin(coin) => coin.to_bcs_bytes(),
        }
    }
}

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
            Value::Raw(RawValueType::Any, _) => {
                todo!("support this for dev inspect")
            }
            Value::Raw(RawValueType::Loaded { .. }, _) | Value::EmptyVec => {
                panic!("not an object")
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
        Value::Object(_) | Value::EmptyVec => {
            panic!("expected non object")
        }
        Value::Raw(RawValueType::Any, bytes) => {
            let Ok(val) = bcs::from_bytes(bytes) else {
                panic!("invalid pure arg")
            };
            Ok(val)
        }
        Value::Raw(RawValueType::Loaded { ty, .. }, bytes) => {
            if ty == &expected_ty {
                panic!("type mismatch")
            }
            let Ok(res) = bcs::from_bytes(bytes) else {
                panic!("invalid bytes for type")
            };
            Ok(res)
        }
    }
}
