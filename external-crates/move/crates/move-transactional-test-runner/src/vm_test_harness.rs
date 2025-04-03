// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

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
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, TypeTag},
    runtime_value::MoveValue,
};
use move_stdlib::move_stdlib_named_addresses;
use move_symbol_pool::Symbol;
use move_vm_config::runtime::VMConfig;
use move_vm_runtime::{
    dev_utils::{
        gas_schedule::{self, GasStatus},
        in_memory_test_adapter::InMemoryTestAdapter,
        storage::{InMemoryStorage, StoredPackage},
        vm_test_adapter::VMTestAdapter,
    },
    execution::vm::MoveVM,
    natives::move_stdlib::{stdlib_native_functions, GasParameters},
    runtime::MoveRuntime,
    shared::{
        gas::GasMeter, linkage_context::LinkageContext, serialization::SerializedReturnValues,
    },
};
use once_cell::sync::Lazy;

use std::{collections::BTreeMap, path::Path, sync::Arc};

const STD_ADDR: AccountAddress = AccountAddress::ONE;

struct SimpleRuntimeTestAdapter {
    compiled_state: CompiledState,
    default_syntax: SyntaxChoice,
    // NB: We can reuse the in-memory test adapter from the vm runtime tests
    adapter: InMemoryTestAdapter,
    // storage: InMemoryStorage,
    // vm: VirtualMachine,
}

#[derive(Debug, Parser)]
pub struct AdapterInitArgs {
    #[arg(long = "edition")]
    pub edition: Option<Edition>,
}

#[derive(Debug, Parser, Default)]
pub struct PublishLinkageArgs {
    #[arg(long = "location")]
    pub location: Option<AccountAddress>,
    #[clap(flatten)]
    pub linkage: Linkage,
}

#[derive(Debug, Clone, Parser, Default)]
pub struct Linkage {
    #[arg(long = "linkage", value_parser = parse_linkage, num_args(1..))]
    pub linkage: Vec<(AccountAddress, AccountAddress)>,
    #[arg(long = "type-origin", value_parser = parse_type_origin, num_args(1..))]
    pub type_origin: Vec<((AccountAddress, Identifier, Identifier), AccountAddress)>,
}

impl Linkage {
    pub fn overlay(&self, mut existing_linkage: LinkageContext) -> VMResult<LinkageContext> {
        for (original_id, version_id) in self.linkage.iter() {
            existing_linkage
                .add_entry(*original_id, *version_id)
                .map_err(|err| err.finish(Location::Undefined))?;
        }
        Ok(existing_linkage)
    }
}

impl PublishLinkageArgs {
    pub fn overlay(&self, existing_linkage: LinkageContext) -> VMResult<LinkageContext> {
        self.linkage.overlay(existing_linkage)
    }

    pub fn resolve_publication_location(
        &self,
        sender: AccountAddress,
        linkage: &LinkageContext,
    ) -> AccountAddress {
        // 1. Use location if specified; otherwise
        // 2. Use the sender address (remapped if it exists in the linkage table); otherwise
        // 3. Use the sender address
        self.location.unwrap_or(
            linkage
                .linkage_table
                .get(&sender)
                .copied()
                .unwrap_or(sender),
        )
    }
}

fn parse_linkage(s: &str) -> Result<(AccountAddress, AccountAddress)> {
    let parts: Vec<_> = s.split("=>").collect::<Vec<_>>();
    if parts.len() != 2 {
        return Err(anyhow!("Invalid linkage format. Expected 'addr1=>addr2'"));
    }
    let addr1 = AccountAddress::from_hex_literal(parts[0])?;
    let addr2 = AccountAddress::from_hex_literal(parts[1])?;
    Ok((addr1, addr2))
}

fn parse_type_origin(
    s: &str,
) -> Result<((AccountAddress, Identifier, Identifier), AccountAddress)> {
    let parts: Vec<_> = s.split("=>").collect::<Vec<_>>();
    if parts.len() != 2 {
        return Err(anyhow!(
            "Invalid type origin format. Expected 'addr1::name::name=>addr2'"
        ));
    }
    let (addr1, mname, tname) = {
        let tparts: Vec<_> = parts[0].split("::").collect::<Vec<_>>();
        if tparts.len() != 3 {
            return Err(anyhow!(
                "Invalid type origin format. Expected 'addr1::name::name=>addr2'"
            ));
        }
        (
            AccountAddress::from_hex_literal(tparts[0])?,
            Identifier::new(tparts[1])?,
            Identifier::new(tparts[2])?,
        )
    };
    let addr2 = AccountAddress::from_hex_literal(parts[1])?;
    Ok(((addr1, mname, tname), addr2))
}

