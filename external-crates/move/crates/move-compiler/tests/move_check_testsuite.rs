// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use move_command_line_common::{
    env::read_bool_env_var,
    files::MOVE_EXTENSION,
    insta_assert,
    testing::{InstaOptions, OUT_EXT},
};
use move_compiler::{
    command_line::compiler::move_check_for_errors,
    diagnostics::warning_filters::WarningFiltersBuilder,
    diagnostics::*,
    editions::{Edition, Flavor},
    linters::{self, LintLevel},
    shared::{Flags, NumericalAddress, PackageConfig, PackagePaths},
    sui_mode, Compiler, PASS_PARSER,
};
use serde::{Deserialize, Serialize};

/// Shared flag to keep any temporary results of the test
const KEEP_TMP: &str = "KEEP";

const TEST_EXT: &str = "unit_test";
const UNUSED_EXT: &str = "unused";
const MIGRATION_EXT: &str = "migration";
const IDE_EXT: &str = "ide";

const LINTER_DIR: &str = "linter";
const SUI_MODE_DIR: &str = "sui_mode";
const MOVE_2024_DIR: &str = "move_2024";
const DEV_DIR: &str = "development";

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct TestInfo {
    flavor: Flavor,
    edition: Edition,
    lint: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestKind {
    // Normal test
    Normal,
    // Tests unit test functionality
    Test,
    // Does not silence warnings for unused items
    Unused,
    // Tests edition migration
    Migration,
    // Tests additional generation for the IDE
    IDE,
}

impl TestKind {
    fn from_extension(path_extension: &std::ffi::OsStr) -> Self {
        match () {
            _ if path_extension == MOVE_EXTENSION => TestKind::Normal,
            _ if path_extension == TEST_EXT => TestKind::Test,
            _ if path_extension == UNUSED_EXT => TestKind::Unused,
            _ if path_extension == MIGRATION_EXT => TestKind::Migration,
            _ if path_extension == IDE_EXT => TestKind::IDE,
            _ => panic!("Unknown extension: {}", path_extension.to_string_lossy()),
        }
    }

    fn snap_suffix(&self) -> Option<&'static str> {
        match self {
            TestKind::Normal => None,
            TestKind::Test => Some(TEST_EXT),
            TestKind::Unused => Some(UNUSED_EXT),
            TestKind::Migration => Some(MIGRATION_EXT),
            TestKind::IDE => Some(IDE_EXT),
        }
    }
}

