// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use crate::tasks::{
    taskify, InitCommand, PrintBytecodeCommand, PublishCommand, RunCommand, SyntaxChoice,
    TaskCommand, TaskInput,
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use clap::Parser;
use move_binary_format::file_format::CompiledModule;
use move_bytecode_source_map::{mapping::SourceMapping, source_map::SourceMap};
use move_command_line_common::{
    env::read_bool_env_var,
    files::{MOVE_EXTENSION, MOVE_IR_EXTENSION},
    insta_assert,
};
use move_compiler::{
    compiled_unit::AnnotatedCompiledUnit,
    diagnostics::{warning_filters::WarningFiltersBuilder, Diagnostics},
    editions::{Edition, Flavor},
    shared::{files::MappedFiles, NumericalAddress, PackageConfig},
    FullyCompiledProgram,
};
use move_core_types::parsing::{
    address::ParsedAddress,
    types::ParsedType,
    values::{ParsableValue, ParsedValue},
};
use move_core_types::{
    account_address::AccountAddress,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, TypeTag},
};
use move_disassembler::disassembler::{Disassembler, DisassemblerOptions};
use move_ir_types::location::Spanned;
use move_symbol_pool::Symbol;
use move_vm_runtime::session::SerializedReturnValues;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt::{Debug, Write as FmtWrite},
    future::Future,
    io::Write,
    path::Path,
    sync::Arc,
};
use tempfile::NamedTempFile;

pub struct CompiledState {
    pre_compiled_deps: Option<Arc<FullyCompiledProgram>>,
    pre_compiled_ids: BTreeSet<(AccountAddress, String)>,
    compiled_module_named_address_mapping: BTreeMap<ModuleId, Symbol>,
    pub named_address_mapping: BTreeMap<String, NumericalAddress>,
    default_named_address_mapping: Option<NumericalAddress>,
    edition: Edition,
    flavor: Flavor,
    modules: BTreeMap<ModuleId, CompiledModule>,
    temp_files: BTreeMap<String, NamedTempFile>,
}

impl CompiledState {
    pub fn resolve_named_address(&self, s: &str) -> AccountAddress {
        if let Some(addr) = self
            .named_address_mapping
            .get(s)
            .or(self.default_named_address_mapping.as_ref())
        {
            return AccountAddress::new(addr.into_bytes());
        }
        panic!("Failed to resolve named address '{}'", s)
    }

    pub fn resolve_address(&self, addr: &ParsedAddress) -> AccountAddress {
        match addr {
            ParsedAddress::Named(named_addr) => self.resolve_named_address(named_addr.as_str()),
            ParsedAddress::Numerical(addr) => addr.into_inner(),
        }
    }

    pub fn resolve_args<Extra: ParsableValue>(
        &self,
        args: Vec<ParsedValue<Extra>>,
    ) -> Result<Vec<Extra::ConcreteValue>> {
        args.into_iter()
            .map(|arg| arg.into_concrete_value(&|s| Some(self.resolve_named_address(s))))
            .collect()
    }

    pub fn resolve_type_args(&self, type_args: Vec<ParsedType>) -> Result<Vec<TypeTag>> {
        type_args
            .into_iter()
            .map(|arg| arg.into_type_tag(&|s| Some(self.resolve_named_address(s))))
            .collect()
    }
}

fn merge_output(left: Option<String>, right: Option<String>) -> Option<String> {
    match (left, right) {
        (None, right) => right,
        (left, None) => left,
        (Some(mut left), Some(right)) => {
            left.push_str(&right);
            Some(left)
        }
    }
}

