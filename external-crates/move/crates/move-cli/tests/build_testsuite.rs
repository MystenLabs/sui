// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_cli::sandbox::commands::test;
use tracing_subscriber::EnvFilter;

use std::path::{Path, PathBuf};

fn run_all(args_path: &Path) -> datatest_stable::Result<()> {
    let _ = tracing_subscriber::fmt::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .without_time()
        .with_target(false)
        .try_init();

    let cli_exe = env!("CARGO_BIN_EXE_move");
    let use_temp_dir = !args_path.parent().unwrap().join("NO_TEMPDIR").exists();
    test::run_one(
        args_path,
        &PathBuf::from(cli_exe),
        /* use_temp_dir */ use_temp_dir,
        /* track_cov */ false,
    )?;
    Ok(())
}

// runs all the tests
datatest_stable::harness!(run_all, "tests/build_tests", r"args\.txt$");
