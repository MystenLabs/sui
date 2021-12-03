// Copyright (c) The Diem Core Contributors
// SPDX-License-Identifier: Apache-2.0

//use move_cli::sandbox::commands::test;

use std::path::{Path, PathBuf};

fn run_all(args_path: &Path) -> datatest_stable::Result<()> {
    let _use_temp_dir = !args_path.parent().unwrap().join("NO_TEMPDIR").exists();
    let fastx_binary = PathBuf::from("../../target/debug/fastx");
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
