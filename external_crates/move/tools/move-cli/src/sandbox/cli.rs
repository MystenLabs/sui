// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    sandbox::{
        self,
        utils::{on_disk_state_view::OnDiskStateView, PackageContext},
    },
    Move, NativeFunctionRecord, DEFAULT_BUILD_DIR,
};
use anyhow::Result;
use clap::Parser;
use move_core_types::{
    errmap::ErrorMapping, language_storage::TypeTag, parser,
    transaction_argument::TransactionArgument,
};
use move_package::compilation::package_layout::CompiledPackageLayout;
use move_vm_test_utils::gas_schedule::CostTable;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Parser)]
pub enum SandboxCommand {
    /// Compile the modules in this package and its dependencies and publish the resulting bytecodes in global storage.
    #[clap(name = "publish")]
    Publish {
        /// By default, code that might cause breaking changes for bytecode
        /// linking or data layout compatibility checks will not be published.
        /// Set this flag to ignore breaking changes checks and publish anyway.
        #[clap(long = "ignore-breaking-changes")]
        ignore_breaking_changes: bool,
        /// If set, publish not only the modules in this package but also
        /// modules in all its dependencies.
        #[clap(long = "with-deps")]
        with_deps: bool,
        /// If set, all modules at once as a bundle. The default is to publish
        /// modules sequentially.
        #[clap(long = "bundle")]
        bundle: bool,
        /// Manually specify the publishing order of modules.
        #[clap(
            long = "override-ordering",
            takes_value(true),
            multiple_values(true),
            multiple_occurrences(true)
        )]
        override_ordering: Option<Vec<String>>,
    },
    /// Run a Move script that reads/writes resources stored on disk in `storage-dir`.
    /// The script must be defined in the package.
    #[clap(name = "run")]
    Run {
        /// Path to .mv file containing either script or module bytecodes. If the file is a module, the
        /// `script_name` parameter must be set.
        #[clap(name = "script", parse(from_os_str))]
        script_file: PathBuf,
        /// Name of the script function inside `script_file` to call. Should only be set if `script_file`
        /// points to a module.
        #[clap(name = "name")]
        script_name: Option<String>,
        /// Possibly-empty list of signers for the current transaction (e.g., `account` in
        /// `main(&account: signer)`). Must match the number of signers expected by `script_file`.
        #[clap(
            long = "signers",
            takes_value(true),
            multiple_values(true),
            multiple_occurrences(true)
        )]
        signers: Vec<String>,
        /// Possibly-empty list of arguments passed to the transaction (e.g., `i` in
        /// `main(i: u64)`). Must match the arguments types expected by `script_file`.
        /// Supported argument types are
        /// bool literals (true, false),
        /// u64 literals (e.g., 10, 58),
        /// address literals (e.g., 0x12, 0x0000000000000000000000000000000f),
        /// hexadecimal strings (e.g., x"0012" will parse as the vector<u8> value [00, 12]), and
        /// ASCII strings (e.g., 'b"hi" will parse as the vector<u8> value [68, 69]).
        #[clap(
            long = "args",
            parse(try_from_str = parser::parse_transaction_argument),
            takes_value(true),
            multiple_values(true),
            multiple_occurrences(true)
        )]
        args: Vec<TransactionArgument>,
        /// Possibly-empty list of type arguments passed to the transaction (e.g., `T` in
        /// `main<T>()`). Must match the type arguments kinds expected by `script_file`.
        #[clap(
            long = "type-args",
            parse(try_from_str = parser::parse_type_tag),
            takes_value(true),
            multiple_values(true),
            multiple_occurrences(true)
        )]
        type_args: Vec<TypeTag>,
        /// Maximum number of gas units to be consumed by execution.
        /// When the budget is exhaused, execution will abort.
        /// By default, no `gas-budget` is specified and gas metering is disabled.
        #[clap(long = "gas-budget", short = 'g')]
        gas_budget: Option<u64>,
        /// If set, the effects of executing `script_file` (i.e., published, updated, and
        /// deleted resources) will NOT be committed to disk.
        #[clap(long = "dry-run", short = 'n')]
        dry_run: bool,
    },
    /// Run expected value tests using the given batch file.
    #[clap(name = "exp-test")]
    Test {
        /// Use an ephemeral directory to serve as the testing workspace.
        /// By default, the directory containing the `args.txt` will be the workspace.
        #[clap(long = "use-temp-dir")]
        use_temp_dir: bool,
        /// Show coverage information after tests are done.
        /// By default, coverage will not be tracked nor shown.
        #[clap(long = "track-cov")]
        track_cov: bool,
    },
    /// View Move resources, events files, and modules stored on disk.
    #[clap(name = "view")]
    View {
        /// Path to a resource, events file, or module stored on disk.
        #[clap(name = "file", parse(from_os_str))]
        file: PathBuf,
    },
    /// Delete all resources, events, and modules stored on disk under `storage-dir`.
    /// Does *not* delete anything in `src`.
    Clean {},
    /// Run well-formedness checks on the `storage-dir` and `install-dir` directories.
    #[clap(name = "doctor")]
    Doctor {},
    /// Generate struct layout bindings for the modules stored on disk under `storage-dir`
    // TODO: expand this to generate script bindings, etc.?.
    #[clap(name = "generate")]
    Generate {
        #[clap(subcommand)]
        cmd: GenerateCommand,
    },
}

