// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

use move_core_types::account_address::AccountAddress;

#[macro_use]
pub mod error;

pub mod base_types;
pub mod batch;
pub mod coin;
pub mod committee;
pub mod crypto;
pub mod event;
pub mod gas;
pub mod gas_coin;
pub mod id;
pub mod json_schema;
pub mod messages;
pub mod move_package;
pub mod object;
pub mod readable_serde;
pub mod signature_seed;
pub mod storage;

/// 0x1-- account address where Move stdlib modules are stored
/// Same as the ObjectID
pub const MOVE_STDLIB_ADDRESS: AccountAddress = AccountAddress::ONE;

/// 0x2-- account address where sui framework modules are stored
/// Same as the ObjectID
pub const SUI_FRAMEWORK_ADDRESS: AccountAddress = get_hex_address_two();

const fn get_hex_address_two() -> AccountAddress {
    let mut addr = [0u8; AccountAddress::LENGTH];
    addr[AccountAddress::LENGTH - 1] = 2u8;
    AccountAddress::new(addr)
}
