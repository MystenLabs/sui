// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::{StructTag, TypeTag},
    u256,
};
use anyhow::{anyhow, bail, Result as AResult};
use serde::{
    de::Error as DeError,
    ser::{SerializeMap, SerializeSeq, SerializeStruct, SerializeTuple},
    Deserialize, Serialize,
};
use std::{
    convert::TryInto,
    fmt::{self, Debug},
};

/// In the `WithTypes` configuration, a Move struct gets serialized into a Serde struct with this name
pub const MOVE_STRUCT_NAME: &str = "struct";

/// In the `WithTypes` configuration, a Move struct gets serialized into a Serde struct with this as the first field
pub const MOVE_STRUCT_TYPE: &str = "type";

/// In the `WithTypes` configuration, a Move struct gets serialized into a Serde struct with this as the second field
pub const MOVE_STRUCT_FIELDS: &str = "fields";

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum MoveStruct {
    /// The representation used by the MoveVM
    Runtime(Vec<MoveValue>),
    /// A decorated representation with human-readable field names
    WithFields(Vec<(Identifier, MoveValue)>),
    /// An even more decorated representation with both types and human-readable field names
    WithTypes {
        type_: StructTag,
        fields: Vec<(Identifier, MoveValue)>,
    },
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum MoveValue {
    U8(u8),
    U64(u64),
    U128(u128),
    Bool(bool),
    Address(AccountAddress),
    Vector(Vec<MoveValue>),
    Struct(MoveStruct),
    Signer(AccountAddress),
    // NOTE: Added in bytecode version v6, do not reorder!
    U16(u16),
    U32(u32),
    U256(u256::U256),
}

/// A layout associated with a named field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveFieldLayout {
    pub name: Identifier,
    pub layout: MoveTypeLayout,
}

