// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::Path};

use crate::{
    framework::{run_test_impl, CompiledState, MaybeNamedCompiledModule, MoveTestAdapter},
    tasks::{EmptyCommand, InitCommand, SyntaxChoice, TaskInput},
};
use anyhow::{anyhow, Error, Result};
use async_trait::async_trait;
use clap::Parser;
use move_binary_format::{
    errors::{Location, VMError, VMResult},
    CompiledModule,
};
use move_command_line_common::{
    address::ParsedAddress, files::verify_and_create_named_address_mapping,
};
use move_compiler::{editions::Edition, shared::PackagePaths, FullyCompiledProgram};
use move_core_types::{
    account_address::AccountAddress,
    identifier::IdentStr,
    language_storage::{ModuleId, TypeTag},
    runtime_value::MoveValue,
};
use move_stdlib::move_stdlib_named_addresses;
use move_symbol_pool::Symbol;
use move_vm_config::runtime::VMConfig;
use move_vm_runtime::{
    natives::functions::NativeFunctions, shared::serialization::SerializedReturnValues,
};
use move_vm_runtime::{
    natives::move_stdlib::{stdlib_native_functions, GasParameters},
    vm::vm::VirtualMachine,
};
use move_vm_test_utils::{gas_schedule::GasStatus, InMemoryStorage};
use once_cell::sync::Lazy;
use std::sync::Arc;

const STD_ADDR: AccountAddress = AccountAddress::ONE;

struct SimpleVMTestAdapter {
    compiled_state: CompiledState,
    storage: InMemoryStorage,
    default_syntax: SyntaxChoice,
}

#[derive(Debug, Parser)]
pub struct AdapterInitArgs {
    #[arg(long = "edition")]
    pub edition: Option<Edition>,
}

#[async_trait]
impl<'a> MoveTestAdapter<'a> for SimpleVMTestAdapter {
    type ExtraInitArgs = AdapterInitArgs;
    type ExtraPublishArgs = EmptyCommand;
    type ExtraValueArgs = ();
    type ExtraRunArgs = EmptyCommand;
    type Subcommand = EmptyCommand;

    fn compiled_state(&mut self) -> &mut CompiledState {
        &mut self.compiled_state
    }

    fn default_syntax(&self) -> SyntaxChoice {
        self.default_syntax
    }

    async fn cleanup_resources(&mut self) -> Result<()> {
        Ok(())
    }

    async fn init(
        default_syntax: SyntaxChoice,
        pre_compiled_deps: Option<Arc<FullyCompiledProgram>>,
        task_opt: Option<TaskInput<(InitCommand, Self::ExtraInitArgs)>>,
        _path: &Path,
    ) -> (Self, Option<String>) {
        println!("---- INITIALIZING -------------------------------------------------------------");
        println!("grabbing init arguments");
        let (additional_mapping, compiler_edition) = match task_opt.map(|t| t.command) {
            Some((InitCommand { named_addresses }, AdapterInitArgs { edition })) => {
                let addresses = verify_and_create_named_address_mapping(named_addresses).unwrap();
                let compiler_edition = edition.unwrap_or(Edition::LEGACY);
                (addresses, compiler_edition)
            }
            None => (BTreeMap::new(), Edition::LEGACY),
        };

        println!("generating named address map");
        let mut named_address_mapping = move_stdlib_named_addresses();
        for (name, addr) in additional_mapping {
            if named_address_mapping.contains_key(&name) {
                panic!(
                    "Invalid init. The named address '{}' is reserved by the move-stdlib",
                    name
                )
            }
            named_address_mapping.insert(name, addr);
        }
        println!("creating adapter");
        let mut adapter = Self {
            compiled_state: CompiledState::new(
                named_address_mapping,
                pre_compiled_deps,
                None,
                Some(compiler_edition),
                None,
            ),
            default_syntax,
            storage: InMemoryStorage::new(),
        };

        println!("doing initial publish");
        adapter
            .perform_action(
                None,
                |vm, storage, gas_status| {
                    let mut sender = None;
                    println!("serializing modules");
                    let modules = MOVE_STDLIB_COMPILED
                        .iter()
                        .map(|module| {
                            let mut module_bytes = vec![];
                            sender = Some(*module.self_id().address());
                            module
                                .serialize_with_version(module.version, &mut module_bytes)
                                .unwrap();
                            module_bytes
                        })
                        .collect::<Vec<_>>();

                    println!("calling publish");
                    let pub_storage: &InMemoryStorage = storage;
                    let (changeset, _storage) = vm.publish_package(
                        pub_storage,
                        sender.expect("Must have at least one module"),
                        modules,
                        gas_status,
                    );
                    storage.apply(changeset?).unwrap();
                    Ok(())
                },
                VMConfig::default(),
            )
            .unwrap();
        let mut addr_to_name_mapping = BTreeMap::new();
        for (name, addr) in move_stdlib_named_addresses() {
            let prev = addr_to_name_mapping.insert(addr, Symbol::from(name));
            assert!(prev.is_none());
        }
        for module in MOVE_STDLIB_COMPILED
            .iter()
            .filter(|module| !adapter.compiled_state.is_precompiled_dep(&module.self_id()))
            .collect::<Vec<_>>()
        {
            adapter
                .compiled_state
                .add_and_generate_interface_file(module.clone());
        }
        (adapter, None)
    }

