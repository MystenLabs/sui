// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use clap::Parser;
use move_binary_format::file_format::CompiledModule;
use move_coverage::{
    coverage_map::{CoverageMap, TraceMap},
    format_csv_summary, format_human_summary, summary,
};
use std::{
    fs::{self, File},
    io::{self, Write},
    path::Path,
};

#[derive(Debug, Parser)]
#[clap(
    name = "coverage-summaries",
    about = "Creates a coverage summary from the trace data collected from the Move VM",
    author,
    version
)]
struct Args {
    /// The path to the coverage map or trace file
    #[clap(long = "input-trace-path", short = 't')]
    pub input_trace_path: String,
    /// Whether the passed-in file is a raw trace file or a serialized coverage map
    #[clap(long = "is-raw-trace", short = 'r')]
    pub is_raw_trace_file: bool,
    /// The path to the module binary
    #[clap(long = "module-path", short = 'b')]
    pub module_binary_path: Option<String>,
    /// Optional path for summaries. Printed to stdout if not present.
    #[clap(long = "summary-path", short = 'o')]
    pub summary_path: Option<String>,
    /// Whether function coverage summaries should be displayed
    #[clap(long = "summarize-functions", short = 'f')]
    pub summarize_functions: bool,
    /// The path to the standard library binary directory for Move
    #[clap(long = "stdlib-path", short = 's')]
    pub stdlib_path: Option<String>,
    /// Whether path coverage should be derived (default is instruction coverage)
    #[clap(long = "derive-path-coverage", short = 'p')]
    pub derive_path_coverage: bool,
    /// Output CSV data of coverage
    #[clap(long = "csv", short = 'c')]
    pub csv_output: bool,
}

fn get_modules(args: &Args) -> Vec<CompiledModule> {
    let mut modules = Vec::new();
    if let Some(stdlib_path) = &args.stdlib_path {
        let stdlib_modules = fs::read_dir(stdlib_path).unwrap().map(|file| {
            let bytes = fs::read(file.unwrap().path()).unwrap();
            CompiledModule::deserialize_with_defaults(&bytes)
                .expect("Module blob can't be deserialized")
        });
        modules.extend(stdlib_modules);
    }

    if let Some(module_binary_path) = &args.module_binary_path {
        let bytecode_bytes = fs::read(module_binary_path).expect("Unable to read bytecode file");
        let compiled_module = CompiledModule::deserialize_with_defaults(&bytecode_bytes)
            .expect("Module blob can't be deserialized");
        modules.push(compiled_module);
    }

    if modules.is_empty() {
        panic!("No modules provided for coverage checking")
    }

    modules
}

fn main() {
    let args = Args::parse();
    let input_trace_path = Path::new(&args.input_trace_path);

    let mut summary_writer: Box<dyn Write> = match &args.summary_path {
        Some(x) => {
            let path = Path::new(x);
            Box::new(File::create(path).unwrap())
        }
        None => Box::new(io::stdout()),
    };

    let modules = get_modules(&args);
    if args.derive_path_coverage {
        let trace_map = if args.is_raw_trace_file {
            TraceMap::from_trace_file(input_trace_path)
        } else {
            TraceMap::from_binary_file(input_trace_path)
        };
        if !args.csv_output {
            format_human_summary(
                &modules,
                &trace_map,
                summary::summarize_path_cov,
                &mut summary_writer,
                args.summarize_functions,
            )
        } else {
            format_csv_summary(
                &modules,
                &trace_map,
                summary::summarize_path_cov,
                &mut summary_writer,
            )
        }
    } else {
        let coverage_map = if args.is_raw_trace_file {
            CoverageMap::from_trace_file(input_trace_path)
        } else {
            CoverageMap::from_binary_file(input_trace_path).unwrap()
        };
        let unified_exec_map = coverage_map.to_unified_exec_map();
        if !args.csv_output {
            format_human_summary(
                &modules,
                &unified_exec_map,
                summary::summarize_inst_cov,
                &mut summary_writer,
                args.summarize_functions,
            )
        } else {
            format_csv_summary(
                &modules,
                &unified_exec_map,
                summary::summarize_inst_cov,
                &mut summary_writer,
            )
        }
    }
}
