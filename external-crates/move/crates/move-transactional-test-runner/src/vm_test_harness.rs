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
use move_command_line_common::files::verify_and_create_named_address_mapping;
use move_compiler::{editions::Edition, shared::PackagePaths, FullyCompiledProgram};
use move_core_types::parsing::address::ParsedAddress;
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
    move_vm::MoveVM,
    session::{SerializedReturnValues, Session},
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

    async fn init(
        default_syntax: SyntaxChoice,
        pre_compiled_deps: Option<Arc<FullyCompiledProgram>>,
        task_opt: Option<TaskInput<(InitCommand, Self::ExtraInitArgs)>>,
        _path: &Path,
    ) -> (Self, Option<String>) {
        let (additional_mapping, compiler_edition) = match task_opt.map(|t| t.command) {
            Some((InitCommand { named_addresses }, AdapterInitArgs { edition })) => {
                let addresses = verify_and_create_named_address_mapping(named_addresses).unwrap();
                let compiler_edition = edition.unwrap_or(Edition::LEGACY);
                (addresses, compiler_edition)
            }
            None => (BTreeMap::new(), Edition::LEGACY),
        };

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

        adapter
            .perform_session_action(
                None,
                |session, gas_status| {
                    for module in &*MOVE_STDLIB_COMPILED {
                        let mut module_bytes = vec![];
                        module
                            .serialize_with_version(module.version, &mut module_bytes)
                            .unwrap();

                        let id = module.self_id();
                        let sender = *id.address();
                        session
                            .publish_module(module_bytes, sender, gas_status)
                            .unwrap();
                    }
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
        let all_bytes = modules
            .iter()
            .map(|m| {
                let mut module_bytes = vec![];
                m.module
                    .serialize_with_version(m.module.version, &mut module_bytes)?;
                Ok(module_bytes)
            })
            .collect::<Result<_>>()?;

        let id = modules.first().unwrap().module.self_id();
        let sender = *id.address();
        match self.perform_session_action(
            gas_budget,
            |session, gas_status| session.publish_module_bundle(all_bytes, sender, gas_status),
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
            .perform_session_action(
                gas_budget,
                |session, gas_status| {
                    let type_args: Vec<_> = type_arg_tags
                        .into_iter()
                        .map(|tag| session.load_type(&tag))
                        .collect::<VMResult<_>>()?;

                    session.execute_function_bypass_visibility(
                        module, function, type_args, args, gas_status, None,
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
    fn perform_session_action<Ret>(
        &mut self,
        gas_budget: Option<u64>,
        f: impl FnOnce(&mut Session<&InMemoryStorage>, &mut GasStatus) -> VMResult<Ret>,
        vm_config: VMConfig,
    ) -> VMResult<Ret> {
        // start session
        let vm = MoveVM::new_with_config(
            move_stdlib_natives::all_natives(
                STD_ADDR,
                // TODO: come up with a suitable gas schedule
                move_stdlib_natives::GasParameters::zeros(),
                /* silent */ false,
            ),
            vm_config,
        )
        .unwrap();
        let (mut session, mut gas_status) = {
            let gas_status = move_cli::sandbox::utils::get_gas_status(
                &move_vm_test_utils::gas_schedule::INITIAL_COST_SCHEDULE,
                gas_budget,
            )
            .unwrap();
            let session = vm.new_session(&self.storage);
            (session, gas_status)
        };

        // perform op
        let res = f(&mut session, &mut gas_status)?;

        // save changeset
        // TODO support events
        let changeset = session.finish().0?;
        self.storage.apply(changeset).unwrap();
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