#[async_trait]
pub trait MoveTestAdapter<'a>: Sized + Send {
    type ExtraPublishArgs: Send + Parser + Default;
    type ExtraValueArgs: ParsableValue + Clone;
    type ExtraRunArgs: Send + Parser;
    type Subcommand: Send + Parser;
    type ExtraInitArgs: Send + Parser;

    fn compiled_state(&mut self) -> &mut CompiledState;
    fn default_syntax(&self) -> SyntaxChoice;
    async fn init(
        default_syntax: SyntaxChoice,
        option: Option<Arc<FullyCompiledProgram>>,
        init_data: Option<TaskInput<(InitCommand, Self::ExtraInitArgs)>>,
        path: &Path,
    ) -> (Self, Option<String>);

    async fn publish_modules(
        &mut self,
        modules: Vec<MaybeNamedCompiledModule>,
        gas_budget: Option<u64>,
        extra: Self::ExtraPublishArgs,
    ) -> Result<(Option<String>, Vec<MaybeNamedCompiledModule>)>;
    async fn call_function(
        &mut self,
        module: &ModuleId,
        function: &IdentStr,
        type_args: Vec<TypeTag>,
        signers: Vec<ParsedAddress>,
        args: Vec<<<Self as MoveTestAdapter<'a>>::ExtraValueArgs as ParsableValue>::ConcreteValue>,
        gas_budget: Option<u64>,
        extra: Self::ExtraRunArgs,
    ) -> Result<(Option<String>, SerializedReturnValues)>;

    async fn handle_subcommand(
        &mut self,
        subcommand: TaskInput<Self::Subcommand>,
    ) -> Result<Option<String>>;

    fn render_command_input(
        &self,
        _task: &TaskInput<
            TaskCommand<
                Self::ExtraInitArgs,
                Self::ExtraPublishArgs,
                Self::ExtraValueArgs,
                Self::ExtraRunArgs,
                Self::Subcommand,
            >,
        >,
    ) -> Option<String> {
        None
    }

    async fn process_error(&self, error: anyhow::Error) -> anyhow::Error;

    async fn handle_command(
        &mut self,
        task: TaskInput<
            TaskCommand<
                Self::ExtraInitArgs,
                Self::ExtraPublishArgs,
                Self::ExtraValueArgs,
                Self::ExtraRunArgs,
                Self::Subcommand,
            >,
        >,
    ) -> Result<Option<String>>
    where
        'a: 'async_trait,
    {
        let TaskInput {
            command,
            name,
            number,
            start_line,
            command_lines_stop,
            stop_line,
            data,
            task_text,
        } = task;
        match command {
            TaskCommand::Init { .. } => {
                panic!("The 'init' command is optional. But if used, it must be the first command")
            }
            TaskCommand::PrintBytecode(PrintBytecodeCommand { syntax }) => {
                let syntax = syntax.unwrap_or_else(|| self.default_syntax());
                let (warnings_opt, output, _data, modules) = compile_any(
                    self,
                    "publish",
                    syntax,
                    name,
                    number,
                    start_line,
                    command_lines_stop,
                    stop_line,
                    data,
                    |_adapter, modules| async { Ok((None, modules)) },
                )
                .await?;
                let output = merge_output(output, warnings_opt);
                let output = modules.into_iter().fold(output, |output, m| {
                    let MaybeNamedCompiledModule {
                        module, source_map, ..
                    } = m;
                    let source_mapping = match source_map {
                        Some(m) => SourceMapping::new(m, &module),
                        None => SourceMapping::new_without_source_map(
                            &module,
                            Spanned::unsafe_no_loc(()).loc,
                        )
                        .expect("Unable to build dummy source mapping"),
                    };
                    let disassembler =
                        Disassembler::new(source_mapping, DisassemblerOptions::new());
                    merge_output(output, Some(disassembler.disassemble().unwrap()))
                });
                Ok(output)
            }
            TaskCommand::Publish(PublishCommand { gas_budget, syntax }, extra_args) => {
                let syntax = syntax.unwrap_or_else(|| self.default_syntax());
                let (warnings_opt, output, data, modules) = compile_any(
                    self,
                    "publish",
                    syntax,
                    name,
                    number,
                    start_line,
                    command_lines_stop,
                    stop_line,
                    data,
                    |adapter, modules| adapter.publish_modules(modules, gas_budget, extra_args),
                )
                .await?;
                store_modules(self, syntax, data, modules);
                Ok(merge_output(warnings_opt, output))
            }
            TaskCommand::Run(
                RunCommand {
                    signers,
                    args,
                    type_args,
                    gas_budget,
                    syntax,
                    name: None,
                },
                extra_args,
            ) => {
                let syntax = syntax.unwrap_or_else(|| self.default_syntax());
                let empty_publish_args = <Self::ExtraPublishArgs as Default>::default();
                let (warnings_opt, output, data, modules) = compile_any(
                    self,
                    "publish",
                    syntax,
                    name,
                    number,
                    start_line,
                    command_lines_stop,
                    stop_line,
                    data,
                    |adapter, modules| {
                        adapter.publish_modules(modules, gas_budget, empty_publish_args)
                    },
                )
                .await?;
                let (module_id, name) = single_entry_function(&modules).unwrap_or_else(|err| {
                    panic!(
                        "{} on lines {}-{} for task\n{}",
                        err, start_line, command_lines_stop, task_text
                    )
                });
                let output = merge_output(warnings_opt, output);
                store_modules(self, syntax, data, modules);
                let type_args = self.compiled_state().resolve_type_args(type_args)?;
                let args = self.compiled_state().resolve_args(args)?;
                let (ret_output, return_values) = self
                    .call_function(
                        &module_id,
                        name.as_ident_str(),
                        type_args,
                        signers,
                        args,
                        gas_budget,
                        extra_args,
                    )
                    .await?;
                let output = merge_output(ret_output, output);
                let rendered_return_value = display_return_values(return_values);
                Ok(merge_output(output, rendered_return_value))
            }
            TaskCommand::Run(
                RunCommand {
                    signers,
                    args,
                    type_args,
                    gas_budget,
                    syntax,
                    name: Some((raw_addr, module_name, name)),
                },
                extra_args,
            ) => {
                assert!(
                    syntax.is_none(),
                    "syntax flag meaningless with function execution"
                );
                let addr = self.compiled_state().resolve_address(&raw_addr);
                let module_id = ModuleId::new(addr, module_name);
                let type_args = self.compiled_state().resolve_type_args(type_args)?;
                let args = self.compiled_state().resolve_args(args)?;
                let (output, return_values) = self
                    .call_function(
                        &module_id,
                        name.as_ident_str(),
                        type_args,
                        signers,
                        args,
                        gas_budget,
                        extra_args,
                    )
                    .await?;
                let rendered_return_value = display_return_values(return_values);
                Ok(merge_output(output, rendered_return_value))
            }
            TaskCommand::Subcommand(c) => {
                self.handle_subcommand(TaskInput {
                    command: c,
                    name,
                    number,
                    start_line,
                    command_lines_stop,
                    stop_line,
                    data,
                    task_text,
                })
                .await
            }
        }
    }
}

