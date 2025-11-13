// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared OptionVisitor implementation for deserializing Move Option types.

use move_core_types::{
    account_address::AccountAddress, annotated_value::MoveTypeLayout, annotated_visitor as AV,
    u256::U256,
};

use crate::base_types::RESOLVED_STD_OPTION;

/// Error type for OptionVisitor.
#[derive(thiserror::Error, Debug)]
#[error("Unexpected type")]
pub struct Error;

/// A visitor that deserializes an `Option<T>` by interpreting an empty vector as `None` and a
/// single-element vector as `Some(T)`.
pub struct OptionVisitor<'a, T>(pub &'a mut T);

impl<'b, 'l, T, E> AV::Visitor<'b, 'l> for OptionVisitor<'_, T>
where
    T: AV::Visitor<'b, 'l, Error = E>,
    E: From<Error> + From<AV::Error>,
{
    type Value = Option<T::Value>;
    type Error = E;

    fn visit_vector(
        &mut self,
        driver: &mut AV::VecDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        match driver.len() {
            0 => Ok(None),
            1 => driver.next_element(self.0),
            _ => Err(Error.into()),
        }
    }

    fn visit_struct(
        &mut self,
        driver: &mut AV::StructDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        if is_option(driver.struct_layout()) {
            driver
                .next_field(self)?
                .ok_or_else(|| Error.into())
                .map(|(_, option)| option)
        } else {
            Err(Error.into())
        }
    }

    // === Empty/default cases ===

    fn visit_u8(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        _: u8,
    ) -> Result<Self::Value, Self::Error> {
        Err(Error.into())
    }

    fn visit_u16(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        _: u16,
    ) -> Result<Self::Value, Self::Error> {
        Err(Error.into())
    }

    fn visit_u32(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        _: u32,
    ) -> Result<Self::Value, Self::Error> {
        Err(Error.into())
    }

    fn visit_u64(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        _: u64,
    ) -> Result<Self::Value, Self::Error> {
        Err(Error.into())
    }

    fn visit_u128(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        _: u128,
    ) -> Result<Self::Value, Self::Error> {
        Err(Error.into())
    }

    fn visit_u256(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        _: U256,
    ) -> Result<Self::Value, Self::Error> {
        Err(Error.into())
    }

    fn visit_bool(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        _: bool,
    ) -> Result<Self::Value, Self::Error> {
        Err(Error.into())
    }

    fn visit_address(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        _: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        Err(Error.into())
    }

    fn visit_signer(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        _: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        Err(Error.into())
    }

    fn visit_variant(
        &mut self,
        _: &mut AV::VariantDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        Err(Error.into())
    }
}

/// Check if a struct layout represents a Move Option type.
fn is_option(struct_layout: &move_core_types::annotated_value::MoveStructLayout) -> bool {
    let ty = &struct_layout.type_;

    if (&ty.address, ty.module.as_ref(), ty.name.as_ref()) != RESOLVED_STD_OPTION {
        return false;
    }

    if ty.type_params.len() != 1 {
        return false;
    }

    let Some(type_param) = ty.type_params.first() else {
        return false;
    };

    if struct_layout.fields.len() != 1 {
        return false;
    }

    let Some(field) = struct_layout.fields.first() else {
        return false;
    };

    if field.name.as_str() != "vec" {
        return false;
    }

    match &field.layout {
        MoveTypeLayout::Vector(elem) => {
            if !elem.is_type(type_param) {
                return false;
            }
        }
        _ => return false,
    }

    true
}
