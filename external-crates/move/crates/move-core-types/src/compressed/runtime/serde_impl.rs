// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::layout::*;
use crate::runtime_value::{MoveStruct, MoveValue, MoveVariant};
use crate::{VARIANT_TAG_MAX_VALUE, account_address::AccountAddress, u256};
use serde::{Deserialize, de::Error as _};

// -------------------------------------------------------------------------
// Deserialization — DeserializeSeed for &MoveLayoutView
// -------------------------------------------------------------------------

impl<'d> serde::de::DeserializeSeed<'d> for &MoveTypeLayout {
    type Value = MoveValue;

    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        match self.as_view() {
            MoveLayoutView::Bool => bool::deserialize(deserializer).map(MoveValue::Bool),
            MoveLayoutView::U8 => u8::deserialize(deserializer).map(MoveValue::U8),
            MoveLayoutView::U16 => u16::deserialize(deserializer).map(MoveValue::U16),
            MoveLayoutView::U32 => u32::deserialize(deserializer).map(MoveValue::U32),
            MoveLayoutView::U64 => u64::deserialize(deserializer).map(MoveValue::U64),
            MoveLayoutView::U128 => u128::deserialize(deserializer).map(MoveValue::U128),
            MoveLayoutView::U256 => u256::U256::deserialize(deserializer).map(MoveValue::U256),
            MoveLayoutView::Address => {
                AccountAddress::deserialize(deserializer).map(MoveValue::Address)
            }
            MoveLayoutView::Signer => {
                AccountAddress::deserialize(deserializer).map(MoveValue::Signer)
            }
            MoveLayoutView::Struct(fv) => {
                let fields = deserializer
                    .deserialize_tuple(fv.field_count(), CompressedStructFieldVisitor(&fv.0))?;
                Ok(MoveValue::Struct(MoveStruct(fields)))
            }
            MoveLayoutView::Enum(ev) => {
                let variant = deserializer.deserialize_tuple(2, CompressedEnumFieldVisitor(&ev))?;
                Ok(MoveValue::Variant(variant))
            }
            MoveLayoutView::Vector(vv) => Ok(MoveValue::Vector(
                deserializer.deserialize_seq(CompressedVectorVisitor(&vv))?,
            )),
        }
    }
}

struct CompressedVectorVisitor<'a>(&'a MoveTypeLayout);

impl<'d> serde::de::Visitor<'d> for CompressedVectorVisitor<'_> {
    type Value = Vec<MoveValue>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

struct CompressedStructFieldVisitor<'a>(&'a MoveFieldsLayout);

impl<'d> serde::de::Visitor<'d> for CompressedStructFieldVisitor<'_> {
    type Value = Vec<MoveValue>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Struct")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'d>,
    {
        let mut vals = Vec::new();
        for (i, field_view) in self.0.fields().enumerate() {
            match seq.next_element_seed(&field_view)? {
                Some(elem) => vals.push(elem),
                None => return Err(A::Error::invalid_length(i, &self)),
            }
        }
        Ok(vals)
    }
}

struct CompressedEnumFieldVisitor<'a>(&'a MoveEnumLayout);

impl<'d> serde::de::Visitor<'d> for CompressedEnumFieldVisitor<'_> {
    type Value = MoveVariant;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Enum")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'d>,
    {
        let tag = match seq.next_element::<u8>()? {
            Some(tag) if tag as u64 <= VARIANT_TAG_MAX_VALUE => tag as u16,
            Some(tag) => return Err(A::Error::invalid_length(tag as usize, &self)),
            None => return Err(A::Error::invalid_length(0, &self)),
        };

        let variant_fv = match self.0.variant(tag as usize) {
            Some(VariantLayout::Known(fv)) => fv,
            Some(VariantLayout::Unknown) => {
                return Err(A::Error::custom(format!(
                    "cannot deserialize variant {tag}: layout unknown"
                )));
            }
            None => return Err(A::Error::invalid_length(tag as usize, &self)),
        };

        let Some(fields) = seq.next_element_seed(CompressedVariantFieldSeed(variant_fv))? else {
            return Err(A::Error::invalid_length(1, &self));
        };

        Ok(MoveVariant { tag, fields })
    }
}

struct CompressedVariantFieldSeed<'a>(&'a MoveFieldsLayout);

impl<'d> serde::de::DeserializeSeed<'d> for CompressedVariantFieldSeed<'_> {
    type Value = Vec<MoveValue>;

    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_tuple(self.0.field_count(), CompressedStructFieldVisitor(self.0))
    }
}
