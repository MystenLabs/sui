// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{UnitTestingConfig, vm_test_setup::VMTestSetup};
use anyhow::Context;
use move_command_line_common::files::find_filenames;

pub fn run_tests_with_config_and_filter<V: VMTestSetup + Sync>(
    config: UnitTestingConfig,
    root_path: &str,
    source_pattern: &str,
    dep_root: Option<&str>,
    vm_test_setup: V,
) {
    if let Err(error) = run_tests(config, root_path, source_pattern, dep_root, vm_test_setup) {
        panic!("{error:#}");
    }
}

fn run_tests<V: VMTestSetup + Sync>(
    mut config: UnitTestingConfig,
    root_path: &str,
    source_pattern: &str,
    dep_root: Option<&str>,
    vm_test_setup: V,
) -> anyhow::Result<()> {
    config.source_files = find_matching_files(root_path, source_pattern)?;
    if let Some(root) = dep_root {
        config.dep_files = find_matching_files(root, r".*\.move$")?;
    }
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
        fn move_unit_tests() {
            $crate::cargo_runner::run_tests_with_config_and_filter(
                $config,
                $root,
                $pattern,
                None,
                $crate::vm_test_setup::DefaultVMTestSetup::legacy_default(),
            )
        }
    };
    ($config:expr, $root:expr, $source_pattern:expr, $dep_root:expr, $vm_test_setup:expr) => {
        #[test]
        fn move_unit_tests() {
            $crate::cargo_runner::run_tests_with_config_and_filter(
                $config,
                $root,
                $source_pattern,
                Some($dep_root),
                $vm_test_setup,
            )
        }
    };
}

#[cfg(all(test, unix))]
mod tests {
    use super::find_matching_files;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn source_traversal_errors_are_propagated() {
        let temp_dir = tempfile::tempdir().unwrap();
        let unreadable = temp_dir.path().join("unreadable");
        std::fs::create_dir(&unreadable).unwrap();
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();

        let result = find_matching_files(temp_dir.path().to_str().unwrap(), r".*\.move$");
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let error = result.unwrap_err();

        assert!(
            error
                .to_string()
                .contains("Failed to discover Move sources")
        );
    }
}

#[cfg(test)]
mod macro_tests {
    crate::register_move_unit_tests!(
        {
            crate::UnitTestingConfig {
                num_threads: 1,
                gas_limit: Some(1000),
                dep_files: move_stdlib::source_files(),
                named_address_values: move_stdlib::named_addresses().into_iter().collect(),
                report_stacktrace_on_abort: true,
                deterministic_generation: true,
                ..crate::UnitTestingConfig::default_with_bound(None)
            }
        },
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/test_sources"),
        r".*[\\/]do_nothing\.move$"
    );
}

#[cfg(test)]
mod macro_with_deps_tests {
    crate::register_move_unit_tests!(
        crate::UnitTestingConfig {
            num_threads: 1,
            gas_limit: Some(1000),
            named_address_values: move_stdlib::named_addresses().into_iter().collect(),
            report_stacktrace_on_abort: true,
            deterministic_generation: true,
            ..crate::UnitTestingConfig::default_with_bound(None)
        },
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/test_sources"),
        r".*[\\/]do_nothing\.move$",
        &move_stdlib::modules_full_path(),
        crate::vm_test_setup::DefaultVMTestSetup::legacy_default()
    );
}
