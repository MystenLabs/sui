// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use sui_framework_snapshot::SnapshotManifest;

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub enum SnapshotCommand {
    /// This is run when we are cutting a new release branch from the main branch, usually by the PE team.
    /// It will generate a bytecode snapshot for the latest framework, and update the manifest file
    /// with the corresponding git revision and protocol version.
    /// If you need to run this but not sure what you are doing, please first consult with the PE team.
    #[clap(name = "branch-cut")]
    BranchCut,
    /// This is run when we just published a new release to testnet or mainnet.
    /// It will update the latest entry in the bytecode snapshot manifest to indicate that this version is in production.
    #[clap(name = "release")]
    Release,
}

fn main() {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_log_level("off,sui_framework_snapshot=info")
        .with_env()
        .init();

    let args = SnapshotCommand::parse();
    match args {
        SnapshotCommand::BranchCut => branch_cut(),
        SnapshotCommand::Release => release(),
    }
}

fn branch_cut() {
    let mut snapshot = SnapshotManifest::new();
    snapshot.generate_new_snapshot();
}

fn release() {
    let mut snapshot = SnapshotManifest::new();
    snapshot.release_latest_snapshot();
}
