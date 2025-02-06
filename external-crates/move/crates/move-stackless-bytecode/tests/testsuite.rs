// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use codespan_reporting::{diagnostic::Severity, term::termcolor::Buffer};
use move_command_line_common::insta_assert;
use move_compiler::{diagnostics::warning_filters::WarningFiltersBuilder, shared::PackagePaths};
use move_model::{model::GlobalEnv, options::ModelBuilderOptions, run_model_builder_with_options};
use move_stackless_bytecode::{
    borrow_analysis::BorrowAnalysisProcessor,
    clean_and_optimize::CleanAndOptimizeProcessor,
    eliminate_imm_refs::EliminateImmRefsProcessor,
    escape_analysis::EscapeAnalysisProcessor,
    function_target_pipeline::{
        FunctionTargetPipeline, FunctionTargetsHolder, ProcessorResultDisplay,
    },
    livevar_analysis::LiveVarAnalysisProcessor,
    memory_instrumentation::MemoryInstrumentationProcessor,
    mut_ref_instrumentation::MutRefInstrumenter,
    options::ProverOptions,
    print_targets_for_test,
    reaching_def_analysis::ReachingDefProcessor,
};
use regex::Regex;
use std::{fs::File, io::Read, path::Path};

// Extracts lines out of some text file where each line starts with `start` which can be a regular
// expressions. Returns the list of such lines with `start` stripped. Use as in
// `extract_test_directives(file, "// dep:")`.
fn extract_test_directives(path: &Path, start: &str) -> anyhow::Result<Vec<String>> {
    let rex = Regex::new(&format!("(?m)^{}(?P<ann>.*?)$", start)).unwrap();
    let mut content = String::new();
    let mut file = File::open(path)?;
    file.read_to_string(&mut content)?;
    let mut at = 0;
    let mut res = vec![];
    while let Some(cap) = rex.captures(&content[at..]) {
        res.push(cap.name("ann").unwrap().as_str().trim().to_string());
        at += cap.get(0).unwrap().end();
    }
    Ok(res)
}

fn get_tested_transformation_pipeline(
    dir_name: &str,
) -> anyhow::Result<Option<FunctionTargetPipeline>> {
    match dir_name {
        "from_move" => Ok(None),
        "eliminate_imm_refs" => {
            let mut pipeline = FunctionTargetPipeline::default();
            pipeline.add_processor(EliminateImmRefsProcessor::new());
            Ok(Some(pipeline))
        }
        "mut_ref_instrumentation" => {
            let mut pipeline = FunctionTargetPipeline::default();
            pipeline.add_processor(EliminateImmRefsProcessor::new());
            pipeline.add_processor(MutRefInstrumenter::new());
            Ok(Some(pipeline))
        }
        "reaching_def" => {
            let mut pipeline = FunctionTargetPipeline::default();
            pipeline.add_processor(EliminateImmRefsProcessor::new());
            pipeline.add_processor(MutRefInstrumenter::new());
            pipeline.add_processor(ReachingDefProcessor::new());
            Ok(Some(pipeline))
        }
        "livevar" => {
            let mut pipeline = FunctionTargetPipeline::default();
            pipeline.add_processor(EliminateImmRefsProcessor::new());
            pipeline.add_processor(MutRefInstrumenter::new());
            pipeline.add_processor(ReachingDefProcessor::new());
            pipeline.add_processor(LiveVarAnalysisProcessor::new());
            Ok(Some(pipeline))
        }
        "borrow" => {
            let mut pipeline = FunctionTargetPipeline::default();
            pipeline.add_processor(EliminateImmRefsProcessor::new());
            pipeline.add_processor(MutRefInstrumenter::new());
            pipeline.add_processor(ReachingDefProcessor::new());
            pipeline.add_processor(LiveVarAnalysisProcessor::new());
            pipeline.add_processor(BorrowAnalysisProcessor::new());
            Ok(Some(pipeline))
        }
        "borrow_strong" => {
            let mut pipeline = FunctionTargetPipeline::default();
            pipeline.add_processor(EliminateImmRefsProcessor::new());
            pipeline.add_processor(MutRefInstrumenter::new());
            pipeline.add_processor(ReachingDefProcessor::new());
            pipeline.add_processor(LiveVarAnalysisProcessor::new());
            pipeline.add_processor(BorrowAnalysisProcessor::new());
            Ok(Some(pipeline))
        }
        "escape_analysis" => {
            let mut pipeline = FunctionTargetPipeline::default();
            pipeline.add_processor(Box::new(EscapeAnalysisProcessor {}));
            Ok(Some(pipeline))
        }
        "memory_instr" => {
            let mut pipeline = FunctionTargetPipeline::default();
            pipeline.add_processor(EliminateImmRefsProcessor::new());
            pipeline.add_processor(MutRefInstrumenter::new());
            pipeline.add_processor(ReachingDefProcessor::new());
            pipeline.add_processor(LiveVarAnalysisProcessor::new());
            pipeline.add_processor(BorrowAnalysisProcessor::new());
            pipeline.add_processor(MemoryInstrumentationProcessor::new());
            Ok(Some(pipeline))
        }
        "clean_and_optimize" => {
            let mut pipeline = FunctionTargetPipeline::default();
            pipeline.add_processor(EliminateImmRefsProcessor::new());
            pipeline.add_processor(MutRefInstrumenter::new());
            pipeline.add_processor(ReachingDefProcessor::new());
            pipeline.add_processor(LiveVarAnalysisProcessor::new());
            pipeline.add_processor(BorrowAnalysisProcessor::new());
            pipeline.add_processor(MemoryInstrumentationProcessor::new());
            pipeline.add_processor(CleanAndOptimizeProcessor::new());
            Ok(Some(pipeline))
        }
        _ => Err(anyhow!(
            "the sub-directory `{}` has no associated pipeline to test",
            dir_name
        )),
    }
}