#[derive(Parser)]
pub enum GenerateCommand {
    /// Generate struct layout bindings for the modules stored on disk under `storage-dir`.
    #[clap(name = "struct-layouts")]
    StructLayouts {
        /// Path to a module stored on disk.
        #[clap(long, parse(from_os_str))]
        module: PathBuf,
        /// If set, generate bindings for the specified struct and type arguments. If unset,
        /// generate bindings for all closed struct definitions.
        #[clap(flatten)]
        options: StructLayoutOptions,
    },
}
#[derive(Parser)]
pub struct StructLayoutOptions {
    /// Generate layout bindings for this struct.
    #[clap(long = "struct")]
    struct_: Option<String>,
    /// Generate layout bindings for `struct` bound to these type arguments.
    #[clap(
        long = "type-args",
        parse(try_from_str = parser::parse_type_tag),
        requires="struct",
        takes_value(true),
        multiple_values(true),
        multiple_occurrences(true)
    )]
    type_args: Option<Vec<TypeTag>>,
    /// If set, replace all Move source syntax separators ("::" for address/struct/module name
    /// separation, "<", ">", and "," for generics separation) with this string.
    /// If unset, use the same syntax as Move source
    #[clap(long = "separator")]
    separator: Option<String>,
    /// If true, do not include addresses in fully qualified type names.
    /// If there is a name conflict (e.g., the registry we're building has both
    /// 0x1::M::T and 0x2::M::T), layout generation will fail when this option is true.
    #[clap(long = "omit-addresses")]
    omit_addresses: bool,
    /// If true, do not include phantom types in fully qualified type names, since they do not contribute to the layout
    /// E.g., if we have `struct S<phantom T> { u: 64 }` and try to generate bindings for this struct with `T = u8`,
    /// the name for `S` in the registry will be `S<u64>` when this option is false, and `S` when this option is true
    #[clap(long = "ignore-phantom-types")]
    ignore_phantom_types: bool,
    /// If set, generate bindings only for the struct passed in.
    /// When unset, generates bindings for the struct and all of its transitive dependencies.
    #[clap(long = "shallow")]
    shallow: bool,
}

impl SandboxCommand {
    pub fn handle_command(
        &self,
        natives: Vec<NativeFunctionRecord>,
        cost_table: &CostTable,
        error_descriptions: &ErrorMapping,
        move_args: &Move,
        storage_dir: &Path,
    ) -> Result<()> {
        match self {
            SandboxCommand::Publish {
                ignore_breaking_changes,
                with_deps,
                bundle,
                override_ordering,
            } => {
                let context =
                    PackageContext::new(&move_args.package_path, &move_args.build_config)?;
                let state = context.prepare_state(storage_dir)?;
                sandbox::commands::publish(
                    natives,
                    cost_table,
                    &state,
                    context.package(),
                    *ignore_breaking_changes,
                    *with_deps,
                    *bundle,
                    override_ordering.as_ref().map(|o| o.as_slice()),
                    move_args.verbose,
                )
            }
            SandboxCommand::Run {
                script_file,
                script_name,
                signers,
                args,
                type_args,
                gas_budget,
                dry_run,
            } => {
                let context =
                    PackageContext::new(&move_args.package_path, &move_args.build_config)?;
                let state = context.prepare_state(storage_dir)?;
                sandbox::commands::run(
                    natives,
                    cost_table,
                    error_descriptions,
                    &state,
                    context.package(),
                    script_file,
                    script_name,
                    signers,
                    args,
                    type_args.to_vec(),
                    *gas_budget,
                    *dry_run,
                    move_args.verbose,
                )
            }
            SandboxCommand::Test {
                use_temp_dir,
                track_cov,
            } => sandbox::commands::run_all(
                move_args
                    .package_path
                    .as_deref()
                    .unwrap_or_else(|| Path::new(".")),
                &std::env::current_exe()?,
                *use_temp_dir,
                *track_cov,
            ),
            SandboxCommand::View { file } => {
                let state = PackageContext::new(&move_args.package_path, &move_args.build_config)?
                    .prepare_state(storage_dir)?;
                sandbox::commands::view(&state, file)
            }
            SandboxCommand::Clean {} => {
                // delete storage
                let storage_dir = Path::new(storage_dir);
                if storage_dir.exists() {
                    fs::remove_dir_all(storage_dir)?;
                }

                // delete build
                let build_dir = Path::new(
                    &move_args
                        .build_config
                        .install_dir
                        .as_ref()
                        .unwrap_or(&PathBuf::from(DEFAULT_BUILD_DIR)),
                )
                .join(CompiledPackageLayout::Root.path());
                if build_dir.exists() {
                    fs::remove_dir_all(&build_dir)?;
                }
                Ok(())
            }
            SandboxCommand::Doctor {} => {
                let state = PackageContext::new(&move_args.package_path, &move_args.build_config)?
                    .prepare_state(storage_dir)?;
                sandbox::commands::doctor(&state)
            }
            SandboxCommand::Generate { cmd } => {
                let state = PackageContext::new(&move_args.package_path, &move_args.build_config)?
                    .prepare_state(storage_dir)?;
                handle_generate_commands(cmd, &state)
            }
        }
    }
}

fn handle_generate_commands(cmd: &GenerateCommand, state: &OnDiskStateView) -> Result<()> {
    match cmd {
        GenerateCommand::StructLayouts { module, options } => {
            sandbox::commands::generate::generate_struct_layouts(
                module,
                &options.struct_,
                &options.type_args,
                options.separator.clone(),
                options.omit_addresses,
                options.ignore_phantom_types,
                options.shallow,
                state,
            )
        }
    }
}