impl MoveFieldLayout {
    pub fn new(name: Identifier, layout: MoveTypeLayout) -> Self {
        Self { name, layout }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MoveStructLayout {
    /// The representation used by the MoveVM
    Runtime(Vec<MoveTypeLayout>),
    /// A decorated representation with human-readable field names that can be used by clients
    WithFields(Vec<MoveFieldLayout>),
    /// An even more decorated representation with both types and human-readable field names
    WithTypes {
        type_: StructTag,
        fields: Vec<MoveFieldLayout>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MoveTypeLayout {
    #[serde(rename(serialize = "bool", deserialize = "bool"))]
    Bool,
    #[serde(rename(serialize = "u8", deserialize = "u8"))]
    U8,
    #[serde(rename(serialize = "u64", deserialize = "u64"))]
    U64,
    #[serde(rename(serialize = "u128", deserialize = "u128"))]
    U128,
    #[serde(rename(serialize = "address", deserialize = "address"))]
    Address,
    #[serde(rename(serialize = "vector", deserialize = "vector"))]
    Vector(Box<MoveTypeLayout>),
    #[serde(rename(serialize = "struct", deserialize = "struct"))]
    Struct(MoveStructLayout),
    #[serde(rename(serialize = "signer", deserialize = "signer"))]
    Signer,

    // NOTE: Added in bytecode version v6, do not reorder!
    #[serde(rename(serialize = "u16", deserialize = "u16"))]
    U16,
    #[serde(rename(serialize = "u32", deserialize = "u32"))]
    U32,
    #[serde(rename(serialize = "u256", deserialize = "u256"))]
    U256,
}

impl MoveValue {
    pub fn simple_deserialize(blob: &[u8], ty: &MoveTypeLayout) -> AResult<Self> {
        Ok(bcs::from_bytes_seed(ty, blob)?)
    }

    pub fn simple_serialize(&self) -> Option<Vec<u8>> {
        bcs::to_bytes(self).ok()
    }

    pub fn vector_u8(v: Vec<u8>) -> Self {
        MoveValue::Vector(v.into_iter().map(MoveValue::U8).collect())
    }

    /// Converts the `Vec<MoveValue>` to a `Vec<u8>` if the inner `MoveValue` is a `MoveValue::U8`,
    /// or returns an error otherwise.
    pub fn vec_to_vec_u8(vec: Vec<MoveValue>) -> AResult<Vec<u8>> {
        let mut vec_u8 = Vec::with_capacity(vec.len());

        for byte in vec {
            match byte {
                MoveValue::U8(u8) => {
                    vec_u8.push(u8);
                }
                _ => {
                    return Err(anyhow!(
                        "Expected inner MoveValue in Vec<MoveValue> to be a MoveValue::U8"
                            .to_string(),
                    ));
                }
            }
        }
        Ok(vec_u8)
    }

    pub fn vector_address(v: Vec<AccountAddress>) -> Self {
        MoveValue::Vector(v.into_iter().map(MoveValue::Address).collect())
    }

    pub fn decorate(self, layout: &MoveTypeLayout) -> Self {
        match (self, layout) {
            (MoveValue::Struct(s), MoveTypeLayout::Struct(l)) => MoveValue::Struct(s.decorate(l)),
            (MoveValue::Vector(vals), MoveTypeLayout::Vector(t)) => {
                MoveValue::Vector(vals.into_iter().map(|v| v.decorate(t)).collect())
            }
            (v, _) => v,
        }
    }

    pub fn undecorate(self) -> Self {
        match self {
            Self::Struct(s) => MoveValue::Struct(s.undecorate()),
            Self::Vector(vals) => {
                MoveValue::Vector(vals.into_iter().map(MoveValue::undecorate).collect())
            }
            v => v,
        }
    }
}

pub fn serialize_values<'a, I>(vals: I) -> Vec<Vec<u8>>
where
    I: IntoIterator<Item = &'a MoveValue>,
{
    vals.into_iter()
        .map(|val| {
            val.simple_serialize()
                .expect("serialization should succeed")
        })
        .collect()
}

impl MoveStruct {
    pub fn new(value: Vec<MoveValue>) -> Self {
        Self::Runtime(value)
    }

    pub fn with_fields(values: Vec<(Identifier, MoveValue)>) -> Self {
        Self::WithFields(values)
    }

    pub fn with_types(type_: StructTag, fields: Vec<(Identifier, MoveValue)>) -> Self {
        Self::WithTypes { type_, fields }
    }

    pub fn simple_deserialize(blob: &[u8], ty: &MoveStructLayout) -> AResult<Self> {
        Ok(bcs::from_bytes_seed(ty, blob)?)
    }

    pub fn decorate(self, layout: &MoveStructLayout) -> Self {
        match (self, layout) {
            (MoveStruct::Runtime(vals), MoveStructLayout::WithFields(layouts)) => {
                MoveStruct::WithFields(
                    vals.into_iter()
                        .zip(layouts)
                        .map(|(v, l)| (l.name.clone(), v.decorate(&l.layout)))
                        .collect(),
                )
            }
            (MoveStruct::Runtime(vals), MoveStructLayout::WithTypes { type_, fields }) => {
                MoveStruct::WithTypes {
                    type_: type_.clone(),
                    fields: vals
                        .into_iter()
                        .zip(fields)
                        .map(|(v, l)| (l.name.clone(), v.decorate(&l.layout)))
                        .collect(),
                }
            }
            (MoveStruct::WithFields(vals), MoveStructLayout::WithTypes { type_, fields }) => {
                MoveStruct::WithTypes {
                    type_: type_.clone(),
                    fields: vals
                        .into_iter()
                        .zip(fields)
                        .map(|((fld, v), l)| (fld, v.decorate(&l.layout)))
                        .collect(),
                }
            }
            (v, _) => v, // already decorated
        }
    }

    pub fn fields(&self) -> &[MoveValue] {
        match self {
            Self::Runtime(vals) => vals,
            Self::WithFields(_) | Self::WithTypes { .. } => {
                // It's not possible to implement this without changing the return type, and thus
                // panicking is the best move
                panic!("Getting fields for decorated representation")
            }
        }
    }

    pub fn into_fields(self) -> Vec<MoveValue> {
        match self {
            Self::Runtime(vals) => vals,
            Self::WithFields(fields) | Self::WithTypes { fields, .. } => {
                fields.into_iter().map(|(_, f)| f).collect()
            }
        }
    }

    pub fn undecorate(self) -> Self {
        Self::Runtime(
            self.into_fields()
                .into_iter()
                .map(MoveValue::undecorate)
                .collect(),
        )
    }
}

impl MoveStructLayout {
    pub fn new(types: Vec<MoveTypeLayout>) -> Self {
        Self::Runtime(types)
    }

    pub fn with_fields(types: Vec<MoveFieldLayout>) -> Self {
        Self::WithFields(types)
    }

    pub fn with_types(type_: StructTag, fields: Vec<MoveFieldLayout>) -> Self {
        Self::WithTypes { type_, fields }
    }

    pub fn fields(&self) -> &[MoveTypeLayout] {
        match self {
            Self::Runtime(vals) => vals,
            Self::WithFields(_) | Self::WithTypes { .. } => {
                // It's not possible to implement this without changing the return type, and some
                // performance-critical VM serialization code uses the Runtime case of this.
                // panicking is the best move
                panic!("Getting fields for decorated representation")
            }
        }
    }

    pub fn into_fields(self) -> Vec<MoveTypeLayout> {
        match self {
            Self::Runtime(vals) => vals,
            Self::WithFields(fields) | Self::WithTypes { fields, .. } => {
                fields.into_iter().map(|f| f.layout).collect()
            }
        }
    }
}

impl<'d> serde::de::DeserializeSeed<'d> for &MoveTypeLayout {
    type Value = MoveValue;
    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        match self {
            MoveTypeLayout::Bool => bool::deserialize(deserializer).map(MoveValue::Bool),
            MoveTypeLayout::U8 => u8::deserialize(deserializer).map(MoveValue::U8),
            MoveTypeLayout::U16 => u16::deserialize(deserializer).map(MoveValue::U16),
            MoveTypeLayout::U32 => u32::deserialize(deserializer).map(MoveValue::U32),
            MoveTypeLayout::U64 => u64::deserialize(deserializer).map(MoveValue::U64),
            MoveTypeLayout::U128 => u128::deserialize(deserializer).map(MoveValue::U128),
            MoveTypeLayout::U256 => u256::U256::deserialize(deserializer).map(MoveValue::U256),
            MoveTypeLayout::Address => {
                AccountAddress::deserialize(deserializer).map(MoveValue::Address)
            }
            MoveTypeLayout::Signer => {
                AccountAddress::deserialize(deserializer).map(MoveValue::Signer)
            }
            MoveTypeLayout::Struct(ty) => Ok(MoveValue::Struct(ty.deserialize(deserializer)?)),
            MoveTypeLayout::Vector(layout) => Ok(MoveValue::Vector(
                deserializer.deserialize_seq(VectorElementVisitor(layout))?,
            )),
        }
    }
}

struct VectorElementVisitor<'a>(&'a MoveTypeLayout);

impl<'d, 'a> serde::de::Visitor<'d> for VectorElementVisitor<'a> {
    type Value = Vec<MoveValue>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Vector")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'d>,
    {
        let mut vals = Vec::new();
        while let Some(elem) = seq.next_element_seed(self.0)? {
            vals.push(elem)
        }
        Ok(vals)
    }
}

struct DecoratedStructFieldVisitor<'a>(&'a [MoveFieldLayout]);

impl<'d, 'a> serde::de::Visitor<'d> for DecoratedStructFieldVisitor<'a> {
    type Value = Vec<(Identifier, MoveValue)>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Struct")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'d>,
    {
        let mut vals = Vec::new();
        for (i, layout) in self.0.iter().enumerate() {
            match seq.next_element_seed(layout)? {
                Some(elem) => vals.push(elem),
                None => return Err(A::Error::invalid_length(i, &self)),
            }
        }
        Ok(vals)
    }
}

struct StructFieldVisitor<'a>(&'a [MoveTypeLayout]);