fn test_runner(path: &Path) -> datatest_stable::Result<()> {
    let mut sources = extract_test_directives(path, "// dep:")?;
    sources.push(path.to_string_lossy().to_string());
    let env: GlobalEnv = run_model_builder_with_options(
        vec![PackagePaths {
            name: None,
            paths: sources,
            named_address_map: move_stdlib::move_stdlib_named_addresses(),
        }],
        vec![],
        ModelBuilderOptions::default(),
        Some(WarningFiltersBuilder::unused_warnings_filter_for_test()),
    )?;
    let out = if env.has_errors() {
        let mut error_writer = Buffer::no_color();
        env.report_diag(&mut error_writer, Severity::Error);
        String::from_utf8_lossy(&error_writer.into_inner()).to_string()
    } else {
        let options = ProverOptions {
            stable_test_output: true,
            ..Default::default()
        };
        env.set_extension(options);
        let dir_name = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|p| p.to_str())
            .ok_or_else(|| anyhow!("bad file name"))?;
        let pipeline_opt = get_tested_transformation_pipeline(dir_name)?;

        // Initialize and print function targets
        let mut text = String::new();
        let mut targets = FunctionTargetsHolder::default();
        for module_env in env.get_modules() {
            for func_env in module_env.get_functions() {
                targets.add_target(&func_env);
            }
        }
        text += &print_targets_for_test(&env, "initial translation from Move", &targets);

        // Run pipeline if any
        if let Some(pipeline) = pipeline_opt {
            pipeline.run(&env, &mut targets);
            let processor = pipeline.last_processor();
            if !processor.is_single_run() {
                text += &print_targets_for_test(
                    &env,
                    &format!("after pipeline `{}`", dir_name),
                    &targets,
                );
            }
            text += &ProcessorResultDisplay {
                env: &env,
                targets: &targets,
                processor,
            }
            .to_string();
        }
        // add Warning and Error diagnostics to output
        let mut error_writer = Buffer::no_color();
        if env.has_errors() || env.has_warnings() {
            env.report_diag(&mut error_writer, Severity::Warning);
            text += "============ Diagnostics ================\n";
            text += &String::from_utf8_lossy(&error_writer.into_inner());
        }
        text
    };
    insta_assert! {
        input_path: path,
        contents: out,
    };
    Ok(())
}

datatest_stable::harness!(test_runner, "tests", r".*\.move");
