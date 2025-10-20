// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;
use move_core_types::annotated_value as A;
use move_core_types::annotated_visitor as AV;
use move_core_types::u256::U256;
use sui_types::base_types::RESOLVED_STD_OPTION;

use crate::v2::value::Slice;

/// Visitor to call on the fields of an `Option<T>` value to extract it as an `Option<Slice<'_>>`
/// (i.e. converts the Move notion of an optional value into a Rust `Option`).
pub(crate) struct OptionExtractor;

impl<'v> AV::Visitor<'v, 'v> for OptionExtractor {
    type Value = Option<Slice<'v>>;
    type Error = AV::Error;

    fn visit_vector(
        &mut self,
        driver: &mut AV::VecDriver<'_, 'v, 'v>,
    ) -> Result<Self::Value, AV::Error> {
        if driver.len() != 1 {
            return Ok(None);
        }

        let start = driver.position();
        driver.skip_element()?;
        let layout = driver.element_layout();
        let bytes = &driver.bytes()[start..driver.position()];
        Ok(Some(Slice { layout, bytes }))
    }

    // All the other variants are guaranteed not to contain an optional value.

    fn visit_u8(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        _: u8,
    ) -> Result<Self::Value, AV::Error> {
        Ok(None)
    }

    fn visit_u16(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        _: u16,
    ) -> Result<Self::Value, AV::Error> {
        Ok(None)
    }

    fn visit_u32(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        _: u32,
    ) -> Result<Self::Value, AV::Error> {
        Ok(None)
    }

    fn visit_u64(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        _: u64,
    ) -> Result<Self::Value, AV::Error> {
        Ok(None)
    }

    fn visit_u128(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        _: u128,
    ) -> Result<Self::Value, AV::Error> {
        Ok(None)
    }

    fn visit_u256(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        _: U256,
    ) -> Result<Self::Value, AV::Error> {
        Ok(None)
    }

    fn visit_bool(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        _: bool,
    ) -> Result<Self::Value, AV::Error> {
        Ok(None)
    }

    fn visit_address(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        _: AccountAddress,
    ) -> Result<Self::Value, AV::Error> {
        Ok(None)
    }

    fn visit_signer(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        _: AccountAddress,
    ) -> Result<Self::Value, AV::Error> {
        Ok(None)
    }

    fn visit_struct(
        &mut self,
        _: &mut AV::StructDriver<'_, 'v, 'v>,
    ) -> Result<Self::Value, AV::Error> {
        Ok(None)
    }

    fn visit_variant(
        &mut self,
        _: &mut AV::VariantDriver<'_, 'v, 'v>,
    ) -> Result<Self::Value, AV::Error> {
        Ok(None)
    }
}

/// Checks whether the given layout is an `Option<T>`.
pub(crate) fn is_option(layout: &A::MoveStructLayout) -> bool {
    let ty = &layout.type_;

    if (&ty.address, ty.module.as_ref(), ty.name.as_ref()) != RESOLVED_STD_OPTION {
        return false;
    }

    if ty.type_params.len() != 1 {
        return false;
    }

    let Some(type_param) = ty.type_params.first() else {
        return false;
    };

    if layout.fields.len() != 1 {
        return false;
    }

    let Some(field) = layout.fields.first() else {
        return false;
    };

    if field.name.as_str() != "vec" {
        return false;
    }

    matches!(&field.layout, A::MoveTypeLayout::Vector(elem) if elem.is_type(type_param))
}
