// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;

use fastx_adapter::{state_view::FastXStateView, adapter::execute};
use fastx_framework::{natives, FASTX_FRAMEWORK_ADDRESS, MOVE_STDLIB_ADDRESS};

use move_binary_format::views::ViewInternals;
use move_cli::{Command, Move};
use move_core_types::{
    account_address::AccountAddress, errmap::ErrorMapping, identifier::Identifier,
    language_storage::TypeTag, parser, transaction_argument::TransactionArgument,
    language_storage::ModuleId,
};

use structopt::StructOpt;

#[derive(StructOpt)]
pub struct FastXCli {
    #[structopt(flatten)]
    move_args: Move,

    #[structopt(subcommand)]
    cmd: FastXCommand,
}

#[derive(StructOpt)]
pub enum FastXCommand {
    /// Command that delegates to the Move CLI
    #[structopt(flatten)]
    MoveCommand(Command),

    // ... extra commands available only in fastX added below
    #[structopt(name = "run")]
    Run {
        // Path to build directory
        #[structopt(name = "build_dir")]
        build_dir: String,
        // Path to storage directory
        #[structopt(name = "storage_dir")]
        storage_dir: String,
        /// Path to module bytecode stored on disk
        // TODO: We hardcode the module address to the fastX stdlib address for now, but will fix this
        #[structopt(name = "module")]
        module: ModuleId,
        /// Name of function in that module to call
        #[structopt(name = "function", parse(try_from_str = Identifier::new))]
        function: Identifier,
        /// Sender of the transaction
        #[structopt(name = "sender", parse(try_from_str = AccountAddress::from_hex_literal))]
        sender: AccountAddress,
        /// Arguments to the transaction
        #[structopt(long = "args", parse(try_from_str = parser::parse_transaction_argument))]
        args: Vec<TransactionArgument>,
        /// Type arguments to the transaction
        #[structopt(long = "type-args", parse(try_from_str = parser::parse_type_tag))]
        type_args: Vec<TypeTag>,
        /// Maximum number of gas units to be consumed by execution.
        /// When the budget is exhaused, execution will abort.
        /// By default, no `gas-budget` is specified and gas metering is disabled.
        #[structopt(long = "gas-budget", short = "g")]
        gas_budget: Option<u64>,
    },
}

fn main() -> Result<()> {
    // TODO: read this from the build artifacts so we can give better error messages
    let error_descriptions: ErrorMapping = ErrorMapping::default();
    // TODO: less hacky way of doing this?
    let natives = natives::all_natives(MOVE_STDLIB_ADDRESS, FASTX_FRAMEWORK_ADDRESS);

    let args = FastXCli::from_args();
    use FastXCommand::*;
    match args.cmd {
        MoveCommand(cmd) => move_cli::run_cli(natives, &error_descriptions, &args.move_args, &cmd),
        Run { build_dir, storage_dir, module, function, sender,
                args, type_args, gas_budget } => {
            let mut _state_view = FastXStateView::create(&build_dir, &storage_dir)?;
            execute(&mut _state_view, &module, &function, sender, object_args, pure_args, type_args, gas_budget);
            unimplemented!("Fixme: local adapter")
        }
    }
}
