// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::manage_package::resolve_lock_file_path;
use clap::Parser;
use move_cli::base;
use move_package::BuildConfig as MoveBuildConfig;
use serde_json::json;
use std::{fs, path::Path};
use sui_move_build::{check_invalid_dependencies, check_unpublished_dependencies, BuildConfig};

const LAYOUTS_DIR: &str = "layouts";
const STRUCT_LAYOUTS_FILENAME: &str = "struct_layouts.yaml";

#[derive(Parser)]
#[group(id = "sui-move-build")]
pub struct Build {
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
        path: Option<&Path>,
        build_config: MoveBuildConfig,
    ) -> anyhow::Result<()> {
        let rerooted_path = base::reroot_path(path)?;
        let build_config = resolve_lock_file_path(build_config, Some(&rerooted_path))?;
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
        let pkg = BuildConfig {
            config,
            run_bytecode_verifier: true,
            print_diags_to_stderr: true,
            chain_id: None,
        }
        .build(rerooted_path)?;
        if dump_bytecode_as_base64 {
            check_invalid_dependencies(&pkg.dependency_ids.invalid)?;
            if !with_unpublished_deps {
                check_unpublished_dependencies(&pkg.dependency_ids.unpublished)?;
            }

            let package_dependencies = pkg.get_package_dependencies_hex();
            println!(
                "{}",
                json!({
                    "modules": pkg.get_package_base64(with_unpublished_deps),
                    "dependencies": json!(package_dependencies),
                    "digest": pkg.get_package_digest(with_unpublished_deps),
                })
            )
        }

        if generate_struct_layouts {
            let layout_str = serde_yaml::to_string(&pkg.generate_struct_layouts()).unwrap();
            // store under <package_path>/build/<package_name>/layouts/struct_layouts.yaml
            let layout_filename = rerooted_path
                .join("build")
                .join(pkg.package.compiled_package_info.package_name.as_str())
                .join(LAYOUTS_DIR)
                .join(STRUCT_LAYOUTS_FILENAME);
            fs::write(layout_filename, layout_str)?
        }

        pkg.package
            .compiled_package_info
            .build_flags
            .update_lock_file_toolchain_version(rerooted_path, env!("CARGO_PKG_VERSION").into())?;

        Ok(())
    }
}
