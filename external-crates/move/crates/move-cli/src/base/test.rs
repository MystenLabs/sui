// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::reroot_path;
use crate::NativeFunctionRecord;
use anyhow::Result;
use clap::*;
use move_binary_format::CompiledModule;
use move_command_line_common::files::MOVE_COVERAGE_MAP_EXTENSION;
use move_compiler::{
    diagnostics::{self, Diagnostics},
    shared::{NumberFormat, NumericalAddress},
    unit_test::{plan_builder::construct_test_plan, TestPlan},
    PASS_CFGIR,
};
use move_coverage::coverage_map::{output_map_to_file, CoverageMap};
use move_package::{compilation::build_plan::BuildPlan, BuildConfig};
use move_unit_test::UnitTestingConfig;
use move_vm_test_utils::gas_schedule::CostTable;
use std::{io::Write, path::Path, process::ExitStatus};
// if windows
#[cfg(target_family = "windows")]
use std::os::windows::process::ExitStatusExt;
// if unix
#[cfg(target_family = "unix")]
use std::os::unix::prelude::ExitStatusExt;
// if not windows nor unix
#[cfg(not(any(target_family = "windows", target_family = "unix")))]
compile_error!("Unsupported OS, currently we only support windows and unix family");

/// Run Move unit tests in this package.
#[derive(Parser)]
#[clap(name = "test")]
pub struct Test {
    /// Bound the amount of gas used by any one test.
    #[clap(name = "gas-limit", short = 'i', long = "gas-limit")]
    pub gas_limit: Option<u64>,
    /// An optional filter string to determine which unit tests to run. A unit test will be run only if it
    /// contains this string in its fully qualified (<addr>::<module_name>::<fn_name>) name.
    #[clap(name = "filter")]
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
    /// Report test statistics at the end of testing. CSV report generated if 'csv' passed
    #[clap(name = "report-statistics", short = 's', long = "statistics")]
    pub report_statistics: Option<Option<String>>,

    /// Verbose mode
    #[clap(long = "verbose")]
    pub verbose_mode: bool,
    /// Collect coverage information for later use with the various `move coverage` subcommands. Currently supported only in debug builds.
    #[clap(long = "coverage")]
    pub compute_coverage: bool,

    /// The seed to use for the randomness generator.
    #[clap(name = "seed", long = "seed")]
    pub seed: Option<u64>,

    /// The number of iterations to run each test that uses generated values (only used with #[random_test]).
    #[clap(name = "rand-num-iters", long = "rand-num-iters")]
    pub rand_num_iters: Option<u64>,

    // Enable tracing for tests
    #[clap(long = "trace-execution", value_name = "PATH")]
    pub trace_execution: Option<Option<String>>,
}

impl Test {
    pub fn execute(
        self,
        path: Option<&Path>,
        config: BuildConfig,
        natives: Vec<NativeFunctionRecord>,
        cost_table: Option<CostTable>,
    ) -> anyhow::Result<()> {
        let rerooted_path = reroot_path(path)?;
        let compute_coverage = self.compute_coverage;
        // save disassembly if trace execution is enabled
        let save_disassembly = self.trace_execution.is_some();
        let result = run_move_unit_tests(
            &rerooted_path,
            config,
            self.unit_test_config(),
            natives,
            cost_table,
            compute_coverage,
            save_disassembly,
            &mut std::io::stdout(),
        )?;

        // Return a non-zero exit code if any test failed
        if let (UnitTestResult::Failure, _) = result {
            std::process::exit(1)
        }
        Ok(())
    }

    pub fn unit_test_config(self) -> UnitTestingConfig {
        let Self {
            gas_limit,
            filter,
            list,
            num_threads,
            report_statistics,
            verbose_mode,
            compute_coverage: _,
            seed,
            rand_num_iters,
            trace_execution,
        } = self;
        UnitTestingConfig {
            gas_limit,
            filter,
            list,
            num_threads,
            report_statistics,
            verbose: verbose_mode,
            seed,
            rand_num_iters,
            trace_execution,
            ..UnitTestingConfig::default_with_bound(None)
        }
    }
}

/// Encapsulates the possible returned states when running unit tests on a move package.
#[derive(PartialEq, Eq, Debug)]
pub enum UnitTestResult {
    Success,
    Failure,
}

