// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_command_line_common::testing::insta_assert;
use move_unit_test::{self, UnitTestingConfig};
use regex::RegexBuilder;
use std::path::Path;

// Runs all tests under the test/test_sources directory.
fn run_test_impl(path: &Path) -> anyhow::Result<()> {
    std::env::set_var("NO_COLOR", "1");
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

    let test_plan = unit_test_config.build_test_plan();
    let Some(test_plan) = test_plan else {
        anyhow::bail!("No test plan constructed for {:?}", path);
    };

    let (buffer, _) = unit_test_config.run_and_report_unit_tests(test_plan, None, None, vec![])?;
    let base_output = String::from_utf8(buffer)?;
    let cleaned_output = regex.replacen(&base_output, 0, r"$1$2");

    insta_assert! {
        input_path: path,
        contents: cleaned_output,
    };
    Ok(())
}

fn run_test(path: &Path) -> datatest_stable::Result<()> {
    run_test_impl(path)?;
    Ok(())
}

datatest_stable::harness!(run_test, "tests/test_sources", r".*\.move$");