impl<'d, 'a> serde::de::Visitor<'d> for StructFieldVisitor<'a> {
    type Value = Vec<MoveValue>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Struct")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'d>,
    {
        let mut val = Vec::new();
        for (i, field_type) in self.0.iter().enumerate() {
            match seq.next_element_seed(field_type)? {
                Some(elem) => val.push(elem),
                None => return Err(A::Error::invalid_length(i, &self)),
            }
        }
        Ok(val)
    }
}

impl<'d> serde::de::DeserializeSeed<'d> for &MoveFieldLayout {
    type Value = (Identifier, MoveValue);

    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        Ok((self.name.clone(), self.layout.deserialize(deserializer)?))
    }
}

impl<'d> serde::de::DeserializeSeed<'d> for &MoveStructLayout {
    type Value = MoveStruct;

    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        match self {
            MoveStructLayout::Runtime(layout) => {
                let fields =
                    deserializer.deserialize_tuple(layout.len(), StructFieldVisitor(layout))?;
                Ok(MoveStruct::Runtime(fields))
            }
            MoveStructLayout::WithFields(layout) => {
                let fields = deserializer
                    .deserialize_tuple(layout.len(), DecoratedStructFieldVisitor(layout))?;
                Ok(MoveStruct::WithFields(fields))
            }
            MoveStructLayout::WithTypes {
                type_,
                fields: layout,
            } => {
                let fields = deserializer
                    .deserialize_tuple(layout.len(), DecoratedStructFieldVisitor(layout))?;
                Ok(MoveStruct::WithTypes {
                    type_: type_.clone(),
                    fields,
                })
            }
        }
    }
}

