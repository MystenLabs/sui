// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use move_binary_format::{
    compatibility::{Compatibility, InclusionCheck},
    normalized, CompiledModule,
};
use sui_move_build::{BuildConfig, SuiPackageHooks};

pub const TEST_DIR: &str = "tests";

fn run_test(path: &Path) -> datatest_stable::Result<()> {
    move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
    let mut pathbuf = path.to_path_buf();
    pathbuf.pop();
    pathbuf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(pathbuf);
    let base_path = pathbuf.join("base");
    let upgraded_path = pathbuf.join("upgraded");

    let base = compile(&base_path)?;
    let base_normalized = normalize(&base);

    let upgraded = compile(&upgraded_path)?;
    let upgraded_normalized = normalize(&upgraded);

    check_all_compatibilities(
        base_normalized,
        upgraded_normalized,
        pathbuf.file_name().unwrap().to_string_lossy().to_string(),
    )
}

fn compile(path: &Path) -> anyhow::Result<Vec<CompiledModule>> {
    Ok(BuildConfig::new_for_testing()
        .build(path)
        .unwrap()
        .into_modules())
}

fn normalize(modules: &[CompiledModule]) -> Vec<normalized::Module> {
    modules.iter().map(normalized::Module::new).collect()
}

fn check_all_compatibilities(
    base: Vec<normalized::Module>,
    upgraded: Vec<normalized::Module>,
    name: String,
) -> datatest_stable::Result<()> {
    assert_eq!(base.len(), upgraded.len());

    let compatibility_types = [
        // Full compat skip check private entry linking
        Compatibility::upgrade_check(),
        // Full compat but allow any new abilities
        Compatibility::full_check(),
        // Full compat only disallow new key ability
        Compatibility::framework_upgrade_check(),
        Compatibility::no_check(),
    ];

    let mut results = compatibility_types
        .iter()
        .map(|compat| {
            let compatibility_checks: Vec<_> = base
                .iter()
                .zip(upgraded.iter())
                .map(|(base, upgraded)| {
                    format!(
                        "{}::{}:\n\tbase->upgrade: {}\n\tupgrade->base: {}",
                        base.address,
                        base.name,
                        compat.check(base, upgraded).is_ok(),
                        compat.check(upgraded, base).is_ok()
                    )
                })
                .collect();

            format!(
                "====\n{:?}\n{}\n====",
                compat,
                compatibility_checks.join("\n")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let inclusion_results = [InclusionCheck::Equal, InclusionCheck::Subset]
        .iter()
        .map(|compat| {
            let compatibility_checks: Vec<_> = base
                .iter()
                .zip(upgraded.iter())
                .map(|(base, upgraded)| {
                    format!(
                        "{}::{}:\n\tbase->upgrade: {}\n\tupgrade->base: {}",
                        base.address,
                        base.name,
                        compat.check(base, upgraded).is_ok(),
                        compat.check(upgraded, base).is_ok()
                    )
                })
                .collect();

            format!(
                "====\n{:?}\n{}\n====",
                compat,
                compatibility_checks.join("\n")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    results.push_str(&inclusion_results);
    insta::assert_snapshot!(name, results);
    Ok(())
}

datatest_stable::harness!(run_test, TEST_DIR, r".*\.package$");