pub fn run_move_unit_tests<W: Write + Send>(
    pkg_path: &Path,
    mut build_config: move_package::BuildConfig,
    mut unit_test_config: UnitTestingConfig,
    natives: Vec<NativeFunctionRecord>,
    cost_table: Option<CostTable>,
    compute_coverage: bool,
    save_disassembly: bool,
    writer: &mut W,
) -> Result<(UnitTestResult, Option<Diagnostics>)> {
    let mut test_plan = None;
    build_config.test_mode = true;
    build_config.dev_mode = true;
    build_config.save_disassembly = save_disassembly;

    // Build the resolution graph (resolution graph diagnostics are only needed for CLI commands so
    // ignore them by passing a vector as the writer)
    let resolution_graph =
        build_config.resolution_graph_for_package(pkg_path, None, &mut Vec::new())?;

    // Note: unit_test_config.named_address_values is always set to vec![] (the default value) before
    // being passed in.
    unit_test_config.named_address_values = resolution_graph
        .extract_named_address_mapping()
        .map(|(name, addr)| {
            (
                name.to_string(),
                NumericalAddress::new(addr.into_bytes(), NumberFormat::Hex),
            )
        })
        .collect();

    // Collect all the bytecode modules that are dependencies of the package. We need to do this
    // because they're not returned by the compilation result, but we need to add them in the
    // VM storage.
    let mut bytecode_deps_modules = vec![];
    for pkg in resolution_graph.package_table.values() {
        let source_available = !pkg
            .get_sources(&resolution_graph.build_options)
            .unwrap()
            .is_empty();
        if source_available {
            continue;
        }
        for bytes in pkg.get_bytecodes_bytes()? {
            let module = CompiledModule::deserialize_with_defaults(&bytes)?;
            bytecode_deps_modules.push(module);
        }
    }

    let root_package = resolution_graph.root_package();
    let build_plan = BuildPlan::create(resolution_graph)?;

    // Compile the package. We need to intercede in the compilation, process being performed by the
    // Move package system, to first grab the compilation env, construct the test plan from it, and
    // then save it, before resuming the rest of the compilation and returning the results and
    // control back to the Move package system.
    let mut warning_diags = None;
    build_plan.compile_with_driver(writer, |compiler| {
        let (files, comments_and_compiler_res) = compiler.run::<PASS_CFGIR>().unwrap();
        let compiler =
            diagnostics::unwrap_or_report_pass_diagnostics(&files, comments_and_compiler_res);
        let (compiler, cfgir) = compiler.into_ast();
        let compilation_env = compiler.compilation_env();
        let built_test_plan = construct_test_plan(compilation_env, Some(root_package), &cfgir);
        let mapped_files = compilation_env.mapped_files().clone();

        let compilation_result = compiler.at_cfgir(cfgir).build();
        let (units, warnings) =
            diagnostics::unwrap_or_report_pass_diagnostics(&files, compilation_result);
        diagnostics::report_warnings(&files, warnings.clone());
        let named_units: Vec<_> = units
            .clone()
            .into_iter()
            .map(|unit| unit.named_module)
            .collect();
        test_plan = Some((built_test_plan, mapped_files, named_units));
        warning_diags = Some(warnings);
        Ok((files, units))
    })?;

    let (test_plan, mapped_files, units) = test_plan.unwrap();
    let test_plan = test_plan.unwrap();
    let no_tests = test_plan.is_empty();
    let test_plan = TestPlan::new(test_plan, mapped_files, units, bytecode_deps_modules);

    let trace_path = pkg_path.join(".trace");
    let coverage_map_path = pkg_path
        .join(".coverage_map")
        .with_extension(MOVE_COVERAGE_MAP_EXTENSION);
    let cleanup_trace = || {
        if compute_coverage && trace_path.exists() {
            std::fs::remove_file(&trace_path).unwrap();
        }
    };

    cleanup_trace();

    // If we need to compute test coverage set the VM tracking environment variable since we will
    // need this trace to construct the coverage information.
    if compute_coverage {
        std::env::set_var("MOVE_VM_TRACE", &trace_path);
    }

    // Run the tests. If any of the tests fail, then we don't produce a coverage report, so cleanup
    // the trace files.
    if !unit_test_config
        .run_and_report_unit_tests(test_plan, Some(natives), cost_table, writer)?
        .1
    {
        cleanup_trace();
        return Ok((UnitTestResult::Failure, warning_diags));
    }

    // Compute the coverage map. This will be used by other commands after this.
    if compute_coverage && !no_tests {
        let coverage_map = CoverageMap::from_trace_file(trace_path);
        output_map_to_file(coverage_map_path, &coverage_map).unwrap();
    }
    Ok((UnitTestResult::Success, warning_diags))
}

impl From<UnitTestResult> for ExitStatus {
    fn from(result: UnitTestResult) -> Self {
        match result {
            UnitTestResult::Success => ExitStatus::from_raw(0),
            UnitTestResult::Failure => ExitStatus::from_raw(1),
        }
    }
}
