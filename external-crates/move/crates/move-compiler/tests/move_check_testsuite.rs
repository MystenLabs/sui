// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fs,
    path::{self, Path, PathBuf},
};

use move_command_line_common::{
    env::read_bool_env_var,
    files::MOVE_EXTENSION,
    insta_assert,
    testing::{read_env_update_baseline, InstaOptions, EXP_EXT, OUT_EXT},
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

// fn move_check_testsuite(path: &Path) -> datatest_stable::Result<()> {
//     let path_contains = |s| path.components().any(|c| c.as_os_str() == s);
//     let lint = path_contains(LINTER_DIR);
//     let flavor = if path_contains(SUI_MODE_DIR) {
//         Flavor::Sui
//     } else {
//         Flavor::default()
//     };
//     let edition = if path_contains(MOVE_2024_DIR) {
//         Edition::E2024_ALPHA
//     } else if path_contains(DEV_DIR) {
//         Edition::DEVELOPMENT
//     } else {
//         Edition::LEGACY
//     };
//     let config = PackageConfig {
//         flavor,
//         edition,
//         is_dependency: false,
//         warning_filter: WarningFiltersBuilder::new_for_source(),
//     };
//     testsuite(path, config, lint)
// }

// fn testsuite(path: &Path, mut config: PackageConfig, lint: bool) -> datatest_stable::Result<()> {
//     // A test is marked that it should also be compiled in test mode by having a `path.unit_test`
//     // file.
//     if path.with_extension(TEST_EXT).exists() {
//         let test_exp_path = format!(
//             "{}.{TEST_EXT}.{EXP_EXT}",
//             path.with_extension("").to_string_lossy(),
//         );
//         let test_out_path = format!(
//             "{}.{TEST_EXT}.{OUT_EXT}",
//             path.with_extension("").to_string_lossy(),
//         );
//         let mut config = config.clone();
//         config
//             .warning_filter
//             .union(&WarningFiltersBuilder::unused_warnings_filter_for_test());
//         run_test(
//             path,
//             Path::new(&test_exp_path),
//             Path::new(&test_out_path),
//             Flags::testing(),
//             config,
//             lint,
//         )?;
//     }

//     // A test is marked that it should also be compiled in migration mode by having a
//     // `path.migration` file.
//     if path.with_extension(MIGRATION_EXT).exists() {
//         let migration_exp_path = format!(
//             "{}.{MIGRATION_EXT}.{EXP_EXT}",
//             path.with_extension("").to_string_lossy(),
//         );
//         let migration_out_path = format!(
//             "{}.{MIGRATION_EXT}.{OUT_EXT}",
//             path.with_extension("").to_string_lossy(),
//         );
//         let mut config = config.clone();
//         config
//             .warning_filter
//             .union(&WarningFiltersBuilder::unused_warnings_filter_for_test());
//         run_test_inner(
//             path,
//             Path::new(&migration_exp_path),
//             Path::new(&migration_out_path),
//             Flags::testing(),
//             config,
//             lint,
//             true,
//         )?;
//     }

//     // A cross-module unused case that should run without unused warnings suppression
//     if path.with_extension(UNUSED_EXT).exists() {
//         let unused_exp_path = format!(
//             "{}.{UNUSED_EXT}.{EXP_EXT}",
//             path.with_extension("").to_string_lossy(),
//         );
//         let unused_out_path = format!(
//             "{}.{UNUSED_EXT}.{OUT_EXT}",
//             path.with_extension("").to_string_lossy(),
//         );
//         run_test(
//             path,
//             Path::new(&unused_exp_path),
//             Path::new(&unused_out_path),
//             Flags::testing(),
//             config.clone(),
//             lint,
//         )?;
//     }

//     // A cross-module unused case that should run without unused warnings suppression
//     if path.with_extension(IDE_EXT).exists() {
//         let ide_exp_path = format!(
//             "{}.{IDE_EXT}.{EXP_EXT}",
//             path.with_extension("").to_string_lossy(),
//         );
//         let ide_out_path = format!(
//             "{}.{IDE_EXT}.{OUT_EXT}",
//             path.with_extension("").to_string_lossy(),
//         );
//         run_test(
//             path,
//             Path::new(&ide_exp_path),
//             Path::new(&ide_out_path),
//             Flags::testing().set_ide_test_mode(true).set_ide_mode(true),
//             config.clone(),
//             lint,
//         )?;
//     }

//     let exp_path = path.with_extension(EXP_EXT);
//     let out_path = path.with_extension(OUT_EXT);

//     config
//         .warning_filter
//         .union(&WarningFiltersBuilder::unused_warnings_filter_for_test());
//     run_test(path, &exp_path, &out_path, Flags::empty(), config, lint)?;
//     Ok(())
// }

fn test_config(path: &Path) -> (Option<&'static str>, TestInfo, PackageConfig, Flags) {
    let path_extension = path.extension().unwrap();
    let path_contains = |s| path.components().any(|c| c.as_os_str() == s);
    let lint = path_contains(LINTER_DIR);
    let flavor = if path_contains(SUI_MODE_DIR) {
        Flavor::Sui
    } else {
        Flavor::default()
    };
    let migration_mode = path_extension == MIGRATION_EXT;
    let move_2024_mode = path_contains(MOVE_2024_DIR);
    let dev_mode = path_contains(DEV_DIR);
    assert!(
        [move_2024_mode, dev_mode,]
            .into_iter()
            .filter(|x| *x)
            .count()
            <= 1,
        "A test can have at most directory based edition"
    );
    let edition = if migration_mode {
        // migration mode overrides the edition
        Edition::E2024_MIGRATION
    } else if move_2024_mode {
        Edition::E2024_ALPHA
    } else if dev_mode {
        Edition::DEVELOPMENT
    } else {
        Edition::LEGACY
    };
    // test kind
    let test_kind = match () {
        _ if path_extension == MOVE_EXTENSION => None,
        _ if path_extension == TEST_EXT => Some(TEST_EXT),
        _ if path_extension == UNUSED_EXT => Some(UNUSED_EXT),
        _ if path_extension == MIGRATION_EXT => Some(MIGRATION_EXT),
        _ if path_extension == IDE_EXT => Some(IDE_EXT),
        _ => panic!("Unknown extension: {}", path_extension.to_string_lossy()),
    };
    // config
    let mut config = PackageConfig {
        flavor,
        edition,
        is_dependency: false,
        warning_filter: WarningFiltersBuilder::new_for_source(),
    };
    if path_extension != UNUSED_EXT {
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
    let flags = if path_extension == IDE_EXT {
        Flags::testing().set_ide_test_mode(true).set_ide_mode(true)
    } else {
        Flags::testing()
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
    let test_name: &str = path.file_name().unwrap().to_string_lossy().as_ref();
    let move_path = path.with_extension(MOVE_EXTENSION);
    let out_path = out_path(path, test_name, test_kind);
    let flavor = package_config.flavor;
    let targets: Vec<String> = vec![move_path.to_str().unwrap().to_owned()];
    let named_address_map = default_testing_addresses(flavor);
    let deps = vec![PackagePaths {
        name: Some(("stdlib".into(), PackageConfig::default())),
        paths: move_stdlib::move_stdlib_files(),
        named_address_map: named_address_map.clone(),
    }];
    let target_name = if package_config.edition == Edition::E2024_MIGRATION {
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
        if package_config.edition == Edition::E2024_MIGRATION {
            report_migration_to_buffer(&files, diags)
        } else {
            report_diagnostics_to_buffer(&files, diags, /* ansi_color */ false)
        }
    } else {
        vec![]
    };

    let save_diags = read_bool_env_var(KEEP_TMP);
    let update_baseline = read_env_update_baseline();

    let rendered_diags = std::str::from_utf8(&diag_buffer)?;
    if save_diags {
        fs::write(out_path, &diag_buffer)?;
    }

    let mut options = InstaOptions::new();
    options.info = Some(test_info);
    options.suffix = test_kind;
    insta_assert! {
        name: test_name,
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
