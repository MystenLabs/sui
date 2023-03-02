// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::{self, build};
use move_package::{package_hooks::PackageHooks, source_package::parsed_manifest::CustomDepInfo};
use move_package::{
    source_package::{layout, manifest_parser},
    BuildConfig as MoveBuildConfig,
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use sui_framework_build::compiled_package::BuildConfig;

const LAYOUTS_DIR: &str = "layouts";
const STRUCT_LAYOUTS_FILENAME: &str = "struct_layouts.yaml";

#[derive(Parser)]
pub struct Build {
    #[clap(flatten)]
    pub build: build::Build,
    /// Include the contents of packages in dependencies that haven't been published (only relevant
    /// when dumping bytecode as base64)
    #[clap(long, global = true)]
    pub with_unpublished_dependencies: bool,
    /// Whether we are printing in base64.
    #[clap(long, global = true)]
    pub dump_bytecode_as_base64: bool,
    /// If true, generate struct layout schemas for
    /// all struct types passed into `entry` functions declared by modules in this package
    /// These layout schemas can be consumed by clients (e.g.,
    /// the TypeScript SDK) to enable serialization/deserialization of transaction arguments
    /// and events.
    #[clap(long, global = true)]
    pub generate_struct_layouts: bool,
}

impl Build {
    pub fn execute(
        &self,
        path: Option<PathBuf>,
        build_config: MoveBuildConfig,
    ) -> anyhow::Result<()> {
        let rerooted_path = base::reroot_path(path.clone())?;
        let build_config = resolve_lock_file_path(build_config, path.clone())?;
        let lookup = resolve_manifest_lookup(path)?;
        let _published_at = if let Some(_addr) = lookup.get(PUBLISHED_AT_MANIFEST_FIELD) {
            // TODO parse
        } else {
            // TODO decide how to handle / fail when there is no `published-at` field.
        };

        Self::execute_internal(
            &rerooted_path,
            build_config,
            self.with_unpublished_dependencies,
            self.dump_bytecode_as_base64,
            self.generate_struct_layouts,
        )
    }

    pub fn execute_internal(
        rerooted_path: &Path,
        config: MoveBuildConfig,
        with_unpublished_deps: bool,
        dump_bytecode_as_base64: bool,
        generate_struct_layouts: bool,
    ) -> anyhow::Result<()> {
        let pkg = sui_framework::build_move_package(
            rerooted_path,
            BuildConfig {
                config,
                run_bytecode_verifier: true,
                print_diags_to_stderr: true,
            },
        )?;
        if dump_bytecode_as_base64 {
            println!("{}", json!(pkg.get_package_base64(with_unpublished_deps)))
        }

        if generate_struct_layouts {
            let layout_str = serde_yaml::to_string(&pkg.generate_struct_layouts()).unwrap();
            // store under <package_path>/build/<package_name>/layouts/struct_layouts.yaml
            let mut layout_filename = pkg.path;
            layout_filename.push("build");
            layout_filename.push(pkg.package.compiled_package_info.package_name.as_str());
            layout_filename.push(LAYOUTS_DIR);
            layout_filename.push(STRUCT_LAYOUTS_FILENAME);
            fs::write(layout_filename, layout_str)?
        }

        Ok(())
    }
}

/// Resolve Move.lock file path in package directory (where Move.toml is).
pub fn resolve_lock_file_path(
    build_config: MoveBuildConfig,
    package_path: Option<PathBuf>,
) -> Result<MoveBuildConfig, anyhow::Error> {
    let package_root = base::reroot_path(package_path)?;
    let lock_file_path = package_root.join("Move.lock");
    let mut build_config = build_config;
    build_config.lock_file = Some(lock_file_path);
    Ok(build_config)
}

pub const PUBLISHED_AT_MANIFEST_FIELD: &str = "published-at";

pub struct SuiPackageHooks {}

impl PackageHooks for SuiPackageHooks {
    fn custom_package_info_fields(&self) -> Vec<String> {
        vec![PUBLISHED_AT_MANIFEST_FIELD.to_string()]
    }

    fn custom_dependency_key(&self) -> Option<String> {
        None
    }

    fn resolve_custom_dependency(
        &self,
        _dep_name: move_symbol_pool::Symbol,
        _info: &CustomDepInfo,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

pub fn resolve_manifest_lookup(
    package_path: Option<PathBuf>,
) -> anyhow::Result<BTreeMap<String, String>> {
    let package_root = base::reroot_path(package_path)?;
    let manifest_string =
        std::fs::read_to_string(package_root.join(layout::SourcePackageLayout::Manifest.path()))?;
    let toml_manifest = manifest_parser::parse_move_manifest_string(manifest_string)?;
    let manifest = manifest_parser::parse_source_manifest(toml_manifest)?;
    let lookup: BTreeMap<String, String> = manifest
        .package
        .custom_properties
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    Ok(lookup)
}
