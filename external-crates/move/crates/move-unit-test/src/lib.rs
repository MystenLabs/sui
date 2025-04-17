// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod cargo_runner;
pub mod extensions;
pub mod test_reporter;
pub mod test_runner;

use crate::test_runner::TestRunner;
use anyhow::{bail, Result};
use clap::*;
use move_binary_format::CompiledModule;
use move_command_line_common::files::verify_and_create_named_address_mapping;
use move_compiler::{
    self,
    compiled_unit::NamedCompiledModule,
    diagnostics,
    shared::{self, NumericalAddress},
    unit_test::{self, TestPlan},
    Compiler, Flags, PASS_CFGIR,
};
use move_core_types::language_storage::ModuleId;
use move_vm_runtime::native_functions::NativeFunctionTable;
use move_vm_test_utils::gas_schedule::CostTable;
use std::{collections::BTreeMap, io::Write, marker::Send, sync::Mutex};

/// The default value bounding the amount of gas consumed in a test.
const DEFAULT_EXECUTION_BOUND: u64 = 1_000_000;

/// The default number of iterations to run each random test for.
const DEFAULT_RAND_ITERS: u64 = 10;

const RAND_NUM_ITERS_FLAG: &str = "rand-num-iters";
const SEED_FLAG: &str = "seed";
const TRACE_FLAG: &str = "trace-execution";

#[derive(Debug, Parser, Clone)]
#[clap(author, version, about)]
pub struct UnitTestingConfig {
    /// Bound the gas limit for any one test. If using custom gas table, this is the max number of instructions.
    #[clap(name = "gas-limit", short = 'i', long = "gas-limit")]
    pub gas_limit: Option<u64>,

    /// A filter string to determine which unit tests to run
    #[clap(name = "filter", short = 'f', long = "filter")]
    pub filter: Option<String>,

    /// List all tests
    #[clap(name = "list", short = 'l', long = "list")]
    pub list: bool,

    /// Number of threads to use for running tests.
    #[clap(
        name = "num-threads",
        default_value = "8",
        short = 't',
        long = "threads"
    )]
    pub num_threads: usize,

    /// Dependency files
    #[clap(
        name = "dependencies",
        long = "dependencies",
        short = 'd',
        num_args(1..),
        action = clap::ArgAction::Append,
    )]
    pub dep_files: Vec<String>,

    /// Bytecode dependency files
    #[clap(
        name = "bytecode-depencencies",
        long = "bytecode-dependencies",
        num_args(1..),
        action = clap::ArgAction::Append,
    )]
    pub bytecode_deps_files: Vec<String>,

    /// Report test statistics at the end of testing. CSV report generated if 'csv' passed
    #[clap(name = "report-statistics", short = 's', long = "statistics")]
    pub report_statistics: Option<Option<String>>,

    #[clap(
        name = "report_stacktrace_on_abort",
        short = 'r',
        long = "stacktrace_on_abort"
    )]
    pub report_stacktrace_on_abort: bool,

    /// Named address mapping
    #[clap(
        name = "NAMED_ADDRESSES",
        short = 'a',
        long = "addresses",
        value_parser = shared::parse_named_address,
    )]
    pub named_address_values: Vec<(String, NumericalAddress)>,

    /// Source files
    #[clap(
        name = "sources",
        num_args(1..),
        action = clap::ArgAction::Append,
    )]
    pub source_files: Vec<String>,

    /// Verbose mode
    #[clap(short = 'v', long = "verbose")]
    pub verbose: bool,

    /// Number of iterations to run each test if arguments are being generated
    #[clap(long = RAND_NUM_ITERS_FLAG)]
    pub rand_num_iters: Option<u64>,

    /// Seed to use for generating arguments
    #[clap(long = SEED_FLAG)]
    pub seed: Option<u64>,

    // Deterministically generate the same arguments for #[random_test]s between test runs.
    // WARNING: You should only use this flag for debugging and meta-testing purposes!
    #[clap(skip)]
    pub deterministic_generation: bool,

    // Enable tracing for tests
    #[clap(long = TRACE_FLAG, value_name = "PATH")]
    pub trace_execution: Option<Option<String>>,
}

fn format_module_id(
    module_map: &BTreeMap<ModuleId, NamedCompiledModule>,
    module_id: &ModuleId,
) -> String {
    if let Some(address_name) = module_map.get(module_id).and_then(|m| m.address_name()) {
        format!("{}::{}", address_name, module_id.name())
    } else {
        module_id.short_str_lossless()
    }
}

impl UnitTestingConfig {
    /// Create a unit testing config for use with `register_move_unit_tests`
    pub fn default_with_bound(bound: Option<u64>) -> Self {
        Self {
            gas_limit: bound.or(Some(DEFAULT_EXECUTION_BOUND)),
            filter: None,
            num_threads: 8,
            report_statistics: None,
            report_stacktrace_on_abort: false,
            source_files: vec![],
            dep_files: vec![],
            bytecode_deps_files: vec![],
            verbose: false,
            list: false,
            named_address_values: vec![],
            rand_num_iters: Some(DEFAULT_RAND_ITERS),
            seed: None,
            deterministic_generation: false,
            trace_execution: None,
        }
    }

