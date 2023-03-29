// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use base::{
    build::Build, coverage::Coverage, disassemble::Disassemble, docgen::Docgen, errmap::Errmap,
    info::Info, new::New, prove::Prove, test::Test,
};
use move_package::BuildConfig;

pub mod base;
pub mod experimental;
pub mod sandbox;

/// Default directory where saved Move resources live
pub const DEFAULT_STORAGE_DIR: &str = "storage";

/// Default directory for build output
pub const DEFAULT_BUILD_DIR: &str = ".";

/// Extension for resource and event files, which are in BCS format
const BCS_EXTENSION: &str = "bcs";

use anyhow::Result;
use clap::Parser;
use move_core_types::{
    account_address::AccountAddress, errmap::ErrorMapping, identifier::Identifier,
};
use move_vm_runtime::native_functions::NativeFunction;
use move_vm_test_utils::gas_schedule::CostTable;
use std::path::PathBuf;

type NativeFunctionRecord = (AccountAddress, Identifier, Identifier, NativeFunction);

#[derive(Parser)]
#[clap(author, version, about)]
pub struct Move {
    /// Path to a package which the command should be run with respect to.
    #[clap(long = "path", short = 'p', global = true, parse(from_os_str))]
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
    Errmap(Errmap),
    Info(Info),
    New(New),
    Prove(Prove),
    Test(Test),
    /// Execute a sandbox command.
    #[clap(name = "sandbox")]
    Sandbox {
        /// Directory storing Move resources, events, and module bytecodes produced by module publishing
        /// and script execution.
        #[clap(long, default_value = DEFAULT_STORAGE_DIR, parse(from_os_str))]
        storage_dir: PathBuf,
        #[clap(subcommand)]
        cmd: sandbox::cli::SandboxCommand,
    },
    /// (Experimental) Run static analyses on Move source or bytecode.
    #[clap(name = "experimental")]
    Experimental {
        /// Directory storing Move resources, events, and module bytecodes produced by module publishing
        /// and script execution.
        #[clap(long, default_value = DEFAULT_STORAGE_DIR, parse(from_os_str))]
        storage_dir: PathBuf,
        #[clap(subcommand)]
        cmd: experimental::cli::ExperimentalCommand,
    },
}

pub fn run_cli(
    natives: Vec<NativeFunctionRecord>,
    cost_table: &CostTable,
    error_descriptions: &ErrorMapping,
    move_args: Move,
    cmd: Command,
) -> Result<()> {
    // TODO: right now, the gas metering story for move-cli (as a library) is a bit of a mess.
    //         1. It's still using the old CostTable.
    //         2. The CostTable only affects sandbox runs, but not unit tests, which use a unit cost table.
    match cmd {
        Command::Build(c) => c.execute(move_args.package_path, move_args.build_config),
        Command::Coverage(c) => c.execute(move_args.package_path, move_args.build_config),
        Command::Disassemble(c) => c.execute(move_args.package_path, move_args.build_config),
        Command::Docgen(c) => c.execute(move_args.package_path, move_args.build_config),
        Command::Errmap(c) => c.execute(move_args.package_path, move_args.build_config),
        Command::Info(c) => c.execute(move_args.package_path, move_args.build_config),
        Command::New(c) => c.execute_with_defaults(move_args.package_path),
        Command::Prove(c) => c.execute(move_args.package_path, move_args.build_config),
        Command::Test(c) => c.execute(
            move_args.package_path,
            move_args.build_config,
            natives,
            Some(cost_table.clone()),
        ),
        Command::Sandbox { storage_dir, cmd } => cmd.handle_command(
            natives,
            cost_table,
            error_descriptions,
            &move_args,
            &storage_dir,
        ),
        Command::Experimental { storage_dir, cmd } => cmd.handle_command(&move_args, &storage_dir),
    }
}

pub fn move_cli(
    natives: Vec<NativeFunctionRecord>,
    cost_table: &CostTable,
    error_descriptions: &ErrorMapping,
) -> Result<()> {
    let args = MoveCLI::parse();
    run_cli(
        natives,
        cost_table,
        error_descriptions,
        args.move_args,
        args.cmd,
    )
}