fn single_entry_function(
    modules: &[MaybeNamedCompiledModule],
) -> anyhow::Result<(ModuleId, Identifier)> {
    anyhow::ensure!(modules.len() == 1, "Expected exactly one module");
    let module = &modules[0].module;
    let entry_funs: Vec<_> = module
        .function_defs()
        .iter()
        .filter(|def| def.is_entry)
        .collect();
    let function = if entry_funs.len() == 1 {
        entry_funs[0]
    } else if module.function_defs.len() == 1 {
        module.function_def_at(move_binary_format::file_format::FunctionDefinitionIndex::new(0))
    } else {
        anyhow::bail!("Expected exactly one function or one entry function");
    };
    let function_handle = module.function_handle_at(function.function);
    let name = module.identifier_at(function_handle.name).to_owned();
    Ok((module.self_id(), name))
}

fn display_return_values(return_values: SerializedReturnValues) -> Option<String> {
    let SerializedReturnValues {
        mutable_reference_outputs,
        return_values,
    } = return_values;
    let mut output = vec![];
    if !mutable_reference_outputs.is_empty() {
        let values = mutable_reference_outputs
            .iter()
            .map(|(idx, bytes, layout)| {
                let value =
                    move_vm_types::values::Value::simple_deserialize(bytes, layout).unwrap();
                (idx, value)
            })
            .collect::<Vec<_>>();
        let printed = values
            .iter()
            .map(|(idx, v)| {
                let mut buf = String::new();
                move_vm_types::values::debug::print_value(&mut buf, v).unwrap();
                format!("local#{}: {}", idx, buf)
            })
            .collect::<Vec<_>>()
            .join(", ");
        output.push(format!("mutable inputs after call: {}", printed))
    };
    if !return_values.is_empty() {
        let values = return_values
            .iter()
            .map(|(bytes, layout)| {
                move_vm_types::values::Value::simple_deserialize(bytes, layout).unwrap()
            })
            .collect::<Vec<_>>();
        let printed = values
            .iter()
            .map(|v| {
                let mut buf = String::new();
                move_vm_types::values::debug::print_value(&mut buf, v).unwrap();
                buf
            })
            .collect::<Vec<_>>()
            .join(", ");
        output.push(format!("return values: {}", printed))
    };
    if output.is_empty() {
        None
    } else {
        Some(output.join("\n"))
    }
}

