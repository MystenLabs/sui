// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::test::UnitTestResult;
use move_package::BuildConfig;
use std::path::Path;
use sui_move_build::{SuiPackageHooks, implicit_deps, set_sui_flavor};
use sui_package_management::system_package_versions::latest_system_packages;

pub mod build;
pub mod coverage;
pub mod disassemble;
pub mod manage_package;
pub mod migrate;
pub mod new;
pub mod summary;
pub mod unit_test;

#[derive(Parser)]
pub enum Command {
    Build(build::Build),
    Coverage(coverage::Coverage),
    Disassemble(disassemble::Disassemble),
    ManagePackage(manage_package::ManagePackage),
    Migrate(migrate::Migrate),
    New(new::New),
    Test(unit_test::Test),
    Summary(summary::Summary),
}

// Additional per-command metadata that can be passed from other commands (e.g., the Sui CLI) that
// don't appear in the CLI args.
pub enum CommandMeta {
    Summary(summary::PackageSummaryMetadata),
}

pub fn execute_move_command(
    package_path: Option<&Path>,
    mut build_config: BuildConfig,
    command: Command,
    command_meta: Option<CommandMeta>,
) -> anyhow::Result<()> {
    if let Some(err_msg) = set_sui_flavor(&mut build_config) {
        anyhow::bail!(err_msg);
    }

    build_config.implicit_dependencies = implicit_deps(latest_system_packages());

    move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
    match command {
        Command::Build(c) => c.execute(package_path, build_config),
        Command::Coverage(c) => c.execute(package_path, build_config),
        Command::Disassemble(c) => c.execute(package_path, build_config),
        Command::ManagePackage(c) => c.execute(package_path, build_config),
        Command::Migrate(c) => c.execute(package_path, build_config),
        Command::New(c) => c.execute(package_path),
        Command::Summary(s) => {
            let additional_metadata = command_meta
                .map(|meta| {
                    let CommandMeta::Summary(metadata) = meta;
                    metadata
                })
                .unwrap_or_default();
            s.execute(package_path, build_config, additional_metadata)
        }

        Command::Test(c) => {
            let result = c.execute(package_path, build_config)?;

            // Return a non-zero exit code if any test failed
            if let UnitTestResult::Failure = result {
                std::process::exit(1)
            }

            Ok(())
        }
    }
}