impl serde::Serialize for MoveValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            MoveValue::Struct(s) => s.serialize(serializer),
            MoveValue::Bool(b) => serializer.serialize_bool(*b),
            MoveValue::U8(i) => serializer.serialize_u8(*i),
            MoveValue::U16(i) => serializer.serialize_u16(*i),
            MoveValue::U32(i) => serializer.serialize_u32(*i),
            MoveValue::U64(i) => serializer.serialize_u64(*i),
            MoveValue::U128(i) => serializer.serialize_u128(*i),
            MoveValue::U256(i) => i.serialize(serializer),
            MoveValue::Address(a) => a.serialize(serializer),
            MoveValue::Signer(a) => a.serialize(serializer),
            MoveValue::Vector(v) => {
                let mut t = serializer.serialize_seq(Some(v.len()))?;
                for val in v {
                    t.serialize_element(val)?;
                }
                t.end()
            }
        }
    }
}

struct MoveFields<'a>(&'a [(Identifier, MoveValue)]);

impl<'a> serde::Serialize for MoveFields<'a> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut t = serializer.serialize_map(Some(self.0.len()))?;
        for (f, v) in self.0.iter() {
            t.serialize_entry(f, v)?;
        }
        t.end()
    }
}

impl serde::Serialize for MoveStruct {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Runtime(s) => {
                let mut t = serializer.serialize_tuple(s.len())?;
                for v in s.iter() {
                    t.serialize_element(v)?;
                }
                t.end()
            }
            Self::WithFields(fields) => MoveFields(fields).serialize(serializer),
            Self::WithTypes { type_, fields } => {
                // Serialize a Move struct as Serde struct type named `struct `with two fields named `type` and `fields`.
                // `fields` will get serialized as a Serde map.
                // Unfortunately, we can't serialize this in the logical way: as a Serde struct named `type` with a field for
                // each of `fields` because serde insists that struct and field names be `'static &str`'s
                let mut t = serializer.serialize_struct(MOVE_STRUCT_NAME, 2)?;
                // serialize type as string (e.g., 0x0::ModuleName::StructName<TypeArg1,TypeArg2>) instead of (e.g.
                // { address: 0x0...0, module: ModuleName, name: StructName, type_args: [TypeArg1, TypeArg2]})
                t.serialize_field(MOVE_STRUCT_TYPE, &type_.to_string())?;
                t.serialize_field(MOVE_STRUCT_FIELDS, &MoveFields(fields))?;
                t.end()
            }
        }
    }
}

impl fmt::Display for MoveFieldLayout {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}: {}", self.name, self.layout)
    }
}

impl fmt::Display for MoveTypeLayout {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        use MoveTypeLayout::*;
        match self {
            Bool => write!(f, "bool"),
            U8 => write!(f, "u8"),
            U16 => write!(f, "u16"),
            U32 => write!(f, "u32"),
            U64 => write!(f, "u64"),
            U128 => write!(f, "u128"),
            U256 => write!(f, "u256"),
            Address => write!(f, "address"),
            Vector(typ) => write!(f, "vector<{}>", typ),
            Struct(s) => write!(f, "{}", s),
            Signer => write!(f, "signer"),
        }
    }
}

