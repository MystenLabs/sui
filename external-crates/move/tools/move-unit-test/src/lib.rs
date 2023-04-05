// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod cargo_runner;
pub mod extensions;
pub mod test_reporter;
pub mod test_runner;

use crate::test_runner::TestRunner;
use clap::*;
use move_command_line_common::files::verify_and_create_named_address_mapping;
use move_compiler::{
    self,
    diagnostics::{self, codes::Severity},
    shared::{self, NumericalAddress},
    unit_test::{self, TestPlan},
    Compiler, Flags, PASS_CFGIR,
};
use move_core_types::language_storage::ModuleId;
use move_vm_runtime::native_functions::NativeFunctionTable;
use move_vm_test_utils::gas_schedule::CostTable;
use std::{
    collections::BTreeMap,
    io::{Result, Write},
    marker::Send,
    sync::Mutex,
};

/// The default value bounding the amount of gas consumed in a test.
const DEFAULT_EXECUTION_BOUND: u64 = 1_000_000;

#[derive(Debug, Parser, Clone)]
#[clap(author, version, about)]
pub struct UnitTestingConfig {
    /// Bound the gas limit for any one test. If using custom gas table, this is the max number of instructions.
    #[clap(name = "gas_limit", short = 'i', long = "gas_limit")]
    pub gas_limit: Option<u64>,

    /// A filter string to determine which unit tests to run
    #[clap(name = "filter", short = 'f', long = "filter")]
    pub filter: Option<String>,

    /// List all tests
    #[clap(name = "list", short = 'l', long = "list")]
    pub list: bool,

    /// Number of threads to use for running tests.
    #[clap(
        name = "num_threads",
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
        takes_value(true),
        multiple_values(true),
        multiple_occurrences(true)
    )]
    pub dep_files: Vec<String>,

    /// Report test statistics at the end of testing. CSV report generated if 'csv' passed
    #[clap(name = "report_statistics", short = 's', long = "statistics")]
    pub report_statistics: Option<Option<String>>,

    /// Show the storage state at the end of execution of a failing test
    #[clap(name = "global_state_on_error", short = 'g', long = "state_on_error")]
    pub report_storage_on_error: bool,

    #[clap(
        name = "report_stacktrace_on_abort",
        short = 'r',
        long = "stacktrace_on_abort"
    )]
    pub report_stacktrace_on_abort: bool,

    /// Ignore compiler's warning, and continue run tests
    #[clap(name = "ignore_compile_warnings", long = "ignore_compile_warnings")]
    pub ignore_compile_warnings: bool,

    /// Named address mapping
    #[clap(
        name = "NAMED_ADDRESSES",
        short = 'a',
        long = "addresses",
        parse(try_from_str = shared::parse_named_address)
    )]
    pub named_address_values: Vec<(String, NumericalAddress)>,

    /// Source files
    #[clap(
        name = "sources",
        takes_value(true),
        multiple_values(true),
        multiple_occurrences(true)
    )]
    pub source_files: Vec<String>,

    /// Use the stackless bytecode interpreter to run the tests and cross check its results with
    /// the execution result from Move VM.
    #[clap(long = "stackless")]
    pub check_stackless_vm: bool,

    /// Verbose mode
    #[clap(short = 'v', long = "verbose")]
    pub verbose: bool,

    /// Whether the test output need to be printed out.
    #[clap(short = 'v', long = "verbose")]
    pub report_writeset: bool,

    /// Use the EVM-based execution backend.
    /// Does not work with --stackless.
    #[cfg(feature = "evm-backend")]
    #[clap(long = "evm")]
    pub evm: bool,
}

fn format_module_id(module_id: &ModuleId) -> String {
    format!(
        "0x{}::{}",
        module_id.address().short_str_lossless(),
        module_id.name()
    )
}

