// Copyright (c) The Diem Core Contributors
// SPDX-License-Identifier: Apache-2.0

//use move_cli::sandbox::commands::test;

use std::path::Path;

fn run_all(args_path: &Path) -> datatest_stable::Result<()> {
    let _use_temp_dir = !args_path.parent().unwrap().join("NO_TEMPDIR").exists();
    let target =
        std::env::var_os("CARGO_BUILD_TARGET_DIR").unwrap_or_else(|| "../../target".into());
    #[cfg(debug_assertions)]
    let profile: String = "debug".into();
    #[cfg(not(debug_assertions))]
    let profile: String = "release".into();

    let fastx_binary = Path::new(&target).join(profile).join("fastx");
    assert!(fastx_binary.exists(), "No such binary {:?}", fastx_binary);
    // TODO: crashes inside diem code with `Error: prefix not found` when running `cargo test`
    /*test::run_one(
        args_path,
        &fastx_binary,
        /* use_temp_dir */ use_temp_dir,
        /* track_cov */ false,
    )?;*/
    Ok(())
}

// runs all the tests
datatest_stable::harness!(run_all, "tests/testsuite", r"args.txt$");
