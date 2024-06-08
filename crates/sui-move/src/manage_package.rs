// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::bail;
use clap::Parser;
use std::path::{Path, PathBuf};

use move_cli::base;
use move_package::{
    lock_file::{self, LockFile},
    source_package::layout::SourcePackageLayout,
    BuildConfig,
};
use sui_types::base_types::ObjectID;

const NO_LOCK_FILE: &str = "Expected a `Move.lock` file to exist in the package path, \
                            but none found. Consider running `sui move build` to \
                            generate the `Move.lock` file in the package directory.";

/// Record addresses (Object IDs) for where this package is published on chain (this command sets variables in
/// Move.lock).
#[derive(Parser)]
#[group(id = "sui-move-manage-package")]
pub struct ManagePackage {
    #[clap(long)]
    /// The environment to associate this package information with (consider using `sui client active-env`).
    pub environment: String,
    #[clap(long = "network-id")]
    /// The network chain identifer. Use '35834a8a' for mainnet.
    pub chain_id: String,
    #[clap(long = "original-id", value_parser = ObjectID::from_hex_literal)]
    /// The original address (Object ID) where this package is published.
    pub original_id: ObjectID,
    #[clap(long = "latest-id", value_parser = ObjectID::from_hex_literal)]
    /// The most recent address (Object ID) where this package is published. It is the same as 'original-id' if the
    /// package is immutable and published once. It is different from 'original-id' if the package has been upgraded to
    /// a different address.
    pub latest_id: ObjectID,
    #[clap(long = "version-number")]
    /// The version number of the published package. It is '1' if the package is immutable and published once. It is
    /// some number greater than '1' if the package has been upgraded once or more.
    pub version_number: u64,
}

impl ManagePackage {
    pub fn execute(
        self,
        package_path: Option<&Path>,
        build_config: BuildConfig,
    ) -> anyhow::Result<()> {
        let build_config = resolve_lock_file_path(build_config, package_path)?;
        let Some(lock_file) = build_config.lock_file else {
            bail!(NO_LOCK_FILE)
        };
        if !lock_file.exists() {
            bail!(NO_LOCK_FILE)
        };
        let install_dir = build_config.install_dir.unwrap_or(PathBuf::from("."));
        let mut lock = LockFile::from(install_dir.clone(), &lock_file)?;

        // Updating managed packages in the Move.lock file is controlled by distinct `Published` and `Upgraded`
        // commands. To set all relevant values, we run both commands. First use the `Published` update to set the
        // environment, chain ID, and original ID.
        lock_file::schema::update_managed_address(
            &mut lock,
            &self.environment,
            lock_file::schema::ManagedAddressUpdate::Published {
                chain_id: self.chain_id,
                original_id: self.original_id.to_string(),
            },
        )?;
        // Next use the `Upgraded` update to subsequently set the latest ID and version.
        lock_file::schema::update_managed_address(
            &mut lock,
            &self.environment,
            lock_file::schema::ManagedAddressUpdate::Upgraded {
                latest_id: self.latest_id.to_string(),
                version: self.version_number,
            },
        )?;
        lock.commit(lock_file)?;
        Ok(())
    }
}

/// Resolve Move.lock file path in package directory (where Move.toml is).
pub fn resolve_lock_file_path(
    mut build_config: BuildConfig,
    package_path: Option<&Path>,
) -> Result<BuildConfig, anyhow::Error> {
    if build_config.lock_file.is_none() {
        let package_root = base::reroot_path(package_path)?;
        let lock_file_path = package_root.join(SourcePackageLayout::Lock.path());
        build_config.lock_file = Some(lock_file_path);
    }
    Ok(build_config)
}
