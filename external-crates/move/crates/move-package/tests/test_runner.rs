// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::bail;
use move_command_line_common::testing::insta_assert;
use move_package::{
    BuildConfig,
    compilation::{build_plan::BuildPlan, compiled_package::CompiledPackageInfo},
    package_hooks::{self, PackageHooks, PackageIdentifier},
    resolution::resolution_graph::Package,
    source_package::{
        manifest_parser::parse_dependencies,
        parsed_manifest::{Dependencies, OnChainInfo, PackageDigest, SourceManifest},
    },
};
use move_symbol_pool::Symbol;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::{TempDir, tempdir};

/// Resolve the package contained in the same directory as [path], and snapshot a value based
/// on the extension of [path]:
///  - ".progress": the output of the progress indicator
///  - ".locked": the contents of the lockfile
///  - ".notlocked": the nonexistence of the lockfile
///  - ".compiled": the serialized [CompiledPackageInfo] after compilation
///  - ".resolved": the serialized [ResolvedGraph] after package resolution
///
/// If a file named `path.with_extension("implicits")` exists, its contents are a toml file containing
/// additional dependencies which are included as implicit dependencencies.
pub fn run_test(path: &Path) -> datatest_stable::Result<()> {
    if path.iter().any(|part| part == "deps_only") {
        return Ok(());
    }

    let kind = path.extension().unwrap().to_string_lossy();
    let toml_path = path.with_extension("toml");
    let test = Test::from_path_with_kind(&toml_path, &kind)?;
    test.run()
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

    /// Return the value to be snapshotted, based on `self.kind`, as described in [run_test]
    fn output(&self) -> anyhow::Result<String> {
        let out_path = self.output_dir.path().to_path_buf();
        let lock_path = out_path.join("Move.lock");
        let implicits_path = self.toml_path.with_extension("implicits");

        let config = BuildConfig {
            dev_mode: true,
            test_mode: false,
            generate_docs: false,
            install_dir: Some(out_path),
            force_recompilation: false,
            lock_file: ["locked", "notlocked"]
                .contains(&self.kind)
                .then(|| lock_path.clone()),
            implicit_dependencies: load_implicits_from_file(&implicits_path),
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
                let mut pkg = BuildPlan::create(&resolved_package?)?
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

/// Return the dependencies contained in the file at `path`, if any
fn load_implicits_from_file(path: &Path) -> Dependencies {
    let deps_toml = fs::read_to_string(path).unwrap_or("# no implicit deps".to_string());

    parse_dependencies(toml::from_str(&deps_toml).unwrap()).unwrap_or_else(|e| {
        panic!("expected {path:?} to contain a toml-formatted dependencies section\n{e:?}")
    })
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
