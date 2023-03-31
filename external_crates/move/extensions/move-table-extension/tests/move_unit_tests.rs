// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_cli::base::test::{run_move_unit_tests, UnitTestResult};
use move_core_types::account_address::AccountAddress;
use move_table_extension::{table_natives, GasParameters};
use move_unit_test::UnitTestingConfig;
use std::path::PathBuf;
use tempfile::tempdir;

fn run_tests_for_pkg(path_to_pkg: impl Into<String>) {
    let pkg_path = path_in_crate(path_to_pkg);
    let mut natives = move_stdlib::natives::all_natives(
        AccountAddress::from_hex_literal("0x1").unwrap(),
        move_stdlib::natives::GasParameters::zeros(),
    );
    natives.append(&mut table_natives(
        AccountAddress::from_hex_literal("0x2").unwrap(),
        GasParameters::zeros(),
    ));
    let res = run_move_unit_tests(
        &pkg_path,
        move_package::BuildConfig {
            test_mode: true,
            install_dir: Some(tempdir().unwrap().path().to_path_buf()),
            ..Default::default()
        },
        UnitTestingConfig::default_with_bound(Some(100_000)),
        natives,
        None,
        /* compute_coverage */ false,
        &mut std::io::stdout(),
    )
    .unwrap();
    if res != UnitTestResult::Success {
        panic!("aborting because of Move unit test failures");
    }
}

#[test]
fn move_unit_tests() {
    run_tests_for_pkg(".");
}

pub fn path_in_crate<S>(relative: S) -> PathBuf
where
    S: Into<String>,
{
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push(relative.into());
    path
}
