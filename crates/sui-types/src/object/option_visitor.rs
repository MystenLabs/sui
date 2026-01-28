// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared OptionVisitor implementation for deserializing Move Option types.

use move_core_types::{
    account_address::AccountAddress, annotated_value::MoveTypeLayout, annotated_visitor as AV,
    u256::U256, visitor_default,
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

    visitor_default! { <'b, 'l> u8, u16, u32, u64, u128, u256 = Err(Error.into()) }
    visitor_default! { <'b, 'l> bool, address, signer, variant = Err(Error.into()) }

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
