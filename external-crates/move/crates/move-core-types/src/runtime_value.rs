// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    account_address::AccountAddress, annotated_value as A, fmt_list, u256, VARIANT_COUNT_MAX,
};
use anyhow::{anyhow, Result as AResult};
use move_proc_macros::test_variant_order;
use serde::{
    de::Error as DeError,
    ser::{SerializeSeq, SerializeTuple},
    Deserialize, Serialize,
};
use std::fmt::{self, Debug};

/// In the `WithTypes` configuration, a Move struct gets serialized into a Serde struct with this name
pub const MOVE_STRUCT_NAME: &str = "struct";

/// In the `WithTypes` configuration, a Move struct gets serialized into a Serde struct with this as the first field
pub const MOVE_STRUCT_TYPE: &str = "type";

/// In the `WithTypes` configuration, a Move struct gets serialized into a Serde struct with this as the second field
pub const MOVE_STRUCT_FIELDS: &str = "fields";

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct MoveStruct(pub Vec<MoveValue>);

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct MoveVariant {
    pub tag: u16,
    pub fields: Vec<MoveValue>,
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
    Variant(MoveVariant),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveStructLayout(pub Box<Vec<MoveTypeLayout>>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveEnumLayout(pub Box<Vec<Vec<MoveTypeLayout>>>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MoveDatatypeLayout {
    Struct(Box<MoveStructLayout>),
    Enum(Box<MoveEnumLayout>),
}

impl MoveDatatypeLayout {
    pub fn into_layout(self) -> MoveTypeLayout {
        match self {
            MoveDatatypeLayout::Struct(layout) => MoveTypeLayout::Struct(layout),
            MoveDatatypeLayout::Enum(layout) => MoveTypeLayout::Enum(layout),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[test_variant_order(src/unit_tests/staged_enum_variant_order/move_type_layout.yaml)]
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
    Struct(Box<MoveStructLayout>),
    #[serde(rename(serialize = "signer", deserialize = "signer"))]
    Signer,

    // NOTE: Added in bytecode version v6, do not reorder!
    #[serde(rename(serialize = "u16", deserialize = "u16"))]
    U16,
    #[serde(rename(serialize = "u32", deserialize = "u32"))]
    U32,
    #[serde(rename(serialize = "u256", deserialize = "u256"))]
    U256,
    #[serde(rename(serialize = "enum", deserialize = "enum"))]
    Enum(Box<MoveEnumLayout>),
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

    pub fn decorate(self, layout: &A::MoveTypeLayout) -> A::MoveValue {
        match (self, layout) {
            (MoveValue::Struct(s), A::MoveTypeLayout::Struct(l)) => {
                A::MoveValue::Struct(s.decorate(l))
            }
            (MoveValue::Variant(s), A::MoveTypeLayout::Enum(l)) => {
                A::MoveValue::Variant(s.decorate(l))
            }
            (MoveValue::Vector(vals), A::MoveTypeLayout::Vector(t)) => {
                A::MoveValue::Vector(vals.into_iter().map(|v| v.decorate(t)).collect())
            }
            (MoveValue::U8(a), _) => A::MoveValue::U8(a),
            (MoveValue::U64(u), _) => A::MoveValue::U64(u),
            (MoveValue::U128(u), _) => A::MoveValue::U128(u),
            (MoveValue::Bool(b), _) => A::MoveValue::Bool(b),
            (MoveValue::Address(a), _) => A::MoveValue::Address(a),
            (MoveValue::Signer(a), _) => A::MoveValue::Signer(a),
            (MoveValue::U16(u), _) => A::MoveValue::U16(u),
            (MoveValue::U32(u), _) => A::MoveValue::U32(u),
            (MoveValue::U256(u), _) => A::MoveValue::U256(u),
            _ => panic!("Invalid decoration"),
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
        Self(value)
    }

    pub fn simple_deserialize(blob: &[u8], ty: &MoveStructLayout) -> AResult<Self> {
        Ok(bcs::from_bytes_seed(ty, blob)?)
    }

    pub fn decorate(self, layout: &A::MoveStructLayout) -> A::MoveStruct {
        let MoveStruct(vals) = self;
        let A::MoveStructLayout { type_, fields } = layout;
        A::MoveStruct {
            type_: type_.clone(),
            fields: vals
                .into_iter()
                .zip(fields.iter())
                .map(|(v, l)| (l.name.clone(), v.decorate(&l.layout)))
                .collect(),
        }
    }

    pub fn fields(&self) -> &[MoveValue] {
        &self.0
    }

    pub fn into_fields(self) -> Vec<MoveValue> {
        self.0
    }
}

impl MoveVariant {
    pub fn new(tag: u16, fields: Vec<MoveValue>) -> Self {
        Self { tag, fields }
    }

    pub fn simple_deserialize(blob: &[u8], ty: &MoveEnumLayout) -> AResult<Self> {
        Ok(bcs::from_bytes_seed(ty, blob)?)
    }

    pub fn decorate(self, layout: &A::MoveEnumLayout) -> A::MoveVariant {
        let MoveVariant { tag, fields } = self;
        let A::MoveEnumLayout { type_, variants } = layout;
        let ((v_name, _), v_layout) = variants
            .iter()
            .find(|((_, v_tag), _)| *v_tag == tag)
            .unwrap();
        A::MoveVariant {
            type_: type_.clone(),
            tag,
            fields: fields
                .into_iter()
                .zip(v_layout.iter())
                .map(|(v, l)| (l.name.clone(), v.decorate(&l.layout)))
                .collect(),
            variant_name: v_name.clone(),
        }
    }

    pub fn fields(&self) -> &[MoveValue] {
        &self.fields
    }

    pub fn into_fields(self) -> Vec<MoveValue> {
        self.fields
    }
}

impl MoveStructLayout {
    pub fn new(types: Vec<MoveTypeLayout>) -> Self {
        Self(Box::new(types))
    }

    pub fn fields(&self) -> &[MoveTypeLayout] {
        &self.0
    }

    pub fn into_fields(self) -> Vec<MoveTypeLayout> {
        *self.0
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
            MoveTypeLayout::Enum(ty) => Ok(MoveValue::Variant(ty.deserialize(deserializer)?)),
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

impl<'d> serde::de::DeserializeSeed<'d> for &MoveStructLayout {
    type Value = MoveStruct;

    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        Ok(MoveStruct(deserializer.deserialize_tuple(
            self.0.len(),
            StructFieldVisitor(&self.0),
        )?))
    }
}

impl<'d> serde::de::DeserializeSeed<'d> for &MoveEnumLayout {
    type Value = MoveVariant;
    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_tuple(2, EnumFieldVisitor(&self.0))
    }
}

struct EnumFieldVisitor<'a>(&'a Vec<Vec<MoveTypeLayout>>);

impl<'d, 'a> serde::de::Visitor<'d> for EnumFieldVisitor<'a> {
    type Value = MoveVariant;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Enum")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'d>,
    {
        let tag = match seq.next_element_seed(&MoveTypeLayout::U8)? {
            Some(MoveValue::U8(tag)) if tag as u64 <= VARIANT_COUNT_MAX => tag as u16,
            Some(MoveValue::U8(tag)) => return Err(A::Error::invalid_length(tag as usize, &self)),
            Some(val) => {
                return Err(A::Error::invalid_type(
                    serde::de::Unexpected::Other(&format!("{val:?}")),
                    &self,
                ))
            }
            None => return Err(A::Error::invalid_length(0, &self)),
        };

        let Some(variant_layout) = self.0.get(tag as usize) else {
            return Err(A::Error::invalid_length(tag as usize, &self));
        };

        let Some(fields) = seq.next_element_seed(&MoveVariantFieldLayout(variant_layout))? else {
            return Err(A::Error::invalid_length(1, &self));
        };

        Ok(MoveVariant { tag, fields })
    }
}

struct MoveVariantFieldLayout<'a>(&'a [MoveTypeLayout]);

impl<'d, 'a> serde::de::DeserializeSeed<'d> for &MoveVariantFieldLayout<'a> {
    type Value = Vec<MoveValue>;

    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_tuple(self.0.len(), StructFieldVisitor(self.0))
    }
}

impl serde::Serialize for MoveValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            MoveValue::Struct(s) => s.serialize(serializer),
            MoveValue::Variant(v) => v.serialize(serializer),
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

impl serde::Serialize for MoveStruct {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut t = serializer.serialize_tuple(self.0.len())?;
        for v in self.0.iter() {
            t.serialize_element(v)?;
        }
        t.end()
    }
}

impl serde::Serialize for MoveVariant {
    // Serialize a variant as:  (tag, [fields...])
    // Since we restrict tags to be less than or equal to 127, the tag will always be a single byte
    // in uleb encoding and we don't actually need to uleb encode it, but we can at a later date if
    // we want/need to.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let tag = if self.tag as u64 > VARIANT_COUNT_MAX {
            return Err(serde::ser::Error::custom(format!(
                "Variant tag {} is greater than the maximum allowed value of {}",
                self.tag, VARIANT_COUNT_MAX
            )));
        } else {
            self.tag as u8
        };

        let mut t = serializer.serialize_tuple(2)?;

        t.serialize_element(&tag)?;
        t.serialize_element(&MoveFields(&self.fields))?;

        t.end()
    }
}

