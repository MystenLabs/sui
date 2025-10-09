// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use move_analyzer::analyzer;
use move_compiler::editions::Flavor;
use sui_move_build::{implicit_deps, SuiPackageHooks};
use sui_package_management::system_package_versions::latest_system_packages;

// Define the `GIT_REVISION` and `VERSION` consts
bin_version::bin_version!();

#[derive(Parser)]
#[clap(
    name = env!("CARGO_BIN_NAME"),
    rename_all = "kebab-case",
    author,
    version = VERSION,
)]
struct App {}

fn main() {
    App::parse();
    let sui_implicit_deps = implicit_deps(latest_system_packages());
    let flavor = Flavor::Sui;
    let sui_pkg_hooks = Box::new(SuiPackageHooks);
    analyzer::run(sui_implicit_deps, Some(flavor), Some(sui_pkg_hooks));
}
