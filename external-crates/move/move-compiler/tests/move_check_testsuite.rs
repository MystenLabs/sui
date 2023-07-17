// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fs, path::Path};

use move_command_line_common::{
    env::read_bool_env_var,
    testing::{add_update_baseline_fix, format_diff, read_env_update_baseline, EXP_EXT, OUT_EXT},
};
use move_compiler::{
    command_line::compiler::move_check_for_errors,
    diagnostics::*,
    editions::Flavor,
    shared::{Flags, NumericalAddress, PackageConfig},
    Compiler, PASS_PARSER,
};

/// Shared flag to keep any temporary results of the test
const KEEP_TMP: &str = "KEEP";

const TEST_EXT: &str = "unit_test";
const VERIFICATION_EXT: &str = "verification";
const UNUSED_EXT: &str = "unused";

const SUI_MODE_DIR: &str = "sui_mode";

fn default_testing_addresses(flavor: Flavor) -> BTreeMap<String, NumericalAddress> {
    let mut mapping = vec![
        ("std", "0x1"),
        ("sui", "0x2"),
        ("M", "0x1"),
        ("A", "0x42"),
        ("B", "0x42"),
        ("K", "0x19"),
        ("a", "0x42"),
        ("b", "0x42"),
        ("k", "0x19"),
    ];
    if flavor == Flavor::Sui {
        mapping.extend([("sui", "0x2"), ("sui_system", "0x3")]);
    }
    mapping
        .into_iter()
        .map(|(name, addr)| (name.to_string(), NumericalAddress::parse_str(addr).unwrap()))
        .collect()
}

fn move_check_testsuite(path: &Path) -> datatest_stable::Result<()> {
    let flavor = if path.components().any(|c| c.as_os_str() == SUI_MODE_DIR) {
        Flavor::Sui
    } else {
        Flavor::default()
    };
    let config = PackageConfig {
        flavor,
        ..PackageConfig::default()
    };
    testsuite(path, config)
}

fn testsuite(path: &Path, mut config: PackageConfig) -> datatest_stable::Result<()> {
    // A test is marked that it should also be compiled in test mode by having a `path.unit_test`
    // file.
    if path.with_extension(TEST_EXT).exists() {
        let test_exp_path = format!(
            "{}.unit_test.{}",
            path.with_extension("").to_string_lossy(),
            EXP_EXT
        );
        let test_out_path = format!(
            "{}.unit_test.{}",
            path.with_extension("").to_string_lossy(),
            OUT_EXT
        );
        let mut config = config.clone();
        config
            .warning_filter
            .union(&WarningFilters::unused_function_warnings_filter());
        run_test(
            path,
            Path::new(&test_exp_path),
            Path::new(&test_out_path),
            Flags::testing(),
            config,
        )?;
    }

    // A verification case is marked that it should also be compiled in verification mode by having
    // a `path.verification` file.
    if path.with_extension(VERIFICATION_EXT).exists() {
        let verification_exp_path = format!(
            "{}.verification.{}",
            path.with_extension("").to_string_lossy(),
            EXP_EXT
        );
        let verification_out_path = format!(
            "{}.verification.{}",
            path.with_extension("").to_string_lossy(),
            OUT_EXT
        );
        let mut config = config.clone();
        config
            .warning_filter
            .union(&WarningFilters::unused_function_warnings_filter());
        run_test(
            path,
            Path::new(&verification_exp_path),
            Path::new(&verification_out_path),
            Flags::verification(),
            config,
        )?;
    }

    // A cross-module unused case that should run without unused warnings suppression
    if path.with_extension(UNUSED_EXT).exists() {
        let unused_exp_path = format!(
            "{}.unused.{}",
            path.with_extension("").to_string_lossy(),
            EXP_EXT
        );
        let unused_out_path = format!(
            "{}.unused.{}",
            path.with_extension("").to_string_lossy(),
            OUT_EXT
        );
        run_test(
            path,
            Path::new(&unused_exp_path),
            Path::new(&unused_out_path),
            Flags::empty(),
            config.clone(),
        )?;
    }

    let exp_path = path.with_extension(EXP_EXT);
    let out_path = path.with_extension(OUT_EXT);

    let flags = Flags::empty();

    config
        .warning_filter
        .union(&WarningFilters::unused_function_warnings_filter());
    run_test(path, &exp_path, &out_path, flags, config)?;
    Ok(())
}

// Runs all tests under the test/testsuite directory.
pub fn run_test(
    path: &Path,
    exp_path: &Path,
    out_path: &Path,
    flags: Flags,
    default_config: PackageConfig,
) -> anyhow::Result<()> {
    let targets: Vec<String> = vec![path.to_str().unwrap().to_owned()];

    let (files, comments_and_compiler_res) = Compiler::from_files(
        targets,
        move_stdlib::move_stdlib_files(),
        default_testing_addresses(default_config.flavor),
    )
    .set_flags(flags)
    .set_default_config(default_config)
    .run::<PASS_PARSER>()?;
    let diags = move_check_for_errors(comments_and_compiler_res);

    let has_diags = !diags.is_empty();
    let diag_buffer = if has_diags {
        report_diagnostics_to_buffer(&files, diags)
    } else {
        vec![]
    };

    let save_diags = read_bool_env_var(KEEP_TMP);
    let update_baseline = read_env_update_baseline();

    let rendered_diags = std::str::from_utf8(&diag_buffer)?;
    if save_diags {
        fs::write(out_path, &diag_buffer)?;
    }

    if update_baseline {
        if has_diags {
            fs::write(exp_path, rendered_diags)?;
        } else if exp_path.is_file() {
            fs::remove_file(exp_path)?;
        }
        return Ok(());
    }

    let exp_exists = exp_path.is_file();
    match (has_diags, exp_exists) {
        (false, false) => Ok(()),
        (true, false) => {
            let msg = format!(
                "Expected success. Unexpected diagnostics:\n{}",
                rendered_diags
            );
            anyhow::bail!(add_update_baseline_fix(msg))
        }
        (false, true) => {
            let msg = format!(
                "Unexpected success. Expected diagnostics:\n{}",
                fs::read_to_string(exp_path)?
            );
            anyhow::bail!(add_update_baseline_fix(msg))
        }
        (true, true) => {
            let expected_diags = fs::read_to_string(exp_path)?;
            if rendered_diags != expected_diags {
                let msg = format!(
                    "Expected diagnostics differ from actual diagnostics:\n{}",
                    format_diff(expected_diags, rendered_diags),
                );
                anyhow::bail!(add_update_baseline_fix(msg))
            } else {
                Ok(())
            }
        }
    }
}

datatest_stable::harness!(move_check_testsuite, "tests/", r".*\.move$");