struct MoveFields<'a>(&'a [MoveValue]);

impl<'a> serde::Serialize for MoveFields<'a> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut t = serializer.serialize_tuple(self.0.len())?;
        for v in self.0.iter() {
            t.serialize_element(v)?;
        }
        t.end()
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
            Signer => write!(f, "signer"),
            Vector(typ) if f.alternate() => write!(f, "vector<{typ:#}>"),
            Vector(typ) => write!(f, "vector<{typ}>"),
            Struct(s) if f.alternate() => write!(f, "{s:#}"),
            Struct(s) => write!(f, "{s}"),
            Enum(e) if f.alternate() => write!(f, "{e:#}"),
            Enum(e) => write!(f, "{e}"),
        }
    }
}

/// Helper type that uses `T`'s `Display` implementation as its own `Debug` implementation, to allow
/// other `Display` implementations in this module to take advantage of the structured formatting
/// helpers that Rust uses for its own debug types.
struct DebugAsDisplay<'a, T>(&'a T);
impl<'a, T: fmt::Display> fmt::Debug for DebugAsDisplay<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "{:#}", self.0)
        } else {
            write!(f, "{}", self.0)
        }
    }
}

impl fmt::Display for MoveStructLayout {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        use DebugAsDisplay as DD;

        write!(f, "struct ")?;
        let mut map = f.debug_map();
        for (i, l) in self.0.iter().enumerate() {
            map.entry(&i, &DD(&l));
        }

        map.finish()
    }
}

impl fmt::Display for MoveEnumLayout {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        write!(f, "enum ")?;
        for (tag, variant) in self.0.iter().enumerate() {
            write!(f, "variant_tag: {} {{ ", tag)?;
            for (i, l) in variant.iter().enumerate() {
                write!(f, "{}: {}, ", i, l)?
            }
            write!(f, " }} ")?;
        }
        Ok(())
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
            MoveValue::Variant(v) => fmt::Display::fmt(v, f),
        }
    }
}

impl fmt::Display for MoveStruct {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_list(f, "struct[", &self.0, "]")
    }
}

impl fmt::Display for MoveVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_list(
            f,
            &format!("variant(tag = {})[", self.tag),
            &self.fields,
            "]",
        )
    }
}