    pub fn with_named_addresses(
        mut self,
        named_address_values: BTreeMap<String, NumericalAddress>,
    ) -> Self {
        assert!(self.named_address_values.is_empty());
        self.named_address_values = named_address_values.into_iter().collect();
        self
    }

    fn compile_to_test_plan(
        &self,
        source_files: Vec<String>,
        deps: Vec<String>,
        bytecode_deps_files: Vec<String>,
    ) -> Option<TestPlan> {
        let addresses =
            verify_and_create_named_address_mapping(self.named_address_values.clone()).ok()?;
        let flags = Flags::testing();
        let (files, comments_and_compiler_res) =
            Compiler::from_files(None, source_files, deps, addresses)
                .set_flags(flags)
                .run::<PASS_CFGIR>()
                .unwrap();
        let compiler =
            diagnostics::unwrap_or_report_pass_diagnostics(&files, comments_and_compiler_res);

        let (compiler, cfgir) = compiler.into_ast();
        let compilation_env = compiler.compilation_env();
        let test_plan = unit_test::plan_builder::construct_test_plan(compilation_env, None, &cfgir);
        let mapped_files = compilation_env.mapped_files().clone();

        let compilation_result = compiler.at_cfgir(cfgir).build();
        let (units, warnings) =
            diagnostics::unwrap_or_report_pass_diagnostics(&files, compilation_result);
        diagnostics::report_warnings(&files, warnings);
        let units: Vec<_> = units.into_iter().map(|unit| unit.named_module).collect();

        let bytecode_deps_modules = bytecode_deps_files
            .iter()
            .map(|path| {
                let bytes = std::fs::read(path).unwrap();
                CompiledModule::deserialize_with_defaults(&bytes).unwrap()
            })
            .collect::<Vec<_>>();

        test_plan.map(|tests| TestPlan::new(tests, mapped_files, units, bytecode_deps_modules))
    }

    /// Build a test plan from a unit test config
    pub fn build_test_plan(&self) -> Option<TestPlan> {
        let deps = self.dep_files.clone();

        let TestPlan { module_info, .. } =
            self.compile_to_test_plan(deps.clone(), vec![], vec![])?;

        let mut test_plan = self.compile_to_test_plan(
            self.source_files.clone(),
            deps,
            self.bytecode_deps_files.clone(),
        )?;
        test_plan.module_info.extend(module_info);
        Some(test_plan)
    }

    /// Public entry point to Move unit testing as a library
    /// Returns `true` if all unit tests passed. Otherwise, returns `false`.
    pub fn run_and_report_unit_tests<W: Write + Send>(
        &self,
        test_plan: TestPlan,
        native_function_table: Option<NativeFunctionTable>,
        cost_table: Option<CostTable>,
        writer: W,
    ) -> Result<(W, bool)> {
        let shared_writer = Mutex::new(writer);

        let rand_num_iters = match self.rand_num_iters {
            Some(_) if self.seed.is_some() => {
                bail!(format!(
                    "Invalid arguments -- '{RAND_NUM_ITERS_FLAG}' and '{SEED_FLAG}' both set. \
                    You can only set one or the other at a time."
                ))
            }
            Some(0) => {
                bail!(format!(
                    "Invalid argument -- '{RAND_NUM_ITERS_FLAG}' set to zero. \
                    '{RAND_NUM_ITERS_FLAG}' must set be a positive integer."
                ))
            }
            Some(n) => n,
            None if self.seed.is_some() => 1,
            None => DEFAULT_RAND_ITERS,
        };

        if self.list {
            for (module_id, module_test_plan) in &test_plan.module_tests {
                for test_name in module_test_plan.tests.keys() {
                    writeln!(
                        shared_writer.lock().unwrap(),
                        "{}::{}: test",
                        format_module_id(&test_plan.module_info, module_id),
                        test_name
                    )?;
                }
            }
            return Ok((shared_writer.into_inner().unwrap(), true));
        }

        writeln!(shared_writer.lock().unwrap(), "Running Move unit tests")?;
        let trace_location = match &self.trace_execution {
            Some(None) => Some("traces".to_string()),
            Some(Some(path)) => Some(path.clone()),
            None => None,
        };
        let mut test_runner = TestRunner::new(
            self.gas_limit.unwrap_or(DEFAULT_EXECUTION_BOUND),
            self.num_threads,
            self.report_stacktrace_on_abort,
            self.seed,
            rand_num_iters,
            self.deterministic_generation,
            trace_location,
            test_plan,
            native_function_table,
            cost_table,
        )
        .unwrap();

        if let Some(filter_str) = &self.filter {
            test_runner.filter(filter_str)
        }

        let test_results = test_runner.run(&shared_writer).unwrap();
        if let Some(report_type) = &self.report_statistics {
            test_results.report_statistics(&shared_writer, report_type)?;
        }

        let ok = test_results.summarize(&shared_writer)?;

        let writer = shared_writer.into_inner().unwrap();
        Ok((writer, ok))
    }
}