impl CompiledState {
    pub fn new(
        named_address_mapping: BTreeMap<String, NumericalAddress>,
        pre_compiled_deps: Option<Arc<FullyCompiledProgram>>,
        default_named_address_mapping: Option<NumericalAddress>,
        compiler_edition: Option<Edition>,
        flavor: Option<Flavor>,
    ) -> Self {
        let pre_compiled_ids = match pre_compiled_deps.clone() {
            None => BTreeSet::new(),
            Some(pre_compiled) => pre_compiled
                .cfgir
                .modules
                .key_cloned_iter()
                .map(|(ident, _)| {
                    (
                        ident.value.address.into_addr_bytes().into_inner(),
                        ident.value.module.to_string(),
                    )
                })
                .collect(),
        };
        let mut state = Self {
            pre_compiled_deps: pre_compiled_deps.clone(),
            pre_compiled_ids,
            modules: BTreeMap::new(),
            compiled_module_named_address_mapping: BTreeMap::new(),
            named_address_mapping,
            edition: compiler_edition.unwrap_or(Edition::LEGACY),
            flavor: flavor.unwrap_or(Flavor::Core),
            default_named_address_mapping,
            temp_files: BTreeMap::new(),
        };
        if let Some(pcd) = pre_compiled_deps {
            for unit in &pcd.compiled {
                let (named_addr_opt, _id) = unit.module_id();
                state.add_precompiled(
                    named_addr_opt.map(|n| n.value),
                    unit.named_module.module.clone(),
                );
            }
        }
        state
    }

    pub fn dep_modules(&self) -> impl Iterator<Item = &CompiledModule> {
        self.modules.values()
    }

    pub fn source_files(&self) -> impl Iterator<Item = &String> {
        self.temp_files.keys()
    }

    pub fn add_with_source_file(
        &mut self,
        modules: Vec<MaybeNamedCompiledModule>,
        (path, tempfile): (String, NamedTempFile),
    ) {
        let prev = self.temp_files.insert(path, tempfile);
        assert!(prev.is_none());
        for m in modules {
            let MaybeNamedCompiledModule {
                named_address: named_addr_opt,
                module,
                ..
            } = m;
            let id = module.self_id();
            self.check_not_precompiled(&id);
            if let Some(named_addr) = named_addr_opt {
                self.compiled_module_named_address_mapping
                    .insert(id.clone(), named_addr);
            }
            self.modules.insert(id, module);
        }
    }

    pub fn add_and_generate_interface_file(&mut self, module: CompiledModule) {
        let id = module.self_id();
        self.check_not_precompiled(&id);
        let interface_file = NamedTempFile::new().unwrap();
        let path = interface_file.path().to_str().unwrap().to_owned();
        let (_id, interface_text) = move_compiler::interface_generator::write_module_to_string(
            &self.compiled_module_named_address_mapping,
            &module,
        )
        .unwrap();
        interface_file
            .reopen()
            .unwrap()
            .write_all(interface_text.as_bytes())
            .unwrap();
        let prev = self.temp_files.insert(path, interface_file);
        assert!(prev.is_none());
        self.modules.insert(id, module);
    }

