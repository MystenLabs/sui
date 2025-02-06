// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::bail;
use move_command_line_common::testing::insta_assert;
use move_package::{
    compilation::{build_plan::BuildPlan, compiled_package::CompiledPackageInfo},
    package_hooks,
    package_hooks::PackageHooks,
    package_hooks::PackageIdentifier,
    resolution::resolution_graph::Package,
    source_package::parsed_manifest::{OnChainInfo, PackageDigest, SourceManifest},
    BuildConfig,
};
use move_symbol_pool::Symbol;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::{tempdir, TempDir};

pub fn run_test(path: &Path) -> datatest_stable::Result<()> {
    if path.iter().any(|part| part == "deps_only") {
        return Ok(());
    }

    let kind = path.extension().unwrap().to_string_lossy();
    let toml_path = path.with_extension("toml");
    Test::from_path_with_kind(&toml_path, &kind)?.run()
}

struct Test<'a> {
    kind: &'a str,
    toml_path: &'a Path,
    output_dir: TempDir,
}

impl Test<'_> {
    fn from_path_with_kind<'a>(
        toml_path: &'a Path,
        kind: &'a str,
    ) -> datatest_stable::Result<Test<'a>> {
        dbg!(&toml_path);
        Ok(Test {
            toml_path,
            kind,
            output_dir: tempdir()?,
        })
    }

    fn run(&self) -> datatest_stable::Result<()> {
        package_hooks::register_package_hooks(Box::new(TestHooks()));
        let output = self.output().unwrap_or_else(|err| format!("{:#}\n", err));
        insta_assert! {
            input_path: self.toml_path,
            contents: output,
            suffix: self.kind,
        };

        Ok(())
    }

    fn output(&self) -> anyhow::Result<String> {
        let out_path = self.output_dir.path().to_path_buf();
        let lock_path = out_path.join("Move.lock");

        let config = BuildConfig {
            dev_mode: true,
            test_mode: false,
            generate_docs: false,
            install_dir: Some(out_path),
            force_recompilation: false,
            lock_file: ["locked", "notlocked"]
                .contains(&self.kind)
                .then(|| lock_path.clone()),
            ..Default::default()
        };

        let mut progress = Vec::new();
        let resolved_package =
            config.resolution_graph_for_package(self.toml_path, None, &mut progress);

        Ok(match self.kind {
            "progress" => String::from_utf8(progress)?,

            "locked" => fs::read_to_string(&lock_path)?,

            "notlocked" if lock_path.is_file() => {
                bail!("Unexpected lock file");
            }

            "notlocked" => "Lock file uncommitted\n".to_string(),

            "compiled" => {
                let mut pkg = BuildPlan::create(resolved_package?)?
                    .compile(&mut progress, |compile| compile)?;
                scrub_compiled_package(&mut pkg.compiled_package_info);
                format!("{:#?}\n", pkg.compiled_package_info)
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
        vec!["test_hooks_field".to_owned(), "version".to_owned()]
    }

    fn resolve_on_chain_dependency(
        &self,
        dep_name: Symbol,
        info: &OnChainInfo,
    ) -> anyhow::Result<()> {
        bail!("TestHooks resolve dep {:?} = {:?}", dep_name, info.id,)
    }

    fn custom_resolve_pkg_id(
        &self,
        manifest: &SourceManifest,
    ) -> anyhow::Result<PackageIdentifier> {
        let name = manifest.package.name.to_string();
        if name.ends_with("-rename") {
            Ok(Symbol::from(name.replace("-rename", "-resolved")))
        } else {
            Ok(manifest.package.name)
        }
    }

    fn resolve_version(&self, manifest: &SourceManifest) -> anyhow::Result<Option<Symbol>> {
        Ok(manifest
            .package
            .custom_properties
            .get(&Symbol::from("version"))
            .map(|v| Symbol::from(v.as_ref())))
    }
}
// &["progress", "resolved", "locked", "notlocked", "compiled"];
datatest_stable::harness!(
    run_test,
    "tests/test_sources",
    r".*\.progress$",
    run_test,
    "tests/test_sources",
    r".*\.resolved$",
    run_test,
    "tests/test_sources",
    r".*\.locked$",
    run_test,
    "tests/test_sources",
    r".*\.notlocked$",
    run_test,
    "tests/test_sources",
    r".*\.compiled$",
);
