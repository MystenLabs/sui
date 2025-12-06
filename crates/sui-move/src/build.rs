// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::{self};
use move_package_alt_compilation::build_config::BuildConfig as MoveBuildConfig;
use std::{fs, path::Path};
use sui_move_build::BuildConfig;
use sui_package_alt::find_environment;
use sui_sdk::wallet_context::WalletContext;

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
    pub async fn execute(
        &self,
        path: Option<&Path>,
        build_config: MoveBuildConfig,
        wallet: &WalletContext,
    ) -> anyhow::Result<()> {
        let rerooted_path = base::reroot_path(path)?;
        Self::execute_internal(
            &rerooted_path,
            build_config,
            self.generate_struct_layouts,
            wallet,
        )
        .await
    }

    pub async fn execute_internal(
        rerooted_path: &Path,
        config: MoveBuildConfig,
        generate_struct_layouts: bool,
        wallet: &WalletContext,
    ) -> anyhow::Result<()> {
        let environment =
            find_environment(rerooted_path, config.environment.clone(), wallet).await?;
        let pkg = BuildConfig {
            config,
            run_bytecode_verifier: true,
            print_diags_to_stderr: true,
            environment,
        }
        .build(rerooted_path)?;

        if generate_struct_layouts {
            let layout_str = serde_yaml::to_string(&pkg.generate_struct_layouts()).unwrap();
            // store under <package_path>/build/<package_name>/layouts/struct_layouts.yaml
            let dir_name = rerooted_path
                .join("build")
                .join(pkg.package.compiled_package_info.package_name.as_str())
                .join(LAYOUTS_DIR);
            let layout_filename = dir_name.join(STRUCT_LAYOUTS_FILENAME);
            fs::create_dir_all(dir_name)?;
            fs::write(layout_filename, layout_str)?
        }

        Ok(())
    }
}
