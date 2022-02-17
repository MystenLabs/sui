// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]
#![deny(warnings)]

use base_types::ObjectID;

#[macro_use]
pub mod error;

pub mod base_types;
pub mod coin;
pub mod committee;
pub mod event;
pub mod gas;
pub mod gas_coin;
pub mod id;
pub mod messages;
pub mod object;
pub mod serialize;
pub mod storage;

/// 0x1-- account address where Move stdlib modules are stored
pub const MOVE_STDLIB_ADDRESS: ObjectID = ObjectID::ONE;

/// 0x2-- account address where fastX framework modules are stored
pub const SUI_FRAMEWORK_ADDRESS: ObjectID = get_hex_address_two();

const fn get_hex_address_two() -> ObjectID {
    let mut addr = [0u8; ObjectID::LENGTH];
    addr[ObjectID::LENGTH - 1] = 2u8;
    ObjectID::new(addr)
}
