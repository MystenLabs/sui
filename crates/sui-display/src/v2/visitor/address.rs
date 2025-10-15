// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;
use move_core_types::annotated_value as A;
use move_core_types::annotated_visitor as AV;
use move_core_types::u256::U256;
use sui_types::id::ID;
use sui_types::id::UID;

use crate::v2::error::FormatError;

/// Visitor to extract addresses from Objects, UIDs, IDs, or addresses.
pub(crate) struct AddressExtractor;

impl AV::Visitor<'_, '_> for AddressExtractor {
    type Value = Option<AccountAddress>;
    type Error = FormatError;

    fn visit_address(
        &mut self,
        _: &AV::ValueDriver<'_, '_, '_>,
        a: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        Ok(Some(a))
    }

    fn visit_struct(
        &mut self,
        driver: &mut AV::StructDriver<'_, '_, '_>,
    ) -> Result<Self::Value, Self::Error> {
        let id = ID::layout();
        let uid = UID::layout();
        let layout = driver.struct_layout();

        // Detect an inline `ID` or `UID`.
        if layout == &id || layout == &uid {
            return extract_address(driver).map(Some);
        }

        // Otherwise assume the value is an Object and look for `id: UID` as its first field.
        let Some(field) = driver.peek_field() else {
            return Ok(None);
        };

        if field.name.as_str() != "id" {
            return Ok(None);
        }

        let A::MoveTypeLayout::Struct(field) = &field.layout else {
            return Ok(None);
        };

        if field.as_ref() != &uid {
            return Ok(None);
        }

        extract_address(driver).map(Some)
    }

    // All the other variants are guaranteed not to include an address, return `None` for all of
    // them.

    fn visit_u8(
        &mut self,
        _: &AV::ValueDriver<'_, '_, '_>,
        _: u8,
    ) -> Result<Self::Value, Self::Error> {
        Ok(None)
    }

    fn visit_u16(
        &mut self,
        _: &AV::ValueDriver<'_, '_, '_>,
        _: u16,
    ) -> Result<Self::Value, Self::Error> {
        Ok(None)
    }

    fn visit_u32(
        &mut self,
        _: &AV::ValueDriver<'_, '_, '_>,
        _: u32,
    ) -> Result<Self::Value, Self::Error> {
        Ok(None)
    }

    fn visit_u64(
        &mut self,
        _: &AV::ValueDriver<'_, '_, '_>,
        _: u64,
    ) -> Result<Self::Value, Self::Error> {
        Ok(None)
    }

    fn visit_u128(
        &mut self,
        _: &AV::ValueDriver<'_, '_, '_>,
        _: u128,
    ) -> Result<Self::Value, Self::Error> {
        Ok(None)
    }

    fn visit_u256(
        &mut self,
        _: &AV::ValueDriver<'_, '_, '_>,
        _: U256,
    ) -> Result<Self::Value, Self::Error> {
        Ok(None)
    }

    fn visit_bool(
        &mut self,
        _: &AV::ValueDriver<'_, '_, '_>,
        _: bool,
    ) -> Result<Self::Value, Self::Error> {
        Ok(None)
    }

    fn visit_signer(
        &mut self,
        _: &AV::ValueDriver<'_, '_, '_>,
        _: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        Ok(None)
    }

    fn visit_vector(
        &mut self,
        _: &mut AV::VecDriver<'_, '_, '_>,
    ) -> Result<Self::Value, Self::Error> {
        Ok(None)
    }

    fn visit_variant(
        &mut self,
        _: &mut AV::VariantDriver<'_, '_, '_>,
    ) -> Result<Self::Value, Self::Error> {
        Ok(None)
    }
}

/// Attempts to extract an address from the prefix of the struct being visited. Assumes the driver
/// is pointed at the first field of the struct on entry.
fn extract_address(
    driver: &mut AV::StructDriver<'_, '_, '_>,
) -> Result<AccountAddress, FormatError> {
    driver.skip_field()?;
    let bytes = &driver.bytes()[driver.start()..driver.position()];
    AccountAddress::from_bytes(bytes).map_err(|_| FormatError::InvalidBytes)
}