#[async_trait]
impl MoveTestAdapter<'_> for SimpleRuntimeTestAdapter {
    type ExtraInitArgs = AdapterInitArgs;
    type ExtraPublishArgs = PublishLinkageArgs;
    type ExtraValueArgs = ();
    type ExtraRunArgs = Linkage;
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
        println!("creating VM");
        let native_functions =
            stdlib_native_functions(STD_ADDR, GasParameters::zeros(), /* silent */ false)
                .map_err(|e| e.finish(Location::Undefined))
                .expect("Failed to initialize natives");
        let vm_config = test_vm_config();
        let runtime = MoveRuntime::new(native_functions, vm_config);
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
            adapter: InMemoryTestAdapter::new_with_runtime(runtime),
        };

        println!("doing initial publish");
        adapter
            .perform_action(None, |inner_adapter, _gas_status| {
                let move_stdlib = MOVE_STDLIB_COMPILED.to_vec();
                let sender = *move_stdlib.first().unwrap().self_id().address();
                println!("generating stdlib linkage");
                let linkage_context = LinkageContext::new(BTreeMap::from([(sender, sender)]));
                println!("calling stdlib publish with address {sender:?}");
                let pkg = StoredPackage::from_module_for_testing_with_linkage(
                    sender,
                    linkage_context,
                    move_stdlib,
                )
                .unwrap();
                let pkg = pkg.into_serialized_package();
                inner_adapter.publish_package(sender, pkg).unwrap();
                Ok(())
            })
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
        extra_args: Self::ExtraPublishArgs,
    ) -> Result<(Option<String>, Vec<MaybeNamedCompiledModule>)> {
        println!("---- PUBLISHING MODULE --------------------------------------------------------");
        let pub_modules = modules
            .iter()
            .map(|module| module.module.clone())
            .collect::<Vec<_>>();
        println!("collecting ID");
        let id = pub_modules.first().unwrap().self_id();
        println!("computing sender");
        let sender = *id.address();
        println!("performing publish for {sender}");
        println!("generating linkage");
        let linkage_context = extra_args.overlay(self.adapter.generate_linkage_context(
            sender,
            sender,
            &pub_modules,
        )?)?;
        let storage_id = extra_args.resolve_publication_location(sender, &linkage_context);
        println!("linkage: {linkage_context:#?}");
        println!("publication location: {storage_id}");
        println!("doing publish");
        let pkg = StoredPackage::from_module_for_testing_with_linkage(
            storage_id,
            linkage_context,
            pub_modules.clone(),
        )?;
        let original_id = pkg.original_id;
        let publish_result = self.perform_action(gas_budget, |inner_adapter, _gas_status| {
            let pkg = pkg.into_serialized_package();
            inner_adapter.publish_package(original_id, pkg)?;
            println!("done");
            Ok(())
        });
        match publish_result {
            Ok(()) => Ok((None, modules)),
            Err(e) => Err(anyhow!(
                "Unable to publish module '{}'. Got VMError: {}",
                id,
                format_vm_error(&e)
            )),
        }
    }

    async fn publish_modules_with_calls(
        &mut self,
        modules: Vec<MaybeNamedCompiledModule>,
        calls: Vec<(ModuleId, Identifier, Vec<MoveValue>)>,
        signers: Vec<ParsedAddress>,
        gas_budget: Option<u64>,
        extra_args: Self::ExtraPublishArgs,
    ) -> Result<(
        Option<String>,
        Vec<MaybeNamedCompiledModule>,
        Vec<SerializedReturnValues>,
    )> {
        println!("---- PUBLISHING MODULE WITH CALLS ---------------------------------------------");
        let pub_modules = modules
            .iter()
            .map(|module| module.module.clone())
            .collect::<Vec<_>>();
        println!("collecting ID");
        let id = pub_modules.first().unwrap().self_id();
        println!("computing sender");
        let sender = *id.address();
        println!("performing publish for {sender}");
        println!("generating linkage");
        let linkage_context = extra_args.overlay(self.adapter.generate_linkage_context(
            sender,
            sender,
            &pub_modules,
        )?)?;
        let storage_id = extra_args.resolve_publication_location(sender, &linkage_context);
        println!("linkage: {linkage_context:#?}");
        println!("publication location: {storage_id}");
        let mut gas_meter = Self::make_gas_status(gas_budget);
        println!("doing verification");
        let pkg = StoredPackage::from_module_for_testing_with_linkage(
            storage_id,
            linkage_context,
            pub_modules.clone(),
        )?;
        let original_id = pkg.original_id;
        let pkg = pkg.into_serialized_package();
        let (verif_pkg, mut publish_vm) = self
            .perform_action_with_gas(&mut gas_meter, |inner_adapter, _gas_status| {
                inner_adapter.verify_package(original_id, pkg)
            })?;
        println!("doing calls");
        let signers: Vec<_> = signers
            .into_iter()
            .map(|addr| self.compiled_state().resolve_address(&addr))
            .collect();
        let call_results = calls
            .into_iter()
            .map(|(module, function, txn_args)| {
                println!("calling {module}::{function}");
                call_vm_function(
                    &mut publish_vm,
                    &module,
                    &function,
                    vec![],
                    signers.clone(),
                    txn_args,
                    &mut gas_meter,
                )
                .map_err(|e| {
                    anyhow!(
                        "Function execution failed with VMError: {}",
                        format_vm_error(&e)
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?;
        println!("doing publish");
        let publish_result = self
            .perform_action_with_gas(&mut gas_meter, |inner_adapter, _gas_status| {
                inner_adapter.publish_verified_package(sender, verif_pkg)
            });
        match publish_result {
            Ok(()) => Ok((None, modules, call_results)),
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
        extra_args: Self::ExtraRunArgs,
    ) -> Result<(Option<String>, SerializedReturnValues)> {
        println!("---- CALLING FUNCTION ---------------------------------------------------------");
        let signers: Vec<_> = signers
            .into_iter()
            .map(|addr| self.compiled_state().resolve_address(&addr))
            .collect();
        let serialized_return_values = self
            .perform_action(gas_budget, |inner_adapter, gas_status| {
                let original_id = *module.address();
                // TODO: If there are linkage directives, respect them here.
                println!("generating linkage");
                let mut linkage =
                    extra_args.overlay(inner_adapter.get_linkage_context(original_id)?)?;
                linkage.add_type_arg_addresses_reflexive(&type_arg_tags);

                println!("generating vm instance");
                let mut vm_instance = inner_adapter.make_vm(linkage)?;
                call_vm_function(
                    &mut vm_instance,
                    module,
                    function,
                    type_arg_tags,
                    signers,
                    txn_args,
                    gas_status,
                )
            })
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
    let message = if let Some(msg) = e.message() {
        format!(
            "
    message: {:?},",
            msg
        )
    } else {
        "".to_string()
    };
    format!(
        "{{
    major_status: {major_status:?},
    sub_status: {sub_status:?},
    location: {location_string},
    indices: {indices:?},
    offsets: {offsets:?},{message}
}}",
        major_status = e.major_status(),
        sub_status = e.sub_status(),
        location_string = location_string,
        // TODO maybe include source map info?
        indices = e.indices(),
        offsets = e.offsets(),
        message = message,
    )
}

impl SimpleRuntimeTestAdapter {
    fn perform_action<Ret>(
        &mut self,
        gas_budget: Option<u64>,
        f: impl FnOnce(&mut dyn VMTestAdapter<InMemoryStorage>, &mut GasStatus) -> VMResult<Ret>,
    ) -> VMResult<Ret> {
        let mut gas_status = Self::make_gas_status(gas_budget);
        self.perform_action_with_gas(&mut gas_status, f)
    }

    fn make_gas_status<'gas>(gas_budget: Option<u64>) -> GasStatus<'gas> {
        println!("creating gas_status");
        move_cli::sandbox::utils::get_gas_status(&gas_schedule::INITIAL_COST_SCHEDULE, gas_budget)
            .unwrap()
    }

    fn perform_action_with_gas<Ret>(
        &mut self,
        gas_status: &mut GasStatus,
        f: impl FnOnce(&mut dyn VMTestAdapter<InMemoryStorage>, &mut GasStatus) -> VMResult<Ret>,
    ) -> VMResult<Ret> {
        // perform op
        println!("performing operation");
        let res = f(&mut self.adapter, gas_status)?;
        Ok(res)
    }
}

fn call_vm_function(
    vm_instance: &mut MoveVM<'_>,
    module: &ModuleId,
    function: &IdentStr,
    type_arg_tags: Vec<TypeTag>,
    signers: Vec<AccountAddress>,
    txn_args: Vec<MoveValue>,
    gas_meter: &mut impl GasMeter,
) -> VMResult<SerializedReturnValues> {
    println!("Creating type arguments");
    let type_args: Vec<_> = type_arg_tags
        .into_iter()
        .map(|tag| vm_instance.load_type(&tag))
        .collect::<VMResult<_>>()?;

    println!("Creaing args");
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

    println!("Doing call");
    let result = vm_instance
        .execute_function_bypass_visibility(module, function, type_args, args, gas_meter, None);
    println!("Done calling");
    result
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
    run_test_impl::<SimpleRuntimeTestAdapter>(path, Some(Arc::new(PRECOMPILED_MOVE_STDLIB.clone())))
        .await
}
