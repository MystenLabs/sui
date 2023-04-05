// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, bail};
use move_command_line_common::testing::{
    add_update_baseline_fix, format_diff, read_env_update_baseline,
};
use move_package::{
    compilation::{
        build_plan::BuildPlan, compiled_package::CompiledPackageInfo, model_builder::ModelBuilder,
    },
    package_hooks,
    package_hooks::PackageHooks,
    resolution::resolution_graph::Package,
    source_package::parsed_manifest::{CustomDepInfo, PackageDigest},
    BuildConfig, ModelConfig,
};
use move_symbol_pool::Symbol;
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};
use tempfile::{tempdir, TempDir};

const EXTENSIONS: &[&str] = &[
    "progress",
    "resolved",
    "locked",
    "notlocked",
    "compiled",
    "modeled",
];

pub fn run_test(path: &Path) -> datatest_stable::Result<()> {
    if path.iter().any(|part| part == "deps_only") {
        return Ok(());
    }

    let mut tests = EXTENSIONS
        .iter()
        .filter_map(|kind| Test::from_path_with_kind(path, kind).transpose())
        .peekable();

    if tests.peek().is_none() {
        return Err(anyhow!(
            "No snapshot file found for {:?}, please add a file with the same basename and one \
             of the following extensions: {:#?}\n\n\
             You probably want to re-run with `env UPDATE_BASELINE=1` after adding this file.",
            path,
            EXTENSIONS,
        )
        .into());
    }

    for test in tests {
        test?.run()?
    }

    Ok(())
}

struct Test<'a> {
    toml_path: &'a Path,
    expected: PathBuf,
    output_dir: TempDir,
}

impl Test<'_> {
    fn from_path_with_kind<'p>(
        toml_path: &'p Path,
        kind: &str,
    ) -> datatest_stable::Result<Option<Test<'p>>> {
        let expected = toml_path.with_extension(kind);
        if !expected.is_file() {
            Ok(None)
        } else {
            Ok(Some(Test {
                toml_path,
                expected,
                output_dir: tempdir()?,
            }))
        }
    }

    fn run(&self) -> datatest_stable::Result<()> {
        package_hooks::register_package_hooks(Box::new(TestHooks()));
        let update_baseline = read_env_update_baseline();

        let output = self.output().unwrap_or_else(|err| format!("{:#}\n", err));

        if update_baseline {
            fs::write(&self.expected, &output)?;
            return Ok(());
        }

        let expected = fs::read_to_string(&self.expected)?;
        if expected != output {
            return Err(anyhow!(add_update_baseline_fix(format!(
                "Expected outputs differ for {:?}:\n{}",
                self.expected,
                format_diff(expected, output),
            )))
            .into());
        }

        Ok(())
    }

    fn output(&self) -> anyhow::Result<String> {
        let Some(ext) = self.expected.extension().and_then(OsStr::to_str) else {
            bail!("Unexpected snapshot file extension: {:?}", self.expected.extension());
        };

        let out_path = self.output_dir.path().to_path_buf();
        let lock_path = out_path.join("Move.lock");

        let config = BuildConfig {
            dev_mode: true,
            test_mode: false,
            generate_docs: false,
            generate_abis: false,
            install_dir: Some(out_path),
            force_recompilation: false,
            lock_file: ["locked", "notlocked"]
                .contains(&ext)
                .then(|| lock_path.clone()),
            ..Default::default()
        };

        let mut progress = Vec::new();
        let resolved_package = config.resolution_graph_for_package(self.toml_path, &mut progress);

        Ok(match ext {
            "progress" => String::from_utf8(progress)?,

            "locked" => fs::read_to_string(&lock_path)?,

            "notlocked" if lock_path.is_file() => {
                bail!("Unexpected lock file");
            }

            "notlocked" => "Lock file uncommitted\n".to_string(),

            "compiled" => {
                let mut pkg = BuildPlan::create(resolved_package?)?.compile(&mut progress)?;
                scrub_compiled_package(&mut pkg.compiled_package_info);
                format!("{:#?}\n", pkg.compiled_package_info)
            }

            "modeled" => {
                ModelBuilder::create(
                    resolved_package?,
                    ModelConfig {
                        all_files_as_targets: false,
                        target_filter: None,
                    },
                )
                .build_model()?;
                "Built model\n".to_string()
            }

            "resolved" => {
                let mut resolved_package = resolved_package?;
                for package in resolved_package.package_table.values_mut() {
                    scrub_resolved_package(package)
                }

                scrub_build_config(&mut resolved_package.build_options);
                format!("{:#?}\n", resolved_package)
            }

            ext => bail!("Unrecognised snapshot type: '{ext}'"),
        })
    }
}

fn scrub_build_config(config: &mut BuildConfig) {
    config.install_dir = Some(PathBuf::from("ELIDED_FOR_TEST"));
    config.lock_file = Some(PathBuf::from("ELIDED_FOR_TEST"));
}

fn scrub_compiled_package(pkg: &mut CompiledPackageInfo) {
    pkg.source_digest = Some(PackageDigest::from("ELIDED_FOR_TEST"));
    scrub_build_config(&mut pkg.build_flags);
}

fn scrub_resolved_package(pkg: &mut Package) {
    pkg.package_path = PathBuf::from("ELIDED_FOR_TEST");
    pkg.source_digest = PackageDigest::from("ELIDED_FOR_TEST");
}

/// Some dummy hooks for testing the hook mechanism
struct TestHooks();

impl PackageHooks for TestHooks {
    fn custom_package_info_fields(&self) -> Vec<String> {
        vec!["test_hooks_field".to_owned()]
    }

    fn custom_dependency_key(&self) -> Option<String> {
        Some("custom".to_owned())
    }

    fn resolve_custom_dependency(
        &self,
        dep_name: Symbol,
        info: &CustomDepInfo,
    ) -> anyhow::Result<()> {
        bail!(
            "TestHooks resolve dep {:?} = {:?} {:?} {:?} {:?}",
            dep_name,
            info.node_url,
            info.package_name,
            info.package_address,
            info.subdir.to_string_lossy(),
        )
    }
}

datatest_stable::harness!(run_test, "tests/test_sources", r".*\.toml$");
