// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//---------------------------------------------------------------------------
// Serializable Move Values -- these are a representation of Move values that
// keep sizedness of integers in the serialized form and do not need a type layout in order to be
// deserialized into an annotated value.
//---------------------------------------------------------------------------

use core::fmt;
use move_core_types::{
    account_address::AccountAddress,
    annotated_value::{DebugAsDisplay, MoveStruct, MoveValue, MoveVariant},
    identifier::Identifier,
    language_storage::StructTag,
    u256,
};
use serde::{Deserialize, Serialize};

/// A simplified representation of Move values (that in particular drops integer sizing
/// information as this is lost during the serialization/deserialization process to json).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum SerializableMoveValue {
    Bool(bool),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    U256(u256::U256),
    Address(AccountAddress),
    Struct(SimplifiedMoveStruct),
    Vector(Vec<SerializableMoveValue>),
    Variant(SimplifiedMoveVariant),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SimplifiedMoveStruct {
    pub type_: StructTag,
    pub fields: Vec<(Identifier, SerializableMoveValue)>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SimplifiedMoveVariant {
    pub type_: StructTag,
    pub variant_name: Identifier,
    pub tag: u16,
    pub fields: Vec<(Identifier, SerializableMoveValue)>,
}

impl From<MoveValue> for SerializableMoveValue {
    fn from(value: MoveValue) -> Self {
        match value {
            MoveValue::Bool(b) => SerializableMoveValue::Bool(b),
            MoveValue::U256(n) => SerializableMoveValue::U256(n),
            MoveValue::U128(n) => SerializableMoveValue::U128(n),
            MoveValue::U64(n) => SerializableMoveValue::U64(n),
            MoveValue::U32(n) => SerializableMoveValue::U32(n),
            MoveValue::U16(n) => SerializableMoveValue::U16(n),
            MoveValue::U8(n) => SerializableMoveValue::U8(n),
            MoveValue::Address(a) => SerializableMoveValue::Address(a),
            MoveValue::Struct(MoveStruct { type_, fields }) => {
                SerializableMoveValue::Struct(SimplifiedMoveStruct {
                    type_,
                    fields: fields.into_iter().map(|(id, v)| (id, v.into())).collect(),
                })
            }
            MoveValue::Vector(v) => {
                SerializableMoveValue::Vector(v.into_iter().map(Into::into).collect())
            }
            MoveValue::Variant(MoveVariant {
                type_,
                variant_name,
                tag,
                fields,
            }) => SerializableMoveValue::Variant(SimplifiedMoveVariant {
                type_,
                variant_name,
                tag,
                fields: fields.into_iter().map(|(id, v)| (id, v.into())).collect(),
            }),
            MoveValue::Signer(account_address) => SerializableMoveValue::Address(account_address),
        }
    }
}

impl fmt::Display for SerializableMoveValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SerializableMoveValue::Bool(b) => write!(f, "{}", b),
            SerializableMoveValue::U8(n) => write!(f, "{}u8", n),
            SerializableMoveValue::U16(n) => write!(f, "{}u16", n),
            SerializableMoveValue::U32(n) => write!(f, "{}u32", n),
            SerializableMoveValue::U64(n) => write!(f, "{}u64", n),
            SerializableMoveValue::U128(n) => write!(f, "{}u128", n),
            SerializableMoveValue::U256(n) => write!(f, "{}u256", n),
            SerializableMoveValue::Address(a) => write!(f, "{}", a),
            SerializableMoveValue::Struct(s) => {
                write!(f, "{} {{", s.type_)?;
                for (id, v) in &s.fields {
                    write!(f, "{}: {}, ", id, v)?;
                }
                write!(f, "}}")
            }
            SerializableMoveValue::Vector(v) => {
                write!(f, "[")?;
                for e in v {
                    write!(f, "{}, ", e)?;
                }
                write!(f, "]")
            }
            SerializableMoveValue::Variant(v) => {
                write!(f, "{}::{} {{", v.type_, v.variant_name)?;
                for (id, v) in &v.fields {
                    write!(f, "{}: {}, ", id, v)?;
                }
                write!(f, "}}")
            }
        }
    }
}

impl fmt::Display for SimplifiedMoveStruct {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use DebugAsDisplay as DD;
        fmt::Display::fmt(&self.type_, f)?;
        write!(f, " ")?;
        let mut map = f.debug_map();
        for (field, value) in &self.fields {
            map.entry(&DD(field), &DD(value));
        }
        map.finish()
    }
}

impl fmt::Display for SimplifiedMoveVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use DebugAsDisplay as DD;
        let SimplifiedMoveVariant {
            type_,
            variant_name,
            tag: _,
            fields,
        } = self;
        write!(f, "{}::{}", type_, variant_name)?;
        let mut map = f.debug_map();
        for (field, value) in fields {
            map.entry(&DD(field), &DD(value));
        }
        map.finish()
    }
}
