// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::reroot_path;
use clap::*;
use move_compiler::compiled_unit::{CompiledUnit, NamedCompiledModule};
use move_coverage::{
    coverage_map::CoverageMap, format_csv_summary, format_human_summary,
    source_coverage::SourceCoverageBuilder, summary::summarize_inst_cov,
};
use move_disassembler::disassembler::Disassembler;
use move_package::BuildConfig;
use std::path::PathBuf;

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
    pub fn execute(self, path: Option<PathBuf>, config: BuildConfig) -> anyhow::Result<()> {
        let path = reroot_path(path)?;
        let coverage_map = CoverageMap::from_binary_file(path.join(".coverage_map.mvcov"))?;
        let package = config.compile_package(&path, &mut Vec::new())?;
        let modules: Vec<_> = package
            .root_modules()
            .filter_map(|unit| match &unit.unit {
                CompiledUnit::Module(NamedCompiledModule { module, .. }) => Some(module.clone()),
                _ => None,
            })
            .collect();
        match self.options {
            CoverageSummaryOptions::Source { module_name } => {
                let unit = package.get_module_by_name_from_root(&module_name)?;
                let source_path = &unit.source_path;
                let (module, source_map) = match &unit.unit {
                    CompiledUnit::Module(NamedCompiledModule {
                        module, source_map, ..
                    }) => (module, source_map),
                    _ => panic!("Should all be modules"),
                };
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
                        modules.as_slice(),
                        &coverage_map,
                        summarize_inst_cov,
                        &mut std::io::stdout(),
                    )
                } else {
                    format_human_summary(
                        modules.as_slice(),
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
        }
        Ok(())
    }
}
