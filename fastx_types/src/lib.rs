// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]
#![deny(warnings)]

use move_core_types::account_address::AccountAddress;

#[macro_use]
pub mod error;

pub mod base_types;
pub mod coin;
pub mod committee;
pub mod gas;
pub mod gas_coin;
pub mod id;
pub mod messages;
pub mod object;
pub mod serialize;
pub mod storage;

/// 0x1-- account address where Move stdlib modules are stored
pub const MOVE_STDLIB_ADDRESS: AccountAddress = AccountAddress::new([
    0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 1u8,
]);

/// 0x2-- account address where fastX framework modules are stored
pub const FASTX_FRAMEWORK_ADDRESS: AccountAddress = AccountAddress::new([
    0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 2u8,
]);

/// 0xFDC6D587C83A348E456B034E1E0C31E9, object ID of the move stdlib package.
pub const MOVE_STDLIB_OBJECT_ID: AccountAddress = AccountAddress::new([
    0xFD, 0xC6, 0xD5, 0x87, 0xC8, 0x3A, 0x34, 0x8E, 0x45, 0x6B, 0x03, 0x4E, 0x1E, 0x0C, 0x31, 0xE9,
]);

/// 0x6D3FFC5213ED4DF6802CD4535D3C18F6, object ID of the fastx framework package.
pub const FASTX_FRAMEWORK_OBJECT_ID: AccountAddress = AccountAddress::new([
    0x6D, 0x3F, 0xFC, 0x52, 0x13, 0xED, 0x4D, 0xF6, 0x80, 0x2C, 0xD4, 0x53, 0x5D, 0x3C, 0x18, 0xF6,
]);