    fn add_precompiled(&mut self, named_addr_opt: Option<Symbol>, module: CompiledModule) {
        let id = module.self_id();
        if let Some(named_addr) = named_addr_opt {
            self.compiled_module_named_address_mapping
                .insert(id.clone(), named_addr);
        }
        self.modules.insert(id, module);
    }

    pub fn is_precompiled_dep(&self, id: &ModuleId) -> bool {
        let addr = *id.address();
        let name = id.name().to_string();
        self.pre_compiled_ids.contains(&(addr, name))
    }

    fn check_not_precompiled(&self, id: &ModuleId) {
        assert!(
            !self.is_precompiled_dep(id),
            "Error publishing module: '{}'. \
             Re-publishing modules in pre-compiled lib is not yet supported",
            id
        )
    }
}

pub struct MaybeNamedCompiledModule {
    pub named_address: Option<Symbol>,
    pub module: CompiledModule,
    pub source_map: Option<SourceMap>,
}

pub async fn compile_any<'state, 'adapter: 'result, 'result, F, A, R>(
    test_adapter: &'adapter mut A,
    command: &str,
    syntax: SyntaxChoice,
    _name: String,
    _number: usize,
    start_line: usize,
    command_lines_stop: usize,
    _stop_line: usize,
    data: Option<NamedTempFile>,
    handler: F,
) -> Result<(
    Option<String>,
    Option<String>,
    NamedTempFile,
    Vec<MaybeNamedCompiledModule>,
)>
where
    A: MoveTestAdapter<'state> + 'adapter,
    F: FnOnce(&'adapter mut A, Vec<MaybeNamedCompiledModule>) -> R,
    R: Future<Output = Result<(Option<String>, Vec<MaybeNamedCompiledModule>)>> + 'result,
{
    let data = match data {
        Some(f) => f,
        None => panic!(
            "Expected a module text block following '{command}' starting on lines {}-{}",
            start_line, command_lines_stop
        ),
    };
    let state = test_adapter.compiled_state();
    let (modules, warnings_opt) = match syntax {
        SyntaxChoice::Source => {
            let (units, warnings_opt) = compile_source_units(state, data.path())?;
            let modules = units
                .into_iter()
                .map(|unit| {
                    let (named_addr_opt, _id) = unit.module_id();
                    let named_addr_opt = named_addr_opt.map(|n| n.value);
                    let module = unit.named_module.module;
                    let source_map = Some(unit.named_module.source_map);
                    MaybeNamedCompiledModule {
                        named_address: named_addr_opt,
                        module,
                        source_map,
                    }
                })
                .collect();
            (modules, warnings_opt)
        }
        SyntaxChoice::IR => {
            let module = compile_ir_module(state, data.path())?;
            (
                vec![MaybeNamedCompiledModule {
                    named_address: None,
                    module,
                    source_map: None,
                }],
                None,
            )
        }
    };
    let (output, modules) = handler(test_adapter, modules).await?;
    Ok((warnings_opt, output, data, modules))
}

pub fn store_modules<'a, A: MoveTestAdapter<'a>>(
    test_adapter: &mut A,
    syntax: SyntaxChoice,
    data: NamedTempFile,
    mut modules: Vec<MaybeNamedCompiledModule>,
) {
    match syntax {
        SyntaxChoice::Source => {
            let path = data.path().to_str().unwrap().to_owned();
            test_adapter
                .compiled_state()
                .add_with_source_file(modules, (path, data))
        }
        SyntaxChoice::IR => {
            let module = modules.pop().unwrap().module;
            test_adapter
                .compiled_state()
                .add_and_generate_interface_file(module);
        }
    }
}

pub fn compile_source_units(
    state: &CompiledState,
    file_name: impl AsRef<Path>,
) -> Result<(Vec<AnnotatedCompiledUnit>, Option<String>)> {
    fn rendered_diags(files: &MappedFiles, diags: Diagnostics) -> Option<String> {
        if diags.is_empty() {
            return None;
        }

        let ansi_color = read_bool_env_var(move_command_line_common::testing::PRETTY);
        let error_buffer =
            move_compiler::diagnostics::report_diagnostics_to_buffer_with_mapped_files(
                files, diags, ansi_color,
            );
        Some(String::from_utf8(error_buffer).unwrap())
    }

    use move_compiler::PASS_COMPILATION;
    let named_address_mapping = state.named_address_mapping.clone();
    // txn testing framework test code includes private unused functions and unused struct types on
    // purpose and generating warnings for all of them does not make much sense (and there would be
    // a lot of them!) so let's suppress them function warnings, so let's suppress these
    let warning_filter = WarningFiltersBuilder::unused_warnings_filter_for_test();
    let (mut files, compiler_res) = move_compiler::Compiler::from_files(
        None,
        vec![file_name.as_ref().to_str().unwrap().to_owned()],
        state.source_files().cloned().collect::<Vec<_>>(),
        named_address_mapping,
    )
    .set_pre_compiled_lib_opt(state.pre_compiled_deps.clone())
    .set_flags(move_compiler::Flags::empty().set_sources_shadow_deps(true))
    .set_warning_filter(Some(warning_filter))
    .set_default_config(PackageConfig {
        edition: state.edition,
        flavor: state.flavor,
        ..PackageConfig::default()
    })
    .run::<PASS_COMPILATION>()?;
    let units_or_diags = compiler_res.map(|move_compiler| move_compiler.into_compiled_units());

    match units_or_diags {
        Err((_pass, diags)) => {
            if let Some(pcd) = state.pre_compiled_deps.clone() {
                files.extend(pcd.files.clone());
            }
            Err(anyhow!(rendered_diags(&files, diags).unwrap()))
        }
        Ok((units, warnings)) => Ok((units, rendered_diags(&files, warnings))),
    }
}

pub fn compile_ir_module(
    state: &CompiledState,
    file_name: impl AsRef<Path>,
) -> Result<CompiledModule> {
    use move_ir_compiler::Compiler as IRCompiler;
    let code = std::fs::read_to_string(file_name).unwrap();
    let named_addresses = state
        .named_address_mapping
        .iter()
        .map(|(name, addr)| (name.clone(), addr.into_inner()))
        .collect();
    IRCompiler::new(state.dep_modules().collect())
        .with_named_addresses(named_addresses)
        .into_compiled_module(&code)
}

/// Creates an adapter for the given tasks, using the first task command to initialize the adapter
/// if it is a `TaskCommand::Init`. Returns the adapter and the output string.
pub async fn create_adapter<'a, Adapter>(
    path: &Path,
    fully_compiled_program_opt: Option<Arc<FullyCompiledProgram>>,
) -> Result<(String, Adapter), Box<dyn std::error::Error>>
where
    Adapter: MoveTestAdapter<'a>,
    Adapter::ExtraInitArgs: Debug,
    Adapter::ExtraPublishArgs: Debug,
    Adapter::ExtraValueArgs: Debug,
    Adapter::ExtraRunArgs: Debug,
    Adapter::Subcommand: Debug,
{
    let extension = path.extension().unwrap().to_str().unwrap();
    let default_syntax = if extension == MOVE_IR_EXTENSION {
        SyntaxChoice::IR
    } else {
        assert!(extension == MOVE_EXTENSION);
        SyntaxChoice::Source
    };
    let mut output = String::new();
    let mut tasks = taskify::<
        TaskCommand<
            Adapter::ExtraInitArgs,
            Adapter::ExtraPublishArgs,
            Adapter::ExtraValueArgs,
            Adapter::ExtraRunArgs,
            Adapter::Subcommand,
        >,
    >(path)?
    .into_iter()
    .collect::<VecDeque<_>>();
    assert!(!tasks.is_empty());
    let num_tasks = tasks.len();
    writeln!(
        &mut output,
        "processed {} task{}",
        num_tasks,
        if num_tasks > 1 { "s" } else { "" }
    )
    .unwrap();

    let first_task = tasks.pop_front().unwrap();
    let init_opt = match &first_task.command {
        TaskCommand::Init(_, _) => Some(first_task.map(|known| match known {
            TaskCommand::Init(command, extra_args) => (command, extra_args),
            _ => unreachable!(),
        })),
        _ => {
            tasks.push_front(first_task);
            None
        }
    };

    let (adapter, result_opt) =
        Adapter::init(default_syntax, fully_compiled_program_opt, init_opt, path).await;

    if let Some(result) = result_opt {
        if let Err(e) = writeln!(output, "\ninit:\n{}", result) {
            return Err(Box::new(e));
        }
    }

    Ok((output, adapter))
}

