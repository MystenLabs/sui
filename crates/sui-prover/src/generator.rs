#![forbid(unsafe_code)]

use std::cell::RefCell;

use crate::generator_options::Options;
use anyhow::anyhow;
use bimap::btree::BiBTreeMap;
use codespan_reporting::{
    diagnostic::Severity,
    term::termcolor::{Buffer, ColorChoice, StandardStream, WriteColor},
};
#[allow(unused_imports)]
use log::{debug, info, warn};
use move_model::{
    code_writer::CodeWriter, model::GlobalEnv, ty::Type,
};
use crate::boogie_backend::{
    lib::add_prelude, boogie_wrapper::BoogieWrapper, bytecode_translator::BoogieTranslator,
};
use move_stackless_bytecode::{
    escape_analysis::EscapeAnalysisProcessor,
    function_target_pipeline::{
        FunctionTargetPipeline, FunctionTargetsHolder, FunctionVariant, VerificationFlavor,
    },
    number_operation::GlobalNumberOperationState,
    pipeline_factory,
};
use std::{
    fs,
    path::Path,
    time::Instant,
};

pub fn create_init_num_operation_state(env: &GlobalEnv) {
    let mut global_state: GlobalNumberOperationState = Default::default();
    for module_env in env.get_modules() {
        for struct_env in module_env.get_structs() {
            global_state.create_initial_struct_oper_state(&struct_env);
        }
        for fun_env in module_env.get_functions() {
            global_state.create_initial_func_oper_state(&fun_env);
        }
    }
    //global_state.create_initial_exp_oper_state(env);
    env.set_extension(global_state);
}

pub fn run_boogie_gen(env: &GlobalEnv, options: Options) -> anyhow::Result<()> {
    let mut error_writer = StandardStream::stderr(ColorChoice::Auto);

    run_move_prover_with_model(env, &mut error_writer, options, None)
}

pub fn run_move_prover_with_model<W: WriteColor>(
    env: &GlobalEnv,
    error_writer: &mut W,
    options: Options,
    timer: Option<Instant>,
) -> anyhow::Result<()> {
    let now = timer.unwrap_or_else(Instant::now);

    let build_duration = now.elapsed();
    check_errors(
        env,
        &options,
        error_writer,
        "exiting with model building errors",
    )?;
    // TODO: delete duplicate diagnostics reporting
    env.report_diag(error_writer, options.prover.report_severity);

    // Add the prover options as an extension to the environment, so they can be accessed
    // from there.
    env.set_extension(options.prover.clone());

    // Populate initial number operation state for each function and struct based on the pragma
    create_init_num_operation_state(env);

    // Until this point, prover and docgen have same code. Here we part ways.
    if options.run_docgen {
        //return run_docgen(env, &options, error_writer, now);
    }
    // Same for escape analysis
    if options.run_escape {
        return {
            run_escape(env, &options, now);
            Ok(())
        };
    }

    // Check correct backend versions.
    options.backend.check_tool_versions()?;

    // Create and process bytecode
    let now = Instant::now();
    let (targets, _err_processor) = create_and_process_bytecode(&options, env);
    let trafo_duration = now.elapsed();
    check_errors(
        env,
        &options,
        error_writer,
        // TODO: add _err_processor to this message
        "exiting with bytecode transformation errors",
    )?;

    // Generate boogie code
    let now = Instant::now();
    let (code_writer, types) = generate_boogie(env, &options, &targets)?;
    let gen_duration = now.elapsed();
    check_errors(
        env,
        &options,
        error_writer,
        "exiting with condition generation errors",
    )?;

    // Verify boogie code.
    let now = Instant::now();
    verify_boogie(env, &options, &targets, code_writer, types)?;
    let verify_duration = now.elapsed();

    // Report durations.
    info!(
        "{:.3}s build, {:.3}s trafo, {:.3}s gen, {:.3}s verify, total {:.3}s",
        build_duration.as_secs_f64(),
        trafo_duration.as_secs_f64(),
        gen_duration.as_secs_f64(),
        verify_duration.as_secs_f64(),
        build_duration.as_secs_f64()
            + trafo_duration.as_secs_f64()
            + gen_duration.as_secs_f64()
            + verify_duration.as_secs_f64()
    );
    check_errors(
        env,
        &options,
        error_writer,
        "exiting with verification errors",
    )?;

    for spec in targets.specs() {
        let fun_env = env.get_function(*spec);
        if targets.is_verified_spec(spec)
            && targets.has_target(
                &fun_env,
                &FunctionVariant::Verification(VerificationFlavor::Regular),
            )
        {
            println!("\x1b[32m✔\x1b[0m {}", fun_env.get_full_name_str());
        }
    }

    Ok(())
}

pub fn check_errors<W: WriteColor>(
    env: &GlobalEnv,
    options: &Options,
    error_writer: &mut W,
    msg: &'static str,
) -> anyhow::Result<()> {
    env.report_diag(error_writer, options.prover.report_severity);
    if env.has_errors() {
        Err(anyhow!(msg))
    } else {
        Ok(())
    }
}