impl UnitTestingConfig {
    /// Create a unit testing config for use with `register_move_unit_tests`
    pub fn default_with_bound(bound: Option<u64>) -> Self {
        Self {
            gas_limit: bound.or(Some(DEFAULT_EXECUTION_BOUND)),
            filter: None,
            num_threads: 8,
            report_statistics: None,
            report_storage_on_error: false,
            report_stacktrace_on_abort: false,
            ignore_compile_warnings: false,
            source_files: vec![],
            dep_files: vec![],
            check_stackless_vm: false,
            verbose: false,
            list: false,
            named_address_values: vec![],
            report_writeset: false,

            #[cfg(feature = "evm-backend")]
            evm: false,
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
    ) -> Option<TestPlan> {
        let addresses =
            verify_and_create_named_address_mapping(self.named_address_values.clone()).ok()?;
        let (files, comments_and_compiler_res) =
            Compiler::from_files(source_files, deps, addresses)
                .set_flags(Flags::testing())
                .run::<PASS_CFGIR>()
                .unwrap();
        let (_, compiler) =
            diagnostics::unwrap_or_report_diagnostics(&files, comments_and_compiler_res);

        let (mut compiler, cfgir) = compiler.into_ast();
        let compilation_env = compiler.compilation_env();
        let test_plan = unit_test::plan_builder::construct_test_plan(compilation_env, None, &cfgir);

        if let Err(diags) =
            compilation_env.check_diags_at_or_above_severity(if self.ignore_compile_warnings {
                Severity::NonblockingError
            } else {
                Severity::Warning
            })
        {
            diagnostics::report_diagnostics(&files, diags);
        }

        let compilation_result = compiler.at_cfgir(cfgir).build();

        let (units, warnings) =
            diagnostics::unwrap_or_report_diagnostics(&files, compilation_result);
        diagnostics::report_warnings(&files, warnings);
        test_plan.map(|tests| TestPlan::new(tests, files, units))
    }

    /// Build a test plan from a unit test config
    pub fn build_test_plan(&self) -> Option<TestPlan> {
        let deps = self.dep_files.clone();

        let TestPlan {
            files, module_info, ..
        } = self.compile_to_test_plan(deps.clone(), vec![])?;

        let mut test_plan = self.compile_to_test_plan(self.source_files.clone(), deps)?;
        test_plan.module_info.extend(module_info.into_iter());
        test_plan.files.extend(files.into_iter());
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

        if self.list {
            for (module_id, test_plan) in &test_plan.module_tests {
                for test_name in test_plan.tests.keys() {
                    writeln!(
                        shared_writer.lock().unwrap(),
                        "{}::{}: test",
                        format_module_id(module_id),
                        test_name
                    )?;
                }
            }
            return Ok((shared_writer.into_inner().unwrap(), true));
        }

        writeln!(shared_writer.lock().unwrap(), "Running Move unit tests")?;
        let mut test_runner = TestRunner::new(
            self.gas_limit.unwrap_or(DEFAULT_EXECUTION_BOUND),
            self.num_threads,
            self.check_stackless_vm,
            self.verbose,
            self.report_storage_on_error,
            self.report_stacktrace_on_abort,
            test_plan,
            native_function_table,
            cost_table,
            verify_and_create_named_address_mapping(self.named_address_values.clone()).unwrap(),
            self.report_writeset,
            #[cfg(feature = "evm-backend")]
            self.evm,
        )
        .unwrap();

        if let Some(filter_str) = &self.filter {
            test_runner.filter(filter_str)
        }

        let test_results = test_runner.run(&shared_writer).unwrap();
        if let Some(report_type) = &self.report_statistics {
            test_results.report_statistics(&shared_writer, report_type)?;
        }

        if self.report_writeset {
            test_results.report_goldens(&shared_writer)?;
        }

        let ok = test_results.summarize(&shared_writer)?;

        let writer = shared_writer.into_inner().unwrap();
        Ok((writer, ok))
    }
}
