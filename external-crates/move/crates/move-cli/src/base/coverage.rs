// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::reroot_path;
use clap::*;
use move_compiler::compiled_unit::NamedCompiledModule;
use move_coverage::{
    coverage_map::CoverageMap, differential_coverage, format_csv_summary, format_human_summary,
    lcov, source_coverage::SourceCoverageBuilder, summary::summarize_inst_cov,
};
use move_disassembler::disassembler::Disassembler;
use move_package_alt_compilation::{build_config::BuildConfig, find_env};

use move_package_alt::{flavor::MoveFlavor, schema::Environment};
use move_trace_format::format::MoveTraceReader;
use std::{
    fs::File,
    path::{Path, PathBuf},
};

const COVERAGE_FILE_NAME: &str = "lcov.info";
const DIFFERENTIAL: &str = "diff";

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
    /// Generate LCOV coverage information for the package. Requires traces to be present.
    /// Run tests with `--trace` to generate traces.
    #[clap(name = "lcov")]
    Lcov {
        /// Compute differential coverage for the provided test name. Lines that are hit by this
        /// test only will show as covered, and lines that are hit by both this test and all other
        /// tests will show as "uncovered". Otherwise lines are not annotated with coverage
        /// information.
        #[clap(long = "differential-test")]
        differential: Option<String>,
        /// Compute coverage for the provided test name. Only this test will contribute to the
        /// coverage calculation.
        #[clap(long = "only-test", conflicts_with = "differential")]
        test: Option<String>,
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
    pub async fn execute<F: MoveFlavor>(
        self,
        path: Option<&Path>,
        config: BuildConfig,
    ) -> anyhow::Result<()> {
        let path = reroot_path(path)?;
        let env = find_env::<F>(&path, &config)?;

        // We treat lcov-format coverage differently because it requires traces to be present, and
        // we don't use the old trace format for it.
        if let CoverageSummaryOptions::Lcov { differential, test } = self.options {
            return Self::output_lcov_coverage::<F>(path, &env, config, differential, test).await;
        }

        let package = config
            .compile_package::<F, _>(&path, &env, &mut Vec::new())
            .await?;
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
            CoverageSummaryOptions::Lcov { .. } => {
                unreachable!()
            }
        }
        Ok(())
    }

    pub async fn output_lcov_coverage<F: MoveFlavor>(
        path: PathBuf,
        env: &Environment,
        mut config: BuildConfig,
        differential: Option<String>,
        test: Option<String>,
    ) -> anyhow::Result<()> {
        // Make sure we always compile the package in test mode so we get correct source maps.
        config.test_mode = true;
        let package = config
            .compile_package::<F, _>(&path, env, &mut Vec::new())
            .await?;
        let units: Vec<_> = package
            .all_compiled_units_with_source()
            .cloned()
            .map(|unit| (unit.unit, unit.source_path))
            .collect();
        let traces = path.join("traces");
        let sanitize_name = |s: &str| s.replace("::", "__");
        let trace_of_test = |test_name: &str| {
            let trace_substr_name = format!("{}.", sanitize_name(test_name));
            std::fs::read_dir(&traces)?
                .filter_map(|entry| {
                    let entry = entry.unwrap();
                    let path = entry.path();
                    if path.is_file()
                        && path
                            .file_name()
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .contains(&trace_substr_name)
                    {
                        Some(path)
                    } else {
                        None
                    }
                })
                .next()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "No trace found for test {}. Please run with `--trace` to generate traces.",
                        test_name
                    )
                })
        };

        if let Some(test_name) = test {
            let mut coverage = lcov::PackageRecordKeeper::new(units, package.file_map.clone());
            let trace_path = trace_of_test(&test_name)?;
            let file = File::open(&trace_path)?;
            let move_trace_reader = MoveTraceReader::new(file)?;
            coverage.calculate_coverage(move_trace_reader);
            std::fs::write(
                &path.join(format!(
                    "{}.{COVERAGE_FILE_NAME}",
                    sanitize_name(&test_name)
                )),
                coverage.lcov_record_string(),
            )?;
        } else {
            let mut coverage =
                lcov::PackageRecordKeeper::new(units.clone(), package.file_map.clone());
            let differential_test_path = differential
                .as_ref()
                .map(|s| trace_of_test(s))
                .transpose()?;

            for entry in std::fs::read_dir(&traces)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file()
                    && differential_test_path
                        .as_ref()
                        .is_none_or(|diff_path| diff_path != &path)
                {
                    let file = File::open(&path)?;
                    let move_trace_reader = MoveTraceReader::new(file)?;
                    coverage.calculate_coverage(move_trace_reader);
                }
            }

            if let Some(differential_test_name) = differential {
                let trace_path =
                    differential_test_path.expect("Differential test path is already computed");
                let file = File::open(&trace_path)?;
                let move_trace_reader = MoveTraceReader::new(file)?;
                let mut test_coverage =
                    lcov::PackageRecordKeeper::new(units, package.file_map.clone());
                test_coverage.calculate_coverage(move_trace_reader);

                let differential_string =
                    differential_coverage::differential_report(&coverage, &test_coverage)?;

                std::fs::write(
                    &path.join(format!(
                        "{}.{DIFFERENTIAL}.{COVERAGE_FILE_NAME}",
                        sanitize_name(&differential_test_name)
                    )),
                    differential_string,
                )?;
            } else {
                std::fs::write(
                    &path.join(COVERAGE_FILE_NAME),
                    coverage.lcov_record_string(),
                )?;
            }
        };

        Ok(())
    }
}
