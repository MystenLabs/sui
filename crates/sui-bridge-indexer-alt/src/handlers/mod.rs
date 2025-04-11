// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;

pub mod governance_action_handler;
pub mod token_transfer_handler;

const LIMITER: &IdentStr = ident_str!("limiter");
const BRIDGE: &IdentStr = ident_str!("bridge");
const COMMITTEE: &IdentStr = ident_str!("committee");
const TREASURY: &IdentStr = ident_str!("treasury");
#[macro_export]
macro_rules! struct_tag {
    ($address:ident, $module:ident, $name:ident) => {{
        StructTag {
            address: $address,
            module: $module.into(),
            name: $name.into(),
            type_params: vec![],
        }
    }};
}