/// Consumes the adapter to run tasks from path.
pub async fn run_tasks_with_adapter<'a, Adapter>(
    path: &Path,
    mut adapter: Adapter,
    mut output: String,
) -> Result<()>
where
    Adapter: MoveTestAdapter<'a>,
    Adapter::ExtraInitArgs: Debug,
    Adapter::ExtraPublishArgs: Debug,
    Adapter::ExtraValueArgs: Debug,
    Adapter::ExtraRunArgs: Debug,
    Adapter::Subcommand: Debug,
{
    let mut tasks = taskify::<
        TaskCommand<
            Adapter::ExtraInitArgs,
            Adapter::ExtraPublishArgs,
            Adapter::ExtraValueArgs,
            Adapter::ExtraRunArgs,
            Adapter::Subcommand,
        >,
    >(path)?
    .into_iter()
    .collect::<VecDeque<_>>();
    assert!(!tasks.is_empty());

    // Pop off init command if present, this has already been handled before this function was
    // called to initialize the adapter
    if let Some(TaskCommand::Init(_, _)) = tasks.front().map(|t| &t.command) {
        tasks.pop_front();
    }

    for task in tasks {
        handle_known_task(&mut output, &mut adapter, task).await;
    }

    insta_assert! {
        input_path: path,
        contents: output,
    }
    Ok(())
}

