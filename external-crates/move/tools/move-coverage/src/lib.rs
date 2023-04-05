// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::summary::ModuleSummary;
use move_binary_format::CompiledModule;
use std::io::Write;

pub mod coverage_map;
pub mod source_coverage;
pub mod summary;

pub fn format_human_summary<M, F, W: Write>(
    modules: &[CompiledModule],
    coverage_map: &M,
    summary_func: F,
    summary_writer: &mut W,
    summarize_functions: bool,
) where
    F: Fn(&CompiledModule, &M) -> ModuleSummary,
{
    writeln!(summary_writer, "+-------------------------+").unwrap();
    writeln!(summary_writer, "| Move Coverage Summary   |").unwrap();
    writeln!(summary_writer, "+-------------------------+").unwrap();

    let mut total_covered = 0;
    let mut total_instructions = 0;

    for module in modules.iter() {
        let coverage_summary = summary_func(module, coverage_map);
        let (total, covered) = coverage_summary
            .summarize_human(summary_writer, summarize_functions)
            .unwrap();
        total_covered += covered;
        total_instructions += total;
    }

    writeln!(summary_writer, "+-------------------------+").unwrap();
    writeln!(
        summary_writer,
        "| % Move Coverage: {:.2}  |",
        (total_covered as f64 / total_instructions as f64) * 100f64
    )
    .unwrap();
    writeln!(summary_writer, "+-------------------------+").unwrap();
}

pub fn format_csv_summary<M, F, W: Write>(
    modules: &[CompiledModule],
    coverage_map: &M,
    summary_func: F,
    summary_writer: &mut W,
) where
    F: Fn(&CompiledModule, &M) -> ModuleSummary,
{
    writeln!(summary_writer, "ModuleName,FunctionName,Covered,Uncovered").unwrap();

    for module in modules.iter() {
        let coverage_summary = summary_func(module, coverage_map);
        coverage_summary.summarize_csv(summary_writer).unwrap();
    }
}