fn default_testing_addresses(flavor: Flavor) -> BTreeMap<String, NumericalAddress> {
    let mut mapping = vec![
        ("std", "0x1"),
        ("sui", "0x2"),
        ("M", "0x40"),
        ("A", "0x41"),
        ("B", "0x42"),
        ("K", "0x19"),
        ("a", "0x44"),
        ("b", "0x45"),
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

fn test_config(path: &Path) -> (TestKind, TestInfo, PackageConfig, Flags) {
    let test_kind = TestKind::from_extension(path.extension().unwrap());
    let path_contains = |s| path.components().any(|c| c.as_os_str() == s);
    let lint = path_contains(LINTER_DIR);
    let flavor = if path_contains(SUI_MODE_DIR) {
        Flavor::Sui
    } else {
        Flavor::default()
    };
    let move_2024_mode = path_contains(MOVE_2024_DIR);
    let dev_mode = path_contains(DEV_DIR);
    assert!(
        [move_2024_mode, dev_mode]
            .into_iter()
            .filter(|x| *x)
            .count()
            <= 1,
        "A test can have at most directory based edition"
    );
    let edition = if test_kind == TestKind::Migration {
        // migration mode overrides the edition
        Edition::E2024_MIGRATION
    } else if move_2024_mode {
        Edition::E2024_ALPHA
    } else if dev_mode {
        Edition::DEVELOPMENT
    } else {
        Edition::LEGACY
    };
    // config
    let mut config = PackageConfig {
        flavor,
        edition,
        is_dependency: false,
        warning_filter: WarningFiltersBuilder::new_for_source(),
    };
    // Unused and IDE do not have additional warning filters
    if !matches!(test_kind, TestKind::Unused | TestKind::IDE) {
        config
            .warning_filter
            .union(&WarningFiltersBuilder::unused_warnings_filter_for_test());
    }
    // test info
    let test_info = TestInfo {
        flavor,
        edition,
        lint,
    };
    // flags
    let flags = match test_kind {
        // no flags for normal tests
        TestKind::Normal => Flags::empty(),
        // we want to be able to see test/test_only elements in these modes
        TestKind::Test | TestKind::Unused | TestKind::Migration => Flags::testing(),
        // additional flags for IDE
        TestKind::IDE => Flags::testing().set_ide_test_mode(true).set_ide_mode(true),
    };
    (test_kind, test_info, config, flags)
}

fn out_path(path: &Path, test_name: &str, test_kind: Option<&str>) -> PathBuf {
    let n;
    let file_name = match test_kind {
        Some(c) => {
            n = format!("{test_name}@{c}");
            &n
        }
        None => test_name,
    };
    path.with_file_name(file_name).with_extension(OUT_EXT)
}

// Runs all tests under the test/testsuite directory.
pub fn run_test(path: &Path) -> datatest_stable::Result<()> {
    let (test_kind, test_info, package_config, flags) = test_config(path);
    let suffix = test_kind.snap_suffix();
    let migration_mode = package_config.edition == Edition::E2024_MIGRATION;
    let test_name = path.file_stem().unwrap().to_string_lossy();
    let test_name: &str = test_name.as_ref();
    let move_path = path.with_extension(MOVE_EXTENSION);
    let out_path = out_path(path, test_name, suffix);
    let flavor = package_config.flavor;
    let targets: Vec<String> = vec![move_path.to_str().unwrap().to_owned()];
    let named_address_map = default_testing_addresses(flavor);
    let deps = vec![PackagePaths {
        name: Some(("stdlib".into(), PackageConfig::default())),
        paths: move_stdlib::move_stdlib_files(),
        named_address_map: named_address_map.clone(),
    }];
    let target_name = if migration_mode {
        Some(("test".into(), package_config.clone()))
    } else {
        None
    };
    let targets = vec![PackagePaths {
        name: target_name,
        paths: targets,
        named_address_map,
    }];

    let flags = flags.set_sources_shadow_deps(true);
    let mut compiler = Compiler::from_package_paths(None, targets, deps)
        .unwrap()
        .set_flags(flags)
        .set_default_config(package_config);

    if flavor == Flavor::Sui {
        let (prefix, filters) = sui_mode::linters::known_filters();
        compiler = compiler.add_custom_known_filters(prefix, filters);
        if test_info.lint {
            compiler = compiler.add_visitors(sui_mode::linters::linter_visitors(LintLevel::All))
        }
    }
    let (prefix, filters) = linters::known_filters();
    compiler = compiler.add_custom_known_filters(prefix, filters);
    if test_info.lint {
        compiler = compiler.add_visitors(linters::linter_visitors(LintLevel::All))
    }

    let (files, comments_and_compiler_res) = compiler.run::<PASS_PARSER>()?;
    let diags = move_check_for_errors(comments_and_compiler_res);

    let has_diags = !diags.is_empty();
    let diag_buffer = if has_diags {
        if migration_mode {
            report_migration_to_buffer(&files, diags)
        } else {
            report_diagnostics_to_buffer(&files, diags, /* ansi_color */ false)
        }
    } else {
        vec![]
    };

    let save_diags = read_bool_env_var(KEEP_TMP);

    let rendered_diags = std::str::from_utf8(&diag_buffer)?;
    if save_diags {
        fs::write(out_path, &diag_buffer)?;
    }

    let mut options = InstaOptions::new();
    options.info(test_info);
    if let Some(suffix) = suffix {
        options.suffix(suffix);
    }
    options.name(test_name);
    insta_assert! {
        input_path: move_path,
        contents: rendered_diags,
        options: options,
    };
    Ok(())
}

datatest_stable::harness!(
    run_test,
    "tests/",
    r".*\.move$",
    run_test,
    "tests/",
    r".*\.unit_test$",
    run_test,
    "tests/",
    r".*\.unused$",
    run_test,
    "tests/",
    r".*\.migration$",
    run_test,
    "tests/",
    r".*\.ide$",
);
