// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared OptionVisitor implementation for deserializing Move Option types.

use move_core_types::{
    account_address::AccountAddress,
    annotated_value::MoveTypeLayout,
    annotated_visitor::{StructDriver, ValueDriver, VariantDriver, VecDriver, Visitor},
    u256::U256,
};

use crate::base_types::RESOLVED_STD_OPTION;

/// Trait for visitor errors that can be used with OptionVisitor.
/// Requires the ability to create an "unexpected type" error.
pub trait OptionVisitorError: Sized {
    /// Create an error indicating an unexpected type was encountered.
    fn unexpected_type() -> Self;
}

/// A visitor that deserializes an `Option<T>` by interpreting an empty vector as `None` and a
/// single-element vector as `Some(T)`.
pub struct OptionVisitor<'a, T>(pub &'a mut T);

impl<'b, 'l, T, E> Visitor<'b, 'l> for OptionVisitor<'_, T>
where
    T: Visitor<'b, 'l, Error = E>,
    E: OptionVisitorError + From<move_core_types::annotated_visitor::Error>,
{
    type Value = Option<T::Value>;
    type Error = E;

    fn visit_vector(
        &mut self,
        driver: &mut VecDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        match driver.len() {
            0 => Ok(None),
            1 => driver.next_element(self.0),
            _ => Err(E::unexpected_type()),
        }
    }

    fn visit_struct(
        &mut self,
        driver: &mut StructDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        if is_option(driver.struct_layout()) {
            driver
                .next_field(self)?
                .ok_or_else(E::unexpected_type)
                .map(|(_, option)| option)
        } else {
            Err(E::unexpected_type())
        }
    }

    // === Empty/default cases ===

    fn visit_u8(&mut self, _: &ValueDriver<'_, 'b, 'l>, _: u8) -> Result<Self::Value, Self::Error> {
        Err(E::unexpected_type())
    }

    fn visit_u16(
        &mut self,
        _: &ValueDriver<'_, 'b, 'l>,
        _: u16,
    ) -> Result<Self::Value, Self::Error> {
        Err(E::unexpected_type())
    }

    fn visit_u32(
        &mut self,
        _: &ValueDriver<'_, 'b, 'l>,
        _: u32,
    ) -> Result<Self::Value, Self::Error> {
        Err(E::unexpected_type())
    }

    fn visit_u64(
        &mut self,
        _: &ValueDriver<'_, 'b, 'l>,
        _: u64,
    ) -> Result<Self::Value, Self::Error> {
        Err(E::unexpected_type())
    }

    fn visit_u128(
        &mut self,
        _: &ValueDriver<'_, 'b, 'l>,
        _: u128,
    ) -> Result<Self::Value, Self::Error> {
        Err(E::unexpected_type())
    }

    fn visit_u256(
        &mut self,
        _: &ValueDriver<'_, 'b, 'l>,
        _: U256,
    ) -> Result<Self::Value, Self::Error> {
        Err(E::unexpected_type())
    }

    fn visit_bool(
        &mut self,
        _: &ValueDriver<'_, 'b, 'l>,
        _: bool,
    ) -> Result<Self::Value, Self::Error> {
        Err(E::unexpected_type())
    }

    fn visit_address(
        &mut self,
        _: &ValueDriver<'_, 'b, 'l>,
        _: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        Err(E::unexpected_type())
    }

    fn visit_signer(
        &mut self,
        _: &ValueDriver<'_, 'b, 'l>,
        _: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        Err(E::unexpected_type())
    }

    fn visit_variant(
        &mut self,
        _: &mut VariantDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        Err(E::unexpected_type())
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
