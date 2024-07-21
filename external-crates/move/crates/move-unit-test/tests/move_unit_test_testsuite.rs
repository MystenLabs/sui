// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_command_line_common::testing::{
    add_update_baseline_fix, format_diff, read_env_update_baseline, EXP_EXT,
};
use move_unit_test::{self, UnitTestingConfig};
use regex::RegexBuilder;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn run_test_with_modifiers(
    unit_test_config: UnitTestingConfig,
    path: &Path,
) -> anyhow::Result<Vec<((Vec<u8>, bool), PathBuf)>> {
    let mut results = Vec::new();

    let buffer = Vec::new();
    let test_plan = unit_test_config.build_test_plan();
    if test_plan.is_none() {
        anyhow::bail!("No test plan constructed for {:?}", path);
    }

    results.push((
        unit_test_config.run_and_report_unit_tests(test_plan.unwrap(), None, None, buffer)?,
        path.with_extension(EXP_EXT),
    ));

    Ok(results)
}

// Runs all tests under the test/test_sources directory.
fn run_test_impl(path: &Path) -> anyhow::Result<()> {
    std::env::set_var("NO_COLOR", "1");
    let update_baseline = read_env_update_baseline();
    let source_files = vec![path.to_str().unwrap().to_owned()];
    let unit_test_config = UnitTestingConfig {
        num_threads: 1,
        gas_limit: Some(1000),
        source_files,
        dep_files: move_stdlib::move_stdlib_files(),
        named_address_values: move_stdlib::move_stdlib_named_addresses()
            .into_iter()
            .collect(),
        report_stacktrace_on_abort: true,
        deterministic_generation: true,

        ..UnitTestingConfig::default_with_bound(None)
    };

    let regex = RegexBuilder::new(r"(┌─ ).+/([^/]+)$")
        .multi_line(true)
        .build()
        .unwrap();

    for ((buffer, _), exp_path) in run_test_with_modifiers(unit_test_config, path)? {
        let base_output = String::from_utf8(buffer)?;
        let cleaned_output = regex.replacen(&base_output, 0, r"$1$2");
        if update_baseline {
            fs::write(&exp_path, &*cleaned_output)?
        }

        let exp_exists = exp_path.is_file();

        if exp_exists {
            let expected = fs::read_to_string(&exp_path)?;
            if expected != cleaned_output {
                let msg = format!(
                    "Expected outputs differ for {:?}:\n{}",
                    exp_path,
                    format_diff(expected, cleaned_output)
                );
                anyhow::bail!(add_update_baseline_fix(msg));
            }
        } else {
            let msg = format!("No expected output found for {:?}", path);
            anyhow::bail!(add_update_baseline_fix(msg));
        }
    }

    Ok(())
}

fn run_test(path: &Path) -> datatest_stable::Result<()> {
    run_test_impl(path)?;
    Ok(())
}

datatest_stable::harness!(run_test, "tests/test_sources", r".*\.move$");
