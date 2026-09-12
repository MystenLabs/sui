// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{account_address::AccountAddress, ident_str, identifier::IdentStr};
use serde::{Deserialize, Serialize};

use crate::base_types::SuiAddress;

pub const FORWARDING_ADDRESS_MODULE_NAME: &IdentStr = ident_str!("forwarding_address");
pub const FORWARDING_DEPOSIT_STRUCT_NAME: &IdentStr = ident_str!("ForwardingDeposit");
pub const FORWARDING_ADDRESS_MAGIC: [u8; 8] = [0xfd; 8];

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ForwardingAddress {
    pub master_id: u64,
    pub tag: u128,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct ForwardingDeposit {
    pub forwarding_address: SuiAddress,
    pub master: SuiAddress,
    pub amount: u64,
    pub tag: u128,
}

impl ForwardingAddress {
    pub fn derive(master_id: u64, tag: u128) -> SuiAddress {
        let mut bytes = [0; AccountAddress::LENGTH];
        bytes[..8].copy_from_slice(&master_id.to_le_bytes());
        bytes[8..16].copy_from_slice(&FORWARDING_ADDRESS_MAGIC);
        bytes[16..].copy_from_slice(&tag.to_le_bytes());
        SuiAddress::from_bytes(bytes).unwrap()
    }

    pub fn parse(address: SuiAddress) -> Option<Self> {
        let bytes = address.to_inner();
        if bytes[8..16] != FORWARDING_ADDRESS_MAGIC {
            return None;
        }
        Some(Self {
            master_id: u64::from_le_bytes(bytes[..8].try_into().unwrap()),
            tag: u128::from_le_bytes(bytes[16..].try_into().unwrap()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_forwarding_address_layout() {
        let master_id = 0x1020_3040_5060_7080;
        let tag = 0x1122_3344_5566_7788_99aa_bbcc_ddee_ff00;

        assert_eq!(
            ForwardingAddress::parse(ForwardingAddress::derive(master_id, tag)),
            Some(ForwardingAddress { master_id, tag })
        );
    }

    #[test]
    fn ignores_ordinary_addresses() {
        assert_eq!(
            ForwardingAddress::parse(
                SuiAddress::from_bytes([0x42; AccountAddress::LENGTH]).unwrap(),
            ),
            None
        );
    }
}
