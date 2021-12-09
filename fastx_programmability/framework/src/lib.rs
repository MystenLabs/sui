// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;

/// 0x1-- account address where Move stdlib modules are stored
pub const MOVE_STDLIB_ADDRESS: AccountAddress = AccountAddress::new([
    0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 1u8,
]);

/// 0x2-- account address where fastX framework modules are stored
pub const FASTX_FRAMEWORK_ADDRESS: AccountAddress = AccountAddress::new([
    0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 2u8,
]);

pub mod natives;

#[test]
fn check_that_move_code_can_be_built() {
    use move_package::BuildConfig;
    use std::path::PathBuf;

    let framework_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let build_config = BuildConfig {
        dev_mode: true,
        ..Default::default()
    };
    build_config
        .compile_package(&framework_dir, &mut Vec::new())
        .unwrap();
}