    async fn publish_modules(
        &mut self,
        modules: Vec<MaybeNamedCompiledModule>,
        gas_budget: Option<u64>,
        _extra_args: Self::ExtraPublishArgs,
    ) -> Result<(Option<String>, Vec<MaybeNamedCompiledModule>)> {
        println!("---- PUBLISHING MODULE --------------------------------------------------------");
        println!("computing module bytes");
        let all_bytes = modules
            .iter()
            .map(|m| {
                let mut module_bytes = vec![];
                m.module
                    .serialize_with_version(m.module.version, &mut module_bytes)?;
                Ok(module_bytes)
            })
            .collect::<Result<_>>()?;

        println!("collecting ID");
        let id = modules.first().unwrap().module.self_id();
        println!("computing sender");
        let sender = *id.address();
        println!("performing publish");
        match self.perform_action(
            gas_budget,
            |vm, storage, gas_status| {
                let pub_storage: &InMemoryStorage = storage;
                println!("doing publish");
                let (changeset, _storage) =
                    vm.publish_package(&pub_storage, sender, all_bytes, gas_status);
                println!("applying changeset");
                // save changeset
                // TODO support events
                storage.apply(changeset?).unwrap();
                println!("done");
                Ok(())
            },
            VMConfig::default(),
        ) {
            Ok(()) => Ok((None, modules)),
            Err(e) => Err(anyhow!(
                "Unable to publish module '{}'. Got VMError: {}",
                id,
                format_vm_error(&e)
            )),
        }
    }

    async fn call_function(
        &mut self,
        module: &ModuleId,
        function: &IdentStr,
        type_arg_tags: Vec<TypeTag>,
        signers: Vec<ParsedAddress>,
        txn_args: Vec<MoveValue>,
        gas_budget: Option<u64>,
        _extra_args: Self::ExtraRunArgs,
    ) -> Result<(Option<String>, SerializedReturnValues)> {
        println!("---- CALLING FUNCTION ---------------------------------------------------------");
        let signers: Vec<_> = signers
            .into_iter()
            .map(|addr| self.compiled_state().resolve_address(&addr))
            .collect();

        let args = txn_args
            .iter()
            .map(|arg| arg.simple_serialize().unwrap())
            .collect::<Vec<_>>();
        // TODO rethink testing signer args
        let args = signers
            .iter()
            .map(|a| MoveValue::Signer(*a).simple_serialize().unwrap())
            .chain(args)
            .collect();
        let serialized_return_values = self
            .perform_action(
                gas_budget,
                |vm, storage, gas_status| {
                    // Set the storage's root context to the call module's address.
                    storage.context = module.address().clone();
                    println!("Current storage: {:?}", storage);
                    println!("Current vm: {:#?}", vm);
                    let instance_storage: &InMemoryStorage = storage;
                    println!("Generating VM instance");
                    let mut vm_instance = vm.make_instance(instance_storage)?;
                    println!("Creating type arguments");
                    let type_args: Vec<_> = type_arg_tags
                        .into_iter()
                        .map(|tag| vm_instance.load_type(&tag))
                        .collect::<VMResult<_>>()?;

                    println!("Doing call");
                    vm_instance.execute_function_bypass_visibility(
                        module, function, type_args, args, gas_status,
                    )
                },
                test_vm_config(),
            )
            .map_err(|e| {
                anyhow!(
                    "Function execution failed with VMError: {}",
                    format_vm_error(&e)
                )
            })?;
        Ok((None, serialized_return_values))
    }

