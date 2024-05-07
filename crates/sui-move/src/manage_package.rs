// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_package::BuildConfig;
use std::path::PathBuf;
use sui_types::base_types::ObjectID;

/// Record addresses (Object IDs) for where this package is published on chain (this command sets variables in Move.lock).
#[derive(Parser)]
#[group(id = "sui-move-manage-package")]
pub struct ManagePackage {
    #[clap(long)]
    /// The network chain identifer. Use '35834a8a' for mainnet.
    pub network: String,
    #[clap(long = "original-id", value_parser = ObjectID::from_hex_literal)]
    /// The original address (Object ID) where this package is published.
    pub original_id: ObjectID,
    #[clap(long = "latest-id", value_parser = ObjectID::from_hex_literal)]
    /// The most recent address (Object ID) where this package is published. It is the same as 'original-id' if the package is immutable and published once. It is different from 'original-id' if the package has been upgraded to a different address.
    pub latest_id: ObjectID,
    #[clap(long = "version-number")]
    /// The version number of the published package. It is '1' if the package is immutable and published once. It is some number greater than '1' if the package has been upgraded once or more.
    pub version_number: u64,
}

impl ManagePackage {
    pub fn execute(self, _path: Option<PathBuf>, _build_config: BuildConfig) -> anyhow::Result<()> {
        Ok(())
    }
}
