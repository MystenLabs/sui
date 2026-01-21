// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;
use move_core_types::annotated_value as A;
use move_core_types::annotated_visitor as AV;
use move_core_types::u256::U256;
use move_core_types::visitor_default;
use sui_types::id::ID;
use sui_types::id::UID;

use crate::v2::error::FormatError;

/// Visitor to extract addresses from Objects, UIDs, IDs, or addresses.
pub(crate) struct AddressExtractor;

impl AV::Visitor<'_, '_> for AddressExtractor {
    type Value = Option<AccountAddress>;
    type Error = FormatError;

    visitor_default! { <'_, '_> u8, u16, u32, u64, u128, u256 = Ok(None) }
    visitor_default! { <'_, '_> bool, signer, vector, variant = Ok(None) }

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
}

/// Attempts to extract an address from the prefix of the struct being visited. Assumes the driver
/// is pointed at the first field of the struct on entry.
fn extract_address(
    driver: &mut AV::StructDriver<'_, '_, '_>,
) -> Result<AccountAddress, FormatError> {
    driver.skip_field()?;
    let bytes = &driver.bytes()[driver.start()..driver.position()];
    Ok(bcs::from_bytes(bytes)?)
}