pub fn generate_boogie(
    env: &GlobalEnv,
    options: &Options,
    targets: &FunctionTargetsHolder,
) -> anyhow::Result<(CodeWriter, BiBTreeMap<Type, String>)> {
    let writer = CodeWriter::new(env.internal_loc());
    let types = RefCell::new(BiBTreeMap::new());
    add_prelude(env, &options.backend, &writer)?;
    let mut translator = BoogieTranslator::new(env, &options.backend, targets, &writer, &types);
    translator.translate();
    Ok((writer, types.into_inner()))
}

pub fn verify_boogie(
    env: &GlobalEnv,
    options: &Options,
    targets: &FunctionTargetsHolder,
    writer: CodeWriter,
    types: BiBTreeMap<Type, String>,
) -> anyhow::Result<()> {
    let output_existed = std::path::Path::new(&options.output_path).exists();
    debug!("writing boogie to `{}`", &options.output_path);
    writer.process_result(|result| fs::write(&options.output_path, result))?;
    if !options.prover.generate_only {
        let boogie = BoogieWrapper {
            env,
            targets,
            writer: &writer,
            options: &options.backend,
            types: &types,
        };
        boogie.call_boogie_and_verify_output(&options.output_path)?;
        if !output_existed && !options.backend.keep_artifacts {
            std::fs::remove_file(&options.output_path).unwrap_or_default();
        }
    }
    Ok(())
}

/// Create bytecode and process it.
pub fn create_and_process_bytecode(
    options: &Options,
    env: &GlobalEnv,
) -> (FunctionTargetsHolder, Option<String>) {
    let mut targets = FunctionTargetsHolder::new();
    let output_dir = Path::new(&options.output_path)
        .parent()
        .expect("expect the parent directory of the output path to exist");
    let output_prefix = options.move_sources.first().map_or("bytecode", |s| {
        Path::new(s).file_name().unwrap().to_str().unwrap()
    });

    // Add function targets for all functions in the environment.
    for module_env in env.get_modules() {
        if module_env.is_target() {
            info!("preparing module {}", module_env.get_full_name_str());
        }
        if options.prover.dump_bytecode {
            let dump_file = output_dir.join(format!("{}.mv.disas", output_prefix));
            fs::write(&dump_file, module_env.disassemble()).expect("dumping disassembled module");
        }
        for func_env in module_env.get_functions() {
            targets.add_target(&func_env)
        }
    }

    // Create processing pipeline and run it.
    let pipeline = if options.experimental_pipeline {
        pipeline_factory::experimental_pipeline()
    } else {
        pipeline_factory::default_pipeline_with_options(&options.prover)
    };

    let res = if options.prover.dump_bytecode {
        let dump_file_base = output_dir
            .join(output_prefix)
            .into_os_string()
            .into_string()
            .unwrap();
        pipeline.run_with_dump(env, &mut targets, &dump_file_base, options.prover.dump_cfg)
    } else {
        pipeline.run(env, &mut targets)
    };

    // println!(
    //     "{}",
    //     mono_analysis::MonoInfoCFGDisplay {
    //         info: &mono_analysis::get_info(env),
    //         env
    //     }
    // );

    (targets, res.err().map(|p| p.name()))
}

// Tools using the Move prover top-level driver
// ============================================
/* 
fn run_docgen<W: WriteColor>(
    env: &GlobalEnv,
    options: &Options,
    error_writer: &mut W,
    now: Instant,
) -> anyhow::Result<()> {
    let generator = Docgen::new(env, &options.docgen);
    let checking_elapsed = now.elapsed();
    info!("generating documentation");
    for (file, content) in generator.gen() {
        let path = PathBuf::from(&file);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(path.as_path(), content)?;
    }
    let generating_elapsed = now.elapsed();
    info!(
        "{:.3}s checking, {:.3}s generating",
        checking_elapsed.as_secs_f64(),
        (generating_elapsed - checking_elapsed).as_secs_f64()
    );
    if env.has_errors() {
        env.report_diag(error_writer, options.prover.report_severity);
        Err(anyhow!("exiting with documentation generation errors"))
    } else {
        Ok(())
    }
}
*/

fn run_escape(env: &GlobalEnv, options: &Options, now: Instant) {
    let mut targets = FunctionTargetsHolder::new();
    for module_env in env.get_modules() {
        for func_env in module_env.get_functions() {
            targets.add_target(&func_env)
        }
    }
    println!(
        "Analyzing {} modules, {} declared functions, {} declared structs, {} total bytecodes",
        env.get_module_count(),
        env.get_declared_function_count(),
        env.get_declared_struct_count(),
        env.get_move_bytecode_instruction_count(),
    );
    let mut pipeline = FunctionTargetPipeline::default();
    pipeline.add_processor(EscapeAnalysisProcessor::new());

    let start = now.elapsed();
    pipeline.run(env, &mut targets);
    let end = now.elapsed();

    // print escaped internal refs flagged by analysis. do not report errors in dependencies
    let mut error_writer = Buffer::no_color();
    env.report_diag_with_filter(&mut error_writer, |d| {
        let fname = env.get_file(d.labels[0].file_id).to_str().unwrap();
        options.move_sources.iter().any(|d| {
            let p = Path::new(d);
            if p.is_file() {
                d == fname
            } else {
                Path::new(fname).parent().unwrap() == p
            }
        }) && d.severity >= Severity::Error
    });
    println!("{}", String::from_utf8_lossy(&error_writer.into_inner()));
    info!("in ms, analysis took {:.3}", (end - start).as_millis())
}
