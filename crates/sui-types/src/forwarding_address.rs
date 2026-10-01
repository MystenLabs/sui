// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{account_address::AccountAddress, ident_str, identifier::IdentStr};
use serde::{Deserialize, Serialize};

use crate::base_types::SuiAddress;

pub const FORWARDING_ADDRESS_MODULE_NAME: &IdentStr = ident_str!("forwarding_address");
pub const FORWARDING_DEPOSIT_STRUCT_NAME: &IdentStr = ident_str!("ForwardingDeposit");
pub const MASTER_REGISTERED_STRUCT_NAME: &IdentStr = ident_str!("MasterRegistered");
pub const MASTER_RECORD_STRUCT_NAME: &IdentStr = ident_str!("MasterRecord");

/// Layout of a forwarding address, all integers little-endian:
/// `[u32 master_id][FORWARDING_ADDRESS_MAGIC][u8 variant][u8 reserved][u128 tag]`.
/// The master id and magic positions are fixed for every variant; only the meaning of the tag
/// bytes depends on the variant.
pub const FORWARDING_ADDRESS_MAGIC: [u8; 10] = [0xfa; 10];
pub const FORWARDING_ADDRESS_VARIANT_OPAQUE: u8 = 0;
pub const FORWARDING_ADDRESS_RESERVED_MASTER_ID: u32 = 0;

const MASTER_ID_RANGE: std::ops::Range<usize> = 0..4;
const MAGIC_RANGE: std::ops::Range<usize> = 4..14;
const VARIANT_OFFSET: usize = 14;
const RESERVED_OFFSET: usize = 15;
const TAG_RANGE: std::ops::Range<usize> = 16..32;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ForwardingAddress {
    pub master_id: u32,
    pub variant: u8,
    pub tag: u128,
}

/// The address carries the forwarding magic but is not a canonical encoding of any variant.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ForwardingAddressFormatError {
    NonZeroReservedByte(u8),
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct ForwardingDeposit {
    pub forwarding_address: SuiAddress,
    pub master: SuiAddress,
    pub amount: u64,
    pub variant: u8,
    pub tag: u128,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct MasterRegistered {
    pub master_id: u32,
    pub master: SuiAddress,
    pub cap_id: crate::base_types::ObjectID,
}

impl ForwardingAddress {
    pub fn derive(master_id: u32, variant: u8, tag: u128) -> SuiAddress {
        let mut bytes = [0; AccountAddress::LENGTH];
        bytes[MASTER_ID_RANGE].copy_from_slice(&master_id.to_le_bytes());
        bytes[MAGIC_RANGE].copy_from_slice(&FORWARDING_ADDRESS_MAGIC);
        bytes[VARIANT_OFFSET] = variant;
        bytes[TAG_RANGE].copy_from_slice(&tag.to_le_bytes());
        SuiAddress::from_bytes(bytes).unwrap()
    }

    pub fn derive_opaque(master_id: u32, tag: u128) -> SuiAddress {
        Self::derive(master_id, FORWARDING_ADDRESS_VARIANT_OPAQUE, tag)
    }

    /// Whether the address carries the forwarding magic, regardless of whether the rest of it is
    /// a valid encoding. Everything that must refuse to treat such an address as an ordinary
    /// recipient keys off this.
    pub fn has_magic(address: SuiAddress) -> bool {
        address.to_inner()[MAGIC_RANGE] == FORWARDING_ADDRESS_MAGIC
    }

    /// `Ok(None)` is an ordinary address. `Err` is an address with the magic that no variant
    /// encodes canonically; callers must not fall back to treating it as ordinary.
    pub fn parse(address: SuiAddress) -> Result<Option<Self>, ForwardingAddressFormatError> {
        let bytes = address.to_inner();
        if bytes[MAGIC_RANGE] != FORWARDING_ADDRESS_MAGIC {
            return Ok(None);
        }
        if bytes[RESERVED_OFFSET] != 0 {
            return Err(ForwardingAddressFormatError::NonZeroReservedByte(
                bytes[RESERVED_OFFSET],
            ));
        }
        Ok(Some(Self {
            master_id: u32::from_le_bytes(bytes[MASTER_ID_RANGE].try_into().unwrap()),
            variant: bytes[VARIANT_OFFSET],
            tag: u128::from_le_bytes(bytes[TAG_RANGE].try_into().unwrap()),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(bytes: [u8; AccountAddress::LENGTH]) -> SuiAddress {
        SuiAddress::from_bytes(bytes).unwrap()
    }

    #[test]
    fn round_trips_little_endian_fields() {
        let master_id = 0x1020_3040;
        let tag = 0x1122_3344_5566_7788_99aa_bbcc_ddee_ff00;
        let address = ForwardingAddress::derive(master_id, 7, tag);
        let bytes = address.to_inner();
        assert_eq!(bytes[0..4], [0x40, 0x30, 0x20, 0x10]);
        assert_eq!(bytes[4..14], [0xfa; 10]);
        assert_eq!(bytes[14], 7);
        assert_eq!(bytes[15], 0);
        assert_eq!(bytes[16..20], [0x00, 0xff, 0xee, 0xdd]);
        assert_eq!(
            ForwardingAddress::parse(address),
            Ok(Some(ForwardingAddress {
                master_id,
                variant: 7,
                tag,
            }))
        );
        assert!(ForwardingAddress::has_magic(address));
    }

    #[test]
    fn ordinary_addresses_have_no_magic() {
        let ordinary = raw([0x42; AccountAddress::LENGTH]);
        assert_eq!(ForwardingAddress::parse(ordinary), Ok(None));
        assert!(!ForwardingAddress::has_magic(ordinary));

        let mut almost = ForwardingAddress::derive_opaque(1, 2).to_inner();
        almost[13] = 0xfd;
        assert_eq!(ForwardingAddress::parse(raw(almost)), Ok(None));
        assert!(!ForwardingAddress::has_magic(raw(almost)));

        for id in [
            SuiAddress::ZERO,
            crate::SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID.into(),
            crate::SUI_FRAMEWORK_ADDRESS.into(),
        ] {
            assert_eq!(ForwardingAddress::parse(id), Ok(None));
        }
    }

    #[test]
    fn nonzero_reserved_byte_is_not_an_ordinary_address() {
        let mut bytes = ForwardingAddress::derive_opaque(1, 2).to_inner();
        bytes[15] = 0x01;
        let address = raw(bytes);
        assert_eq!(
            ForwardingAddress::parse(address),
            Err(ForwardingAddressFormatError::NonZeroReservedByte(1))
        );
        assert!(ForwardingAddress::has_magic(address));
    }
}
