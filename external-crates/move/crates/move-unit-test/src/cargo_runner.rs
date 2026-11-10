// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{UnitTestingConfig, vm_test_setup::VMTestSetup};
use anyhow::Context;
use move_command_line_common::files::find_filenames;

pub fn run_tests_with_config_and_filter<V: VMTestSetup + Sync>(
    mut config: UnitTestingConfig,
    root_path: &str,
    source_pattern: &str,
    dep_root: Option<&str>,
    vm_test_setup: V,
) -> anyhow::Result<()> {
    let sources = find_matching_files(root_path, source_pattern)?;
    let deps = dep_root
        .map(|root| find_matching_files(root, r".*\.move$"))
        .transpose()?
        .unwrap_or_default();

    config.source_files = sources;
    config.dep_files = deps;
    let test_plan = config
        .build_test_plan()
        .context("Unable to build test plan")?;

    let (_, all_tests_passed) = config
        .run_and_report_unit_tests(test_plan, vm_test_setup, std::io::stdout())
        .context("Failed to execute tests")?;

    if all_tests_passed {
        Ok(())
    } else {
        anyhow::bail!("Move unit tests failed")
    }
}

fn find_matching_files(root_path: &str, pattern: &str) -> anyhow::Result<Vec<String>> {
    let source_re = regex::Regex::new(pattern)
        .with_context(|| format!("Invalid source file pattern '{pattern}'"))?;
    find_filenames(&[root_path], |path| {
        source_re.is_match(&path.to_string_lossy())
    })
    .with_context(|| format!("Failed to discover Move sources under '{root_path}'"))
}

#[macro_export]
macro_rules! register_move_unit_tests {
    ($config:expr, $root:expr, $pattern:expr) => {
        #[test]
        fn move_unit_tests() -> Result<(), Box<dyn std::error::Error>> {
            $crate::cargo_runner::run_tests_with_config_and_filter(
                $config, $root, $pattern, None, None,
            )?;
            Ok(())
        }
    };
    ($config:expr, $root:expr, $source_pattern:expr, $dep_root:expr, $native_function_table:expr) => {
        #[test]
        fn move_unit_tests() -> Result<(), Box<dyn std::error::Error>> {
            $crate::cargo_runner::run_tests_with_config_and_filter(
                $config,
                $root,
                $source_pattern,
                Some($dep_root),
                $native_function_table,
            )?;
            Ok(())
        }
    };
}

#[cfg(all(test, unix))]
mod tests {
    use super::find_matching_files;
    use std::os::unix::fs::symlink;

    #[test]
    fn source_traversal_errors_are_propagated() {
        let temp_dir = tempfile::tempdir().unwrap();
        symlink(temp_dir.path(), temp_dir.path().join("loop")).unwrap();

        let error =
            find_matching_files(temp_dir.path().to_str().unwrap(), r".*\.move$").unwrap_err();

        assert!(
            error
                .to_string()
                .contains("Failed to discover Move sources")
        );
    }
}
