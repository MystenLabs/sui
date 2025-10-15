// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;
use move_core_types::annotated_visitor as AV;
use move_core_types::language_storage::StructTag;
use move_core_types::u256::U256;
use sui_types::SUI_FRAMEWORK_ADDRESS;
use sui_types::base_types::{VEC_MAP_ENTRY_STRUCT_NAME, VEC_MAP_MODULE_NAME, VEC_MAP_STRUCT_NAME};

use crate::v2::error::FormatError;
use crate::v2::value::{Accessor, Value};
use crate::v2::visitor::extractor::Extractor;

/// A visitor that looks for a specific key in a `0x2::vec_map::VecMap<_, _>` and continues
/// extraction using the provided path from its corresponding value.
///
/// The key is provided in its raw form (serialized as BCS).
pub(crate) struct VecMapVisitor<'v, 'p> {
    key: Vec<u8>,
    path: &'p mut Vec<Accessor<'v>>,
}

impl<'v, 'p> VecMapVisitor<'v, 'p> {
    pub(crate) fn new(key: Vec<u8>, path: &'p mut Vec<Accessor<'v>>) -> Self {
        Self { key, path }
    }
}

impl<'v> AV::Visitor<'v, 'v> for VecMapVisitor<'v, '_> {
    type Value = Option<Value<'v>>;
    type Error = FormatError;

    /// Expect to visit the content vector of the VecMap first. Look through each entry for one
    /// with a matching key, and continue visiting the value for that entry.
    fn visit_vector(
        &mut self,
        driver: &mut AV::VecDriver<'_, 'v, 'v>,
    ) -> Result<Self::Value, Self::Error> {
        while let Some(v) = driver.next_element(self)? {
            if let Some(v) = v {
                return Ok(Some(v));
            }
        }

        Ok(None)
    }

    /// Entries of the VecMap are structs, with a `key` field, followed by a `value` field.
    fn visit_struct(
        &mut self,
        driver: &mut AV::StructDriver<'_, 'v, 'v>,
    ) -> Result<Self::Value, Self::Error> {
        // Must be a `0x2::vec_map::Entry<_, _>`.
        if !is_vec_map_entry(&driver.struct_layout().type_) {
            return Ok(None);
        }

        // First field must be `key`.
        let key = driver.skip_field()?;
        if key.is_none_or(|f| f.name.as_str() != "key") {
            return Ok(None);
        }

        // Whose bytes match the key we're looking for.
        let bytes = &driver.bytes()[driver.start()..driver.position()];
        if self.key != bytes {
            return Ok(None);
        }

        // Second field must be `value`.
        let value = driver.peek_field();
        if value.is_none_or(|f| f.name.as_str() != "value") {
            return Ok(None);
        }

        // Continue extracting the corresponding value at the provided path.
        let Some((_, Some(value))) = driver.next_field(&mut Extractor::new(self.path))? else {
            return Ok(None);
        };

        // Confirm the struct has no more fields.
        if driver.peek_field().is_some() {
            return Ok(None);
        }

        Ok(Some(value))
    }

    // All other Move Value variants can be ignored.

    fn visit_u8(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        _: u8,
    ) -> Result<Self::Value, Self::Error> {
        Ok(None)
    }

    fn visit_u16(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        _: u16,
    ) -> Result<Self::Value, Self::Error> {
        Ok(None)
    }

    fn visit_u32(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        _: u32,
    ) -> Result<Self::Value, Self::Error> {
        Ok(None)
    }

    fn visit_u64(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        _: u64,
    ) -> Result<Self::Value, Self::Error> {
        Ok(None)
    }

    fn visit_u128(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        _: u128,
    ) -> Result<Self::Value, Self::Error> {
        Ok(None)
    }

    fn visit_u256(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        _: U256,
    ) -> Result<Self::Value, Self::Error> {
        Ok(None)
    }

    fn visit_bool(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        _: bool,
    ) -> Result<Self::Value, Self::Error> {
        Ok(None)
    }

    fn visit_address(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        _: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        Ok(None)
    }

    fn visit_signer(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        _: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        Ok(None)
    }

    fn visit_variant(
        &mut self,
        _: &mut AV::VariantDriver<'_, 'v, 'v>,
    ) -> Result<Self::Value, Self::Error> {
        Ok(None)
    }
}

pub(crate) fn is_vec_map(tag: &StructTag) -> bool {
    tag.address == SUI_FRAMEWORK_ADDRESS
        && tag.module.as_ref() == VEC_MAP_MODULE_NAME
        && tag.name.as_ref() == VEC_MAP_STRUCT_NAME
        && tag.type_params.len() == 2
}

pub(crate) fn is_vec_map_entry(tag: &StructTag) -> bool {
    tag.address == SUI_FRAMEWORK_ADDRESS
        && tag.module.as_ref() == VEC_MAP_MODULE_NAME
        && tag.name.as_ref() == VEC_MAP_ENTRY_STRUCT_NAME
        && tag.type_params.len() == 2
}