impl fmt::Display for MoveStructLayout {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        write!(f, "{{ ")?;
        match self {
            Self::Runtime(layouts) => {
                for (i, l) in layouts.iter().enumerate() {
                    write!(f, "{}: {}, ", i, l)?
                }
            }
            Self::WithFields(layouts) => {
                for layout in layouts {
                    write!(f, "{}, ", layout)?
                }
            }
            Self::WithTypes { type_, fields } => {
                write!(f, "Type: {}", type_)?;
                write!(f, "Fields:")?;
                for field in fields {
                    write!(f, "{}, ", field)?
                }
            }
        }
        write!(f, "}}")
    }
}

impl TryInto<TypeTag> for &MoveTypeLayout {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<TypeTag, Self::Error> {
        Ok(match self {
            MoveTypeLayout::Address => TypeTag::Address,
            MoveTypeLayout::Bool => TypeTag::Bool,
            MoveTypeLayout::U8 => TypeTag::U8,
            MoveTypeLayout::U16 => TypeTag::U16,
            MoveTypeLayout::U32 => TypeTag::U32,
            MoveTypeLayout::U64 => TypeTag::U64,
            MoveTypeLayout::U128 => TypeTag::U128,
            MoveTypeLayout::U256 => TypeTag::U256,
            MoveTypeLayout::Signer => TypeTag::Signer,
            MoveTypeLayout::Vector(v) => {
                let inner_type = &**v;
                TypeTag::Vector(Box::new(inner_type.try_into()?))
            }
            MoveTypeLayout::Struct(v) => TypeTag::Struct(Box::new(v.try_into()?)),
        })
    }
}

impl TryInto<StructTag> for &MoveStructLayout {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<StructTag, Self::Error> {
        use MoveStructLayout::*;
        match self {
            Runtime(..) | WithFields(..) => bail!(
                "Invalid MoveTypeLayout -> StructTag conversion--needed MoveLayoutType::WithTypes"
            ),
            WithTypes { type_, .. } => Ok(type_.clone()),
        }
    }
}

impl fmt::Display for MoveValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MoveValue::U8(u) => write!(f, "{}u8", u),
            MoveValue::U16(u) => write!(f, "{}u16", u),
            MoveValue::U32(u) => write!(f, "{}u32", u),
            MoveValue::U64(u) => write!(f, "{}u64", u),
            MoveValue::U128(u) => write!(f, "{}u128", u),
            MoveValue::U256(u) => write!(f, "{}u256", u),
            MoveValue::Bool(false) => write!(f, "false"),
            MoveValue::Bool(true) => write!(f, "true"),
            MoveValue::Address(a) => write!(f, "{}", a.to_hex_literal()),
            MoveValue::Signer(a) => write!(f, "signer({})", a.to_hex_literal()),
            MoveValue::Vector(v) => fmt_list(f, "vector[", v, "]"),
            MoveValue::Struct(s) => fmt::Display::fmt(s, f),
        }
    }
}

impl fmt::Display for MoveStruct {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MoveStruct::Runtime(v) => fmt_list(f, "struct[", v, "]"),
            MoveStruct::WithFields(fields) => {
                fmt_list(f, "{", fields.iter().map(DisplayFieldBinding), "}")
            }
            MoveStruct::WithTypes { type_, fields } => {
                fmt::Display::fmt(type_, f)?;
                fmt_list(f, " {", fields.iter().map(DisplayFieldBinding), "}")
            }
        }
    }
}

struct DisplayFieldBinding<'a>(&'a (Identifier, MoveValue));

impl fmt::Display for DisplayFieldBinding<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let DisplayFieldBinding((field, value)) = self;
        write!(f, "{}: {}", field, value)
    }
}

fn fmt_list<T: fmt::Display>(
    f: &mut fmt::Formatter<'_>,
    begin: &str,
    items: impl IntoIterator<Item = T>,
    end: &str,
) -> fmt::Result {
    write!(f, "{}", begin)?;
    let mut items = items.into_iter();
    if let Some(x) = items.next() {
        write!(f, "{}", x)?;
        for x in items {
            write!(f, ", {}", x)?;
        }
    }
    write!(f, "{}", end)?;
    Ok(())
}
