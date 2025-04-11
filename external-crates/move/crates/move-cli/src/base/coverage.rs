// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::reroot_path;
use clap::*;
use move_compiler::compiled_unit::NamedCompiledModule;
use move_coverage::{
    coverage_map::CoverageMap, format_csv_summary, format_human_summary, lcov,
    source_coverage::SourceCoverageBuilder, summary::summarize_inst_cov,
};
use move_disassembler::disassembler::Disassembler;
use move_package::BuildConfig;
use move_trace_format::format::MoveTraceReader;
use std::{fs::File, path::Path};

const COVERAGE_FILE_NAME: &str = "lcov.info";

#[derive(Parser)]
pub enum CoverageSummaryOptions {
    /// Display a coverage summary for all modules in this package
    #[clap(name = "summary")]
    Summary {
        /// Whether function coverage summaries should be displayed
        #[clap(long = "summarize-functions")]
        functions: bool,
        /// Output CSV data of coverage
        #[clap(long = "csv")]
        output_csv: bool,
    },
    /// Display coverage information about the module against source code
    #[clap(name = "source")]
    Source {
        #[clap(long = "module")]
        module_name: String,
    },
    /// Display coverage information about the module against disassembled bytecode
    #[clap(name = "bytecode")]
    Bytecode {
        #[clap(long = "module")]
        module_name: String,
    },
    #[clap(name = "lcov")]
    Lcov,
}

/// Inspect test coverage for this package. A previous test run with the `--coverage` flag must
/// have previously been run.
#[derive(Parser)]
#[clap(name = "coverage")]
pub struct Coverage {
    #[clap(subcommand)]
    pub options: CoverageSummaryOptions,
}

impl Coverage {
    pub fn execute(self, path: Option<&Path>, mut config: BuildConfig) -> anyhow::Result<()> {
        let path = reroot_path(path)?;

        // We treat lcov-format coverage differently because it requires traces to be present, and
        // we don't use the old trace format for it.
        if let CoverageSummaryOptions::Lcov = self.options {
            // Make sure we always compile the package in test mode so we get correct source maps.
            config.test_mode = true;
            let package = config.compile_package(&path, &mut Vec::new())?;
            let units: Vec<_> = package
                .all_modules()
                .cloned()
                .map(|unit| (unit.unit, unit.source_path))
                .collect();
            let traces = path.join("traces");
            let mut coverage = lcov::PackageRecordKeeper::new(units, package.file_map.clone());
            for entry in std::fs::read_dir(traces)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    let file = File::open(&path)?;
                    let move_trace_reader = MoveTraceReader::new(file)?;
                    coverage.calculate_coverage(move_trace_reader);
                }
            }

            std::fs::write(
                &path.join(COVERAGE_FILE_NAME),
                coverage.lcov_record_string(),
            )?;
            return Ok(());
        }

        let package = config.compile_package(&path, &mut Vec::new())?;
        let modules = package.root_modules().map(|unit| &unit.unit.module);
        let coverage_map = CoverageMap::from_binary_file(path.join(".coverage_map.mvcov"))?;
        match self.options {
            CoverageSummaryOptions::Source { module_name } => {
                let unit = package.get_module_by_name_from_root(&module_name)?;
                let source_path = &unit.source_path;
                let NamedCompiledModule {
                    module, source_map, ..
                } = &unit.unit;
                let source_coverage = SourceCoverageBuilder::new(module, &coverage_map, source_map);
                source_coverage
                    .compute_source_coverage(source_path)
                    .output_source_coverage(&mut std::io::stdout())
                    .unwrap();
            }
            CoverageSummaryOptions::Summary {
                functions,
                output_csv,
                ..
            } => {
                let coverage_map = coverage_map.to_unified_exec_map();
                if output_csv {
                    format_csv_summary(
                        modules,
                        &coverage_map,
                        summarize_inst_cov,
                        &mut std::io::stdout(),
                    )
                } else {
                    format_human_summary(
                        modules,
                        &coverage_map,
                        summarize_inst_cov,
                        &mut std::io::stdout(),
                        functions,
                    )
                }
            }
            CoverageSummaryOptions::Bytecode { module_name } => {
                let unit = package.get_module_by_name_from_root(&module_name)?;
                let mut disassembler = Disassembler::from_unit(&unit.unit);
                disassembler.add_coverage_map(coverage_map.to_unified_exec_map());
                println!("{}", disassembler.disassemble()?);
            }
            CoverageSummaryOptions::Lcov => {
                unreachable!()
            }
        }
        Ok(())
    }
}
