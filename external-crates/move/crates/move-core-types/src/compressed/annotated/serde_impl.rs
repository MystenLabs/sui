// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::layout::*;
use crate::annotated_value::{
    MoveStruct as AnnStruct, MoveValue as AnnValue, MoveVariant as AnnVariant,
};
use crate::compressed::VariantTag;
use crate::identifier::Identifier;
use crate::{VARIANT_TAG_MAX_VALUE, account_address::AccountAddress, u256};
use serde::{Deserialize, de::Error as _};

// -------------------------------------------------------------------------
// Deserialization — DeserializeSeed for &MoveTypeLayout
// -------------------------------------------------------------------------

impl<'d> serde::de::DeserializeSeed<'d> for &MoveTypeLayout {
    type Value = AnnValue;

    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        match self.as_view() {
            MoveLayoutView::Bool => bool::deserialize(deserializer).map(AnnValue::Bool),
            MoveLayoutView::U8 => u8::deserialize(deserializer).map(AnnValue::U8),
            MoveLayoutView::U16 => u16::deserialize(deserializer).map(AnnValue::U16),
            MoveLayoutView::U32 => u32::deserialize(deserializer).map(AnnValue::U32),
            MoveLayoutView::U64 => u64::deserialize(deserializer).map(AnnValue::U64),
            MoveLayoutView::U128 => u128::deserialize(deserializer).map(AnnValue::U128),
            MoveLayoutView::U256 => u256::U256::deserialize(deserializer).map(AnnValue::U256),
            MoveLayoutView::Address => {
                AccountAddress::deserialize(deserializer).map(AnnValue::Address)
            }
            MoveLayoutView::Signer => {
                AccountAddress::deserialize(deserializer).map(AnnValue::Signer)
            }
            MoveLayoutView::Struct(sv) => {
                let fields = deserializer.deserialize_tuple(
                    sv.field_count(),
                    CompressedStructFieldVisitor(&sv.fields),
                )?;
                Ok(AnnValue::Struct(AnnStruct {
                    type_: sv.type_().clone(),
                    fields,
                }))
            }
            MoveLayoutView::Enum(ev) => {
                let (variant_name, tag, fields) =
                    deserializer.deserialize_tuple(2, CompressedEnumFieldVisitor(&ev))?;
                Ok(AnnValue::Variant(AnnVariant {
                    type_: ev.type_().clone(),
                    variant_name,
                    tag,
                    fields,
                }))
            }
            MoveLayoutView::Vector(vv) => Ok(AnnValue::Vector(
                deserializer.deserialize_seq(CompressedVectorVisitor(&vv))?,
            )),
        }
    }
}

struct CompressedVectorVisitor<'a>(&'a MoveTypeLayout);

impl<'d> serde::de::Visitor<'d> for CompressedVectorVisitor<'_> {
    type Value = Vec<AnnValue>;

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
    type Value = Vec<(Identifier, AnnValue)>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Struct")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'d>,
    {
        let mut vals = Vec::new();
        for (i, (name, field_layout)) in self.0.fields().enumerate() {
            match seq.next_element_seed(&field_layout)? {
                Some(val) => vals.push((name.clone(), val)),
                None => return Err(A::Error::invalid_length(i, &self)),
            }
        }
        Ok(vals)
    }
}

struct CompressedEnumFieldVisitor<'a>(&'a MoveEnumLayout);

impl<'d> serde::de::Visitor<'d> for CompressedEnumFieldVisitor<'_> {
    type Value = (Identifier, VariantTag, Vec<(Identifier, AnnValue)>);

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Enum")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'d>,
    {
        let tag = match seq.next_element::<u8>()? {
            Some(tag) if tag as u64 <= VARIANT_TAG_MAX_VALUE => tag as VariantTag,
            Some(tag) => return Err(A::Error::invalid_length(tag as usize, &self)),
            None => return Err(A::Error::invalid_length(0, &self)),
        };

        let vl = match self.0.variant_by_tag(tag) {
            Some(vl) => vl,
            None => return Err(A::Error::invalid_length(tag as usize, &self)),
        };

        let fields_layout = match vl.fields() {
            Some(fields) => fields,
            None => {
                return Err(A::Error::custom(format!(
                    "cannot deserialize variant {tag}: layout unknown"
                )));
            }
        };

        let Some(fields) = seq.next_element_seed(CompressedVariantFieldSeed(fields_layout))? else {
            return Err(A::Error::invalid_length(1, &self));
        };

        Ok((vl.name().clone(), tag, fields))
    }
}

struct CompressedVariantFieldSeed<'a>(&'a MoveFieldsLayout);

impl<'d> serde::de::DeserializeSeed<'d> for CompressedVariantFieldSeed<'_> {
    type Value = Vec<(Identifier, AnnValue)>;

    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_tuple(self.0.field_count(), CompressedStructFieldVisitor(self.0))
    }
}
