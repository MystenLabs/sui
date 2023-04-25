// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use insta::assert_snapshot;
use move_cli::base::reroot_path;
use move_disassembler::disassembler::Disassembler;
use sui_move_build::BuildConfig;

const TEST_MODULE_DATA_DIR: &str = "../sui-framework/packages/sui-framework/tests";

// Execute every entry function in Move framework and examples and ensure costs don't change
// To review snapshot changes, and fix snapshot differences,
// 0. Install cargo-insta (`cargo install cargo-insta`)
// 1. Run `cargo insta test --review` under `./sui-cost`.
// 2. Review, accept or reject changes.

#[tokio::test]
async fn test_natives_disassemble_snapshot() -> Result<(), anyhow::Error> {
    let natives_calib = disassemble_test_module("natives_calibration_tests".to_string())?;

    // Assert that nothing has changed in the disassemby of the code
    assert_snapshot!(natives_calib);
    Ok(())
}

#[tokio::test]
async fn test_bytecode_disassemble_snapshot() -> Result<(), anyhow::Error> {
    let bytecode_calib = disassemble_test_module("bytecode_calibration_tests".to_string())?;

    // Assert that nothing has changed in the disassemby of the code
    assert_snapshot!(bytecode_calib);
    Ok(())
}

fn disassemble_test_module(name: String) -> anyhow::Result<String> {
    let path = PathBuf::from(TEST_MODULE_DATA_DIR);
    let mut config = BuildConfig::new_for_testing();
    config.config.test_mode = true;
    let package_name: Option<String> = None;
    let module_name = name;

    let rerooted_path = reroot_path(Some(path))?;

    let package = config
        .config
        .compile_package(&rerooted_path, &mut std::io::sink())?;
    let needle_package = package_name
        .as_deref()
        .unwrap_or(package.compiled_package_info.package_name.as_str());
    match package
        .get_module_by_name(needle_package, &module_name)
        .ok()
    {
        None => anyhow::bail!(
            "Unable to find module or script with name '{}' in package '{}'",
            module_name,
            needle_package,
        ),
        Some(unit) => Disassembler::from_unit(&unit.unit).disassemble(),
    }
}
