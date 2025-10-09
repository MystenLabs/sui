// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Write as _;

use move_core_types::account_address::AccountAddress;
use move_core_types::annotated_value as A;
use move_core_types::annotated_visitor as AV;
use move_core_types::u256::U256;
use sui_types::base_types::{move_ascii_str_layout, move_utf8_str_layout, url_layout};
use sui_types::id::ID;
use sui_types::id::UID;

use super::error::FormatError;
use super::value::Slice;
use super::writer::BoundedWriter;

/// A visitor that writes a formatted string representation of a Move value into a bounded string
/// buffer.
pub(crate) struct FormatVisitor<'w, 'u>(&'w mut BoundedWriter<'u>);

impl<'w, 'u> FormatVisitor<'w, 'u> {
    /// Attempt to extract a value from the given slice, following the provided path of accessors.
    ///
    /// Accessors are expected to be in reverse order, i.e. the last accessor in the vector is
    /// applied first and are consumed as they are successfully applied.
    pub(crate) fn deserialize_slice(
        slice: Slice<'_>,
        writer: &'w mut BoundedWriter<'u>,
    ) -> Result<(), FormatError> {
        A::MoveValue::visit_deserialize(slice.bytes, slice.layout, &mut Self(writer))
    }
}

impl<'v> AV::Visitor<'v, 'v> for FormatVisitor<'_, '_> {
    type Value = ();
    type Error = FormatError;

    fn visit_u8(&mut self, _: &AV::ValueDriver<'_, 'v, 'v>, n: u8) -> Result<(), FormatError> {
        Ok(write!(self.0, "{n}")?)
    }

    fn visit_u16(&mut self, _: &AV::ValueDriver<'_, 'v, 'v>, n: u16) -> Result<(), FormatError> {
        Ok(write!(self.0, "{n}")?)
    }

    fn visit_u32(&mut self, _: &AV::ValueDriver<'_, 'v, 'v>, n: u32) -> Result<(), FormatError> {
        Ok(write!(self.0, "{n}")?)
    }

    fn visit_u64(&mut self, _: &AV::ValueDriver<'_, 'v, 'v>, n: u64) -> Result<(), FormatError> {
        Ok(write!(self.0, "{n}")?)
    }

    fn visit_u128(&mut self, _: &AV::ValueDriver<'_, 'v, 'v>, n: u128) -> Result<(), FormatError> {
        Ok(write!(self.0, "{n}")?)
    }

    fn visit_u256(&mut self, _: &AV::ValueDriver<'_, 'v, 'v>, n: U256) -> Result<(), FormatError> {
        Ok(write!(self.0, "{n}")?)
    }

    fn visit_bool(&mut self, _: &AV::ValueDriver<'_, 'v, 'v>, b: bool) -> Result<(), FormatError> {
        Ok(write!(self.0, "{b}")?)
    }

    fn visit_address(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        a: AccountAddress,
    ) -> Result<(), FormatError> {
        Ok(write!(self.0, "{}", a.to_canonical_display(true))?)
    }

    fn visit_signer(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        s: AccountAddress,
    ) -> Result<(), FormatError> {
        Ok(write!(self.0, "{}", s.to_canonical_display(true))?)
    }

    fn visit_vector(&mut self, _: &mut AV::VecDriver<'_, 'v, 'v>) -> Result<(), FormatError> {
        Err(FormatError::TransformInvalid("str", "vectors"))
    }

    fn visit_struct(
        &mut self,
        driver: &mut AV::StructDriver<'_, 'v, 'v>,
    ) -> Result<(), FormatError> {
        let layout = driver.struct_layout();

        // Special case representation for common types.
        if layout == &move_ascii_str_layout()
            || layout == &move_utf8_str_layout()
            || layout == &url_layout()
        {
            // 0x1::ascii::String, 0x1::string::String, 0x2::url::Url
            driver.skip_field()?;

            let bytes = &driver.bytes()[driver.start()..driver.position()];
            let s: &str = bcs::from_bytes(bytes).map_err(|_| FormatError::InvalidBytes)?;

            Ok(self.0.write_str(s)?)
        } else if layout == &UID::layout() || layout == &ID::layout() {
            // 0x2::object::UID, 0x2::object::ID
            driver.skip_field()?;

            let bytes = &driver.bytes()[driver.start()..driver.position()];
            let id = AccountAddress::from_bytes(bytes).map_err(|_| FormatError::InvalidBytes)?;

            write!(self.0, "{}", id.to_canonical_display(true))?;
            Ok(())
        } else {
            Err(FormatError::TransformInvalid("str", "structs"))
        }
    }

    fn visit_variant(&mut self, _: &mut AV::VariantDriver<'_, 'v, 'v>) -> Result<(), FormatError> {
        Err(FormatError::TransformInvalid("str", "enums"))
    }
}
