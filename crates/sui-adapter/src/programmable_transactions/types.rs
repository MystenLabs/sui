// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use move_binary_format::file_format::AbilitySet;
use move_core_types::{
    identifier::IdentStr,
    language_storage::{ModuleId, StructTag},
    resolver::{ModuleResolver, ResourceResolver},
};
use move_vm_runtime::{move_vm::MoveVM, session::Session};
use move_vm_types::loaded_data::runtime_types::Type;
use serde::Deserialize;
use sui_types::execution_status::CommandArgumentError;
use sui_types::{
    base_types::{MoveObjectType, ObjectID, SequenceNumber, SuiAddress},
    coin::Coin,
    error::{ExecutionError, ExecutionErrorKind, SuiError},
    object::{Data, MoveObject, Object, Owner},
    storage::{BackingPackageStore, ChildObjectResolver, ObjectChange, ParentSync, Storage},
    TypeTag,
};

use super::{context::load_type, linkage_view::LinkageView};

sui_macros::checked_arithmetic! {

pub trait StorageView:
    ResourceResolver<Error = SuiError>
    + ModuleResolver<Error = SuiError>
    + BackingPackageStore
    + Storage
    + ParentSync
    + ChildObjectResolver
{
}

impl<
        T: ResourceResolver<Error = SuiError>
            + ModuleResolver<Error = SuiError>
            + BackingPackageStore
            + Storage
            + ParentSync
            + ChildObjectResolver,
    > StorageView for T
{
}

pub struct ExecutionResults {
    pub object_changes: BTreeMap<ObjectID, ObjectChange>,
    pub user_events: Vec<(ModuleId, StructTag, Vec<u8>)>,
}

#[derive(Clone, Debug)]
pub struct InputObjectMetadata {
    pub id: ObjectID,
    pub is_mutable_input: bool,
    pub owner: Owner,
    pub version: SequenceNumber,
}

#[derive(Clone, Debug)]
pub struct InputValue {
    /// Used to remember the object ID and owner even if the value is taken
    pub object_metadata: Option<InputObjectMetadata>,
    pub inner: ResultValue,
}

#[derive(Clone, Debug)]
pub struct ResultValue {
    /// This is used primarily for values that have `copy` but not `drop` as they must have been
    /// copied after the last borrow, otherwise we cannot consider the last "copy" to be instead
    /// a "move" of the value.
    pub last_usage_kind: Option<UsageKind>,
    pub value: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageKind {
    BorrowImm,
    BorrowMut,
    ByValue,
}

#[derive(Debug, Clone)]
pub enum Value {
    Object(ObjectValue),
    Raw(RawValueType, Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct ObjectValue {
    pub type_: Type,
    pub has_public_transfer: bool,
    // true if it has been used in a public, non-entry Move call
    // In other words, false if all usages have been with non-Move commands or
    // entry Move functions
    pub used_in_non_entry_move_call: bool,
    pub contents: ObjectContents,
}

#[derive(Debug, Clone)]
pub enum ObjectContents {
    Coin(Coin),
    Raw(Vec<u8>),
}

#[derive(Debug, Clone)]
pub enum RawValueType {
    Any,
    Loaded {
        ty: Type,
        abilities: AbilitySet,
        used_in_non_entry_move_call: bool,
    },
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
    SplitCoins,
    MergeCoins,
    Publish,
    Upgrade,
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
        }
    }

    pub fn write_bcs_bytes(&self, buf: &mut Vec<u8>) {
        match self {
            Value::Object(obj_value) => obj_value.write_bcs_bytes(buf),
            Value::Raw(_, bytes) => buf.extend(bytes),
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
        }
    }
}

impl ObjectValue {
    pub fn new<'vm, 'state, S: StorageView>(
        vm: &'vm MoveVM,
        session: &mut Session<'state, 'vm, LinkageView<'state, S>>,
        type_: MoveObjectType,
        has_public_transfer: bool,
        used_in_non_entry_move_call: bool,
        contents: &[u8],
    ) -> Result<Self, ExecutionError> {
        let contents = if type_.is_coin() {
            let Ok(coin) = Coin::from_bcs_bytes(contents) else{
                invariant_violation!("Could not deserialize a coin")
            };
            ObjectContents::Coin(coin)
        } else {
            ObjectContents::Raw(contents.to_vec())
        };
        let tag: StructTag = type_.into();
        let type_ = load_type(session, &TypeTag::Struct(Box::new(tag)))
            .map_err(|e| crate::error::convert_vm_error(e, vm, session.get_resolver()))?;
        Ok(Self {
            type_,
            has_public_transfer,
            used_in_non_entry_move_call,
            contents,
        })
    }

    pub fn from_object<'vm, 'state, S: StorageView>(
        vm: &'vm MoveVM,
        session: &mut Session<'state, 'vm, LinkageView<'state, S>>,
        object: &Object,
    ) -> Result<Self, ExecutionError> {
        let Object { data, .. } = object;
        match data {
            Data::Package(_) => invariant_violation!("Expected a Move object"),
            Data::Move(move_object) => Self::from_move_object(vm, session, move_object),
        }
    }

    pub fn from_move_object<'vm, 'state, S: StorageView>(
        vm: &'vm MoveVM,
        session: &mut Session<'state, 'vm, LinkageView<'state, S>>,
        object: &MoveObject,
    ) -> Result<Self, ExecutionError> {
        Self::new(
            vm,
            session,
            object.type_().clone(),
            object.has_public_transfer(),
            false,
            object.contents(),
        )
    }

    /// # Safety
    /// We must have the Type is the coin type, but we are unable to check it at this spot
    pub unsafe fn coin(type_: Type, coin: Coin) -> Self {
        Self {
            type_,
            has_public_transfer: true,
            used_in_non_entry_move_call: false,
            contents: ObjectContents::Coin(coin),
        }
    }

    pub fn ensure_public_transfer_eligible(&self) -> Result<(), ExecutionError> {
        if !self.has_public_transfer {
            return Err(ExecutionErrorKind::InvalidTransferObject.into());
        }
        Ok(())
    }

    pub fn write_bcs_bytes(&self, buf: &mut Vec<u8>) {
        match &self.contents {
            ObjectContents::Raw(bytes) => buf.extend(bytes),
            ObjectContents::Coin(coin) => buf.extend(coin.to_bcs_bytes()),
        }
    }
}

pub trait TryFromValue: Sized {
    fn try_from_value(value: Value) -> Result<Self, CommandArgumentError>;
}

impl TryFromValue for Value {
    fn try_from_value(value: Value) -> Result<Self, CommandArgumentError> {
        Ok(value)
    }
}

impl TryFromValue for ObjectValue {
    fn try_from_value(value: Value) -> Result<Self, CommandArgumentError> {
        match value {
            Value::Object(o) => Ok(o),
            Value::Raw(RawValueType::Any, _) => Err(CommandArgumentError::TypeMismatch),
            Value::Raw(RawValueType::Loaded { .. }, _) => Err(CommandArgumentError::TypeMismatch),
        }
    }
}

impl TryFromValue for SuiAddress {
    fn try_from_value(value: Value) -> Result<Self, CommandArgumentError> {
        try_from_value_prim(&value, Type::Address)
    }
}

impl TryFromValue for u64 {
    fn try_from_value(value: Value) -> Result<Self, CommandArgumentError> {
        try_from_value_prim(&value, Type::U64)
    }
}

fn try_from_value_prim<'a, T: Deserialize<'a>>(
    value: &'a Value,
    expected_ty: Type,
) -> Result<T, CommandArgumentError> {
    match value {
        Value::Object(_) => Err(CommandArgumentError::TypeMismatch),
        Value::Raw(RawValueType::Any, bytes) => {
            bcs::from_bytes(bytes).map_err(|_| CommandArgumentError::InvalidBCSBytes)
        }
        Value::Raw(RawValueType::Loaded { ty, .. }, bytes) => {
            if ty != &expected_ty {
                return Err(CommandArgumentError::TypeMismatch);
            }
            bcs::from_bytes(bytes).map_err(|_| CommandArgumentError::InvalidBCSBytes)
        }
    }
}

pub fn command_argument_error(e: CommandArgumentError, arg_idx: usize) -> ExecutionError {
    ExecutionError::from_kind(ExecutionErrorKind::command_argument_error(
        e,
        arg_idx as u16,
    ))
}

}
