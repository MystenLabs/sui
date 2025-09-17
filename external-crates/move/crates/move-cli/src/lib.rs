// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod base;
pub mod sandbox;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

use crate::base::test::Test;
use base::{
    build::Build, coverage::Coverage, disassemble::Disassemble, docgen::Docgen, migrate::Migrate,
    new::New, profile::Profile, summary::Summary,
};

use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use move_package_alt::flavor::MoveFlavor;
use move_package_alt_compilation::build_config::BuildConfig;
use move_vm_runtime::native_functions::NativeFunction;
use move_vm_test_utils::gas_schedule::CostTable;

/// Default directory where saved Move resources live
pub const DEFAULT_STORAGE_DIR: &str = "storage";

/// Default directory for build output
pub const DEFAULT_BUILD_DIR: &str = ".";

type NativeFunctionRecord = (AccountAddress, Identifier, Identifier, NativeFunction);

#[derive(Parser)]
#[clap(author, version, about)]
pub struct Move {
    /// Path to a package which the command should be run with respect to.
    #[clap(long = "path", short = 'p', global = true)]
    pub package_path: Option<PathBuf>,

    /// Print additional diagnostics if available.
    #[clap(short = 'v', global = true)]
    pub verbose: bool,

    /// Package build options
    #[clap(flatten)]
    pub build_config: BuildConfig,
}

/// MoveCLI is the CLI that will be executed by the `move-cli` command
/// The `cmd` argument is added here rather than in `Move` to make it
/// easier for other crates to extend `move-cli`
#[derive(Parser)]
pub struct MoveCLI {
    #[clap(flatten)]
    pub move_args: Move,

    #[clap(subcommand)]
    pub cmd: Command,
}

#[derive(Parser)]
pub enum Command {
    Build(Build),
    Coverage(Coverage),
    Disassemble(Disassemble),
    Docgen(Docgen),
    Migrate(Migrate),
    New(New),
    Test(Test),
    Profile(Profile),
    /// Execute a sandbox command.
    #[clap(name = "sandbox")]
    Sandbox {
        /// Directory storing Move resources, events, and module bytecodes produced by module publishing
        /// and script execution.
        #[clap(long, default_value = DEFAULT_STORAGE_DIR)]
        storage_dir: PathBuf,
        #[clap(subcommand)]
        cmd: sandbox::cli::SandboxCommand,
    },
    Summary(Summary),
}

pub async fn run_cli<F: MoveFlavor>(
    natives: Vec<NativeFunctionRecord>,
    cost_table: &CostTable,
    move_args: Move,
    cmd: Command,
) -> Result<()> {
    // TODO: right now, the gas metering story for move-cli (as a library) is a bit of a mess.
    //         1. It's still using the old CostTable.
    //         2. The CostTable only affects sandbox runs, but not unit tests, which use a unit cost table.
    match cmd {
        Command::Build(c) => {
            c.execute::<F>(move_args.package_path.as_deref(), move_args.build_config)
                .await
        }
        Command::Coverage(c) => {
            c.execute::<F>(move_args.package_path.as_deref(), move_args.build_config)
                .await
        }
        Command::Disassemble(c) => {
            c.execute::<F>(move_args.package_path.as_deref(), move_args.build_config)
                .await
        }
        Command::Docgen(c) => {
            c.execute::<F>(move_args.package_path.as_deref(), move_args.build_config)
                .await
        }
        Command::Migrate(c) => {
            c.execute::<F>(move_args.package_path.as_deref(), move_args.build_config)
                .await
        }
        Command::New(c) => c.execute_with_defaults(move_args.package_path.as_deref()),
        Command::Profile(c) => c.execute(),
        Command::Test(c) => {
            c.execute::<F>(
                move_args.package_path.as_deref(),
                move_args.build_config,
                natives,
                Some(cost_table.clone()),
            )
            .await
        }
        Command::Sandbox { storage_dir, cmd } => {
            cmd.handle_command::<F>(natives, cost_table, &move_args, &storage_dir)
                .await
        }
        Command::Summary(summary) => {
            summary
                .execute::<F, ()>(
                    move_args.package_path.as_deref(),
                    move_args.build_config,
                    None,
                )
                .await
        }
    }
}

pub async fn move_cli<F: MoveFlavor>(
    natives: Vec<NativeFunctionRecord>,
    cost_table: &CostTable,
) -> Result<()> {
    let args = MoveCLI::parse();
    run_cli::<F>(natives, cost_table, args.move_args, args.cmd).await
}