    #[allow(clippy::diverging_sub_expression)]
    async fn handle_subcommand(
        &mut self,
        _: TaskInput<Self::Subcommand>,
    ) -> Result<Option<String>> {
        unimplemented!()
    }

    async fn process_error(&self, err: Error) -> Error {
        err
    }
}

pub fn format_vm_error(e: &VMError) -> String {
    let location_string = match e.location() {
        Location::Undefined => "undefined".to_owned(),
        Location::Module(id) => format!("0x{}::{}", id.address().short_str_lossless(), id.name()),
    };
    format!(
        "{{
    major_status: {major_status:?},
    sub_status: {sub_status:?},
    location: {location_string},
    indices: {indices:?},
    offsets: {offsets:?},
}}",
        major_status = e.major_status(),
        sub_status = e.sub_status(),
        location_string = location_string,
        // TODO maybe include source map info?
        indices = e.indices(),
        offsets = e.offsets(),
    )
}

impl SimpleVMTestAdapter {
    fn perform_action<Ret>(
        &mut self,
        gas_budget: Option<u64>,
        f: impl FnOnce(&mut VirtualMachine, &mut InMemoryStorage, &mut GasStatus) -> VMResult<Ret>,
        vm_config: VMConfig,
    ) -> VMResult<Ret> {
        println!("creating natives");
        // start session
        let natives =
            stdlib_native_functions(STD_ADDR, GasParameters::zeros(), /* silent */ false)
                .map_err(|e| e.finish(Location::Undefined))?;
        println!("creating VM");
        let mut vm = VirtualMachine::new(natives, vm_config);
        println!("creating gas_status");
        let mut gas_status = move_cli::sandbox::utils::get_gas_status(
            &move_vm_test_utils::gas_schedule::INITIAL_COST_SCHEDULE,
            gas_budget,
        )
        .unwrap();

        // perform op
        println!("performing operation");
        let res = f(&mut vm, &mut self.storage, &mut gas_status)?;

        Ok(res)
    }
}

pub static PRECOMPILED_MOVE_STDLIB: Lazy<FullyCompiledProgram> = Lazy::new(|| {
    let program_res = move_compiler::construct_pre_compiled_lib(
        vec![PackagePaths {
            name: None,
            paths: move_stdlib::move_stdlib_files(),
            named_address_map: move_stdlib::move_stdlib_named_addresses(),
        }],
        None,
        move_compiler::Flags::empty(),
        None,
    )
    .unwrap();
    match program_res {
        Ok(stdlib) => stdlib,
        Err((files, errors)) => {
            eprintln!("!!!Standard library failed to compile!!!");
            move_compiler::diagnostics::report_diagnostics(&files, errors)
        }
    }
});

static MOVE_STDLIB_COMPILED: Lazy<Vec<CompiledModule>> = Lazy::new(|| {
    let (files, units_res) = move_compiler::Compiler::from_files(
        None,
        move_stdlib::move_stdlib_files(),
        vec![],
        move_stdlib::move_stdlib_named_addresses(),
    )
    .build()
    .unwrap();
    match units_res {
        Err(diags) => {
            eprintln!("!!!Standard library failed to compile!!!");
            move_compiler::diagnostics::report_diagnostics(&files, diags)
        }
        Ok((_, warnings)) if !warnings.is_empty() => {
            eprintln!("!!!Standard library failed to compile!!!");
            move_compiler::diagnostics::report_diagnostics(&files, warnings)
        }
        Ok((units, _warnings)) => units
            .into_iter()
            .map(|annot_module| annot_module.named_module.module)
            .collect(),
    }
});

fn test_vm_config() -> VMConfig {
    VMConfig {
        enable_invariant_violation_check_in_swap_loc: false,
        ..Default::default()
    }
}

#[tokio::main]
pub async fn run_test(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    run_test_impl::<SimpleVMTestAdapter>(path, Some(Arc::new(PRECOMPILED_MOVE_STDLIB.clone())))
        .await
}
