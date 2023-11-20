// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    borrow_analysis::BorrowAnalysisProcessor,
    clean_and_optimize::CleanAndOptimizeProcessor,
    data_invariant_instrumentation::DataInvariantInstrumentationProcessor,
    debug_instrumentation::DebugInstrumenter,
    eliminate_imm_refs::EliminateImmRefsProcessor,
    function_target_pipeline::{FunctionTargetPipeline, FunctionTargetProcessor},
    global_invariant_analysis::GlobalInvariantAnalysisProcessor,
    global_invariant_instrumentation::GlobalInvariantInstrumentationProcessor,
    inconsistency_check::InconsistencyCheckInstrumenter,
    livevar_analysis::LiveVarAnalysisProcessor,
    loop_analysis::LoopAnalysisProcessor,
    memory_instrumentation::MemoryInstrumentationProcessor,
    mono_analysis::MonoAnalysisProcessor,
    mut_ref_instrumentation::MutRefInstrumenter,
    mutation_tester::MutationTester,
    number_operation_analysis::NumberOperationProcessor,
    options::ProverOptions,
    reaching_def_analysis::ReachingDefProcessor,
    spec_instrumentation::SpecInstrumentationProcessor,
    usage_analysis::UsageProcessor,
    verification_analysis::VerificationAnalysisProcessor,
    well_formed_instrumentation::WellFormedInstrumentationProcessor,
};

pub fn default_pipeline_with_options(options: &ProverOptions) -> FunctionTargetPipeline {
    // NOTE: the order of these processors is import!
    let mut processors: Vec<Box<dyn FunctionTargetProcessor>> = vec![
        DebugInstrumenter::new(),
        // transformation and analysis
        EliminateImmRefsProcessor::new(),
        MutRefInstrumenter::new(),
        ReachingDefProcessor::new(),
        LiveVarAnalysisProcessor::new(),
        BorrowAnalysisProcessor::new_borrow_natives(options.borrow_natives.clone()),
        MemoryInstrumentationProcessor::new(),
        CleanAndOptimizeProcessor::new(),
        UsageProcessor::new(),
        VerificationAnalysisProcessor::new(),
    ];

    if !options.skip_loop_analysis {
        processors.push(LoopAnalysisProcessor::new());
    }

    processors.append(&mut vec![
        // spec instrumentation
        SpecInstrumentationProcessor::new(),
        GlobalInvariantAnalysisProcessor::new(),
        GlobalInvariantInstrumentationProcessor::new(),
        WellFormedInstrumentationProcessor::new(),
        DataInvariantInstrumentationProcessor::new(),
        // monomorphization
        MonoAnalysisProcessor::new(),
    ]);

    if options.mutation {
        // pass which may do nothing
        processors.push(MutationTester::new());
    }

    // inconsistency check instrumentation should be the last one in the pipeline
    if options.check_inconsistency {
        processors.push(InconsistencyCheckInstrumenter::new());
    }

    if !options.for_interpretation {
        processors.push(NumberOperationProcessor::new());
    }

    let mut res = FunctionTargetPipeline::default();
    for p in processors {
        res.add_processor(p);
    }
    res
}

pub fn default_pipeline() -> FunctionTargetPipeline {
    default_pipeline_with_options(&ProverOptions::default())
}

pub fn experimental_pipeline() -> FunctionTargetPipeline {
    // Enter your pipeline here
    let processors: Vec<Box<dyn FunctionTargetProcessor>> = vec![
        DebugInstrumenter::new(),
        // transformation and analysis
        EliminateImmRefsProcessor::new(),
        MutRefInstrumenter::new(),
        ReachingDefProcessor::new(),
        LiveVarAnalysisProcessor::new(),
        BorrowAnalysisProcessor::new(),
        MemoryInstrumentationProcessor::new(),
        CleanAndOptimizeProcessor::new(),
        UsageProcessor::new(),
        VerificationAnalysisProcessor::new(),
        LoopAnalysisProcessor::new(),
        // spec instrumentation
        SpecInstrumentationProcessor::new(),
        DataInvariantInstrumentationProcessor::new(),
        GlobalInvariantAnalysisProcessor::new(),
        GlobalInvariantInstrumentationProcessor::new(),
        // optimization
        MonoAnalysisProcessor::new(),
    ];

    let mut res = FunctionTargetPipeline::default();
    for p in processors {
        res.add_processor(p);
    }
    res
}