/// Convenience function that creates an adapter and runs the tasks, to be used when a caller does
/// not need to extend the adapter.
pub async fn run_test_impl<'a, Adapter>(
    path: &Path,
    fully_compiled_program_opt: Option<Arc<FullyCompiledProgram>>,
) -> Result<(), Box<dyn std::error::Error>>
where
    Adapter: MoveTestAdapter<'a>,
    Adapter::ExtraInitArgs: Debug,
    Adapter::ExtraPublishArgs: Debug,
    Adapter::ExtraValueArgs: Debug,
    Adapter::ExtraRunArgs: Debug,
    Adapter::Subcommand: Debug,
{
    let (output, adapter) = create_adapter::<Adapter>(path, fully_compiled_program_opt).await?;
    run_tasks_with_adapter(path, adapter, output).await?;
    Ok(())
}

async fn handle_known_task<'a, Adapter: MoveTestAdapter<'a>>(
    output: &mut String,
    adapter: &mut Adapter,
    task: TaskInput<
        TaskCommand<
            Adapter::ExtraInitArgs,
            Adapter::ExtraPublishArgs,
            Adapter::ExtraValueArgs,
            Adapter::ExtraRunArgs,
            Adapter::Subcommand,
        >,
    >,
) {
    let task_number = task.number;
    let start_line = task.start_line;
    let stop_line = task.stop_line;
    let task_text = adapter
        .render_command_input(&task)
        .unwrap_or_else(|| task.task_text.clone());
    let result = adapter.handle_command(task).await;
    let result_string = match result {
        Ok(None) => return,
        Ok(Some(s)) => s,
        Err(e) => format!("Error: {}", adapter.process_error(e).await),
    };
    assert!(!result_string.is_empty());

    let line_number = if start_line == stop_line {
        format!("line {}", start_line)
    } else {
        format!("lines {}-{}", start_line, stop_line)
    };

    writeln!(
        output,
        "\ntask {task_number}, {line_number}:\n{task_text}\n{result_string}"
    )
    .unwrap();
}
