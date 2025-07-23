// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_move_build::{implicit_deps, set_sui_flavor, BuildConfig};
use sui_package_management::system_package_versions::latest_system_packages;

use move_cli::base::reroot_path;
use move_package::{
    lock_file::schema::ManagedPackage, source_package::layout::SourcePackageLayout,
    BuildConfig as MoveBuildConfig,
};

use clap::Parser;
use std::{fs::File, path::PathBuf};

/// Arguments for the (optional) build sub-command.
#[derive(Parser, Clone, Debug)]
#[clap(
    name = "build",
    about = "Build package preserving its on-chain ID.",
    rename_all = "kebab-case"
)]
pub struct BuildCmdConfig {
    /// Path to a package which the command should be run with respect to.
    #[clap(long = "path", short = 'p', global = true)]
    pub package_path: Option<PathBuf>,

    /// Chain ID to use for the build.
    #[clap(long = "chain-id", short = 'c', global = true)]
    pub chain_id: Option<String>,
}

pub fn handle_build_command(config: BuildCmdConfig) -> anyhow::Result<()> {
    let BuildCmdConfig {
        package_path,
        chain_id,
    } = config;

    let mut move_build_config = MoveBuildConfig {
        test_mode: true,
        ..Default::default()
    };

    if let Some(err_msg) = set_sui_flavor(&mut move_build_config) {
        anyhow::bail!(err_msg);
    }

    let package_root = reroot_path(package_path.as_deref())?;

    let cid = match chain_id {
        Some(id) => id,
        None => {
            let lock_file = package_root.join(SourcePackageLayout::Lock.path());
            let mut lock_file = File::open(lock_file)?;
            let managed_packages_opt = ManagedPackage::read(&mut lock_file).ok();
            let Some(managed_packages) = managed_packages_opt else {
                anyhow::bail!("No chain ID found in lock file (specify one explicitly - you can obtain it from 'sui client chain-identifier')");
            };
            // should not happen, but...
            if managed_packages.is_empty() {
                anyhow::bail!("No chain ID found in lock file (specify one explicitly - you can obtain it from 'sui client chain-identifier')");
            }

            if managed_packages.len() > 1 {
                let chain_ids = managed_packages
                    .iter()
                    .map(|(n, p)| format!("{} (for {})", p.chain_id, n))
                    .collect::<Vec<_>>()
                    .join(", ");

                anyhow::bail!(
                    "Multiple chain IDs found in lock file (specify one explicitly): {chain_ids}"
                );
            }

            managed_packages.values().next().unwrap().chain_id.clone()
        }
    };

    move_build_config.implicit_dependencies = implicit_deps(latest_system_packages());
    let pkg = BuildConfig {
        config: move_build_config,
        run_bytecode_verifier: true,
        print_diags_to_stderr: true,
        chain_id: Some(cid),
    }
    .build(&package_root)?;

    pkg.package
        .compiled_package_info
        .build_flags
        .update_lock_file_toolchain_version(&package_root, env!("CARGO_PKG_VERSION").into())?;

    Ok(())
}
