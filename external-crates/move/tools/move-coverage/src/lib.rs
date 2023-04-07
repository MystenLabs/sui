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

/**
  "source_files": [
        {
            "name": "lib/kernel/match/offset.ml",
            "source_digest": "24192ec6a021028302e42905973a21b3",
            "coverage": [null,null,null,null,null,null,null,null,null,null,972,972,null,972,null,972,null,null,3606954,151710,null,null,null,null,151710,3455244,null,null,0,null,3455244,null,null,972,null,null,6858,350,350,null,608,5900,null,null,2802,3098,3084,null,14,null,null,null,972,0,null,null,null,1216,3606012,1609984,74927,1921101,null,null,1216]
        },
*/
pub fn format_json_summary<M, F, W: Write>(
    modules: &[CompiledModule],
    coverage_map: &M,
    summary_func: F,
    summary_writer: &mut W,
) where
    F: Fn(&CompiledModule, &M) -> ModuleSummary,
{
    writeln!(summary_writer, "{{\"source_files\": [").unwrap();
    for module in modules.iter() {
        let coverage_summary = summary_func(module, coverage_map);
        coverage_summary.summarize_csv(summary_writer).unwrap();
    }
    writeln!(summary_writer, "]}}").unwrap();
}
