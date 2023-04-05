// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use codespan_reporting::{diagnostic::Severity, term::termcolor::Buffer};
use evm::backend::MemoryVicinity;
use evm_exec_utils::{compile, exec::Executor, tracing};
use move_command_line_common::testing::EXP_EXT;
use move_compiler::shared::{NumericalAddress, PackagePaths};
use move_model::{
    model::{FunId, GlobalEnv, QualifiedId},
    options::ModelBuilderOptions,
    run_model_builder_with_options_and_compilation_flags,
};
use move_prover_test_utils::{baseline_test::verify_or_update_baseline, extract_test_directives};
use move_stdlib::move_stdlib_named_addresses;
use move_to_yul::{generator::Generator, options::Options};
use primitive_types::{H160, U256};
use std::{
    collections::BTreeMap,
    fmt::Write,
    path::{Path, PathBuf},
};

fn test_runner(path: &Path) -> datatest_stable::Result<()> {
    let experiments = extract_test_directives(path, "// experiment:")?;

    let mut sources = extract_test_directives(path, "// dep:")?;
    sources.push(path.to_string_lossy().to_string());
    let deps = vec![
        path_from_crate_root("../stdlib/sources"),
        path_from_crate_root("../../move-stdlib/sources"),
        path_from_crate_root("../../extensions/async/move-async-lib/sources"),
    ];
    let mut named_address_map = move_stdlib_named_addresses();
    named_address_map.insert(
        "std".to_string(),
        NumericalAddress::parse_str("0x1").unwrap(),
    );
    named_address_map.insert(
        "Evm".to_string(),
        NumericalAddress::parse_str("0x2").unwrap(),
    );
    named_address_map.insert(
        "Async".to_string(),
        NumericalAddress::parse_str("0x1").unwrap(),
    );
    let env = run_model_builder_with_options_and_compilation_flags(
        vec![PackagePaths {
            name: None,
            paths: sources,
            named_address_map: named_address_map.clone(),
        }],
        vec![PackagePaths {
            name: None,
            paths: deps,
            named_address_map,
        }],
        ModelBuilderOptions::default(),
        move_compiler::Flags::empty()
            .set_sources_shadow_deps(true)
            .set_flavor("async"),
    )?;
    for exp in std::iter::once(String::new()).chain(experiments.into_iter()) {
        let mut options = Options {
            testing: true,
            ..Options::default()
        };
        let ext = if exp.is_empty() {
            EXP_EXT.to_string()
        } else {
            options.experiments.push(exp.clone());
            format!("{}.{}", EXP_EXT, exp)
        };
        let mut contracts = Generator::run(&options, &env);
        let mut out = "".to_string();
        if !env.has_errors() {
            out = out + &contracts.pop().expect("contract").1;
            out = format!("{}\n\n{}", out, compile_check(&options, &out));

            // Also generate any tests and run them.
            let test_cases = Generator::run_for_evm_tests(&options, &env);
            if !test_cases.is_empty() && !env.has_errors() {
                out = format!("{}\n\n{}", out, run_tests(&env, &test_cases)?)
            }
        }
        let mut error_writer = Buffer::no_color();
        env.report_diag(&mut error_writer, Severity::Help);
        let diag = String::from_utf8_lossy(&error_writer.into_inner()).to_string();
        if !diag.is_empty() {
            out = format!("{}\n\n!! Move-To-Yul Diagnostics:\n {}", out, diag);
        }
        let baseline_path = path.with_extension(ext);
        verify_or_update_baseline(baseline_path.as_path(), &out)?;
    }
    Ok(())
}

fn path_from_crate_root(path: &str) -> String {
    let mut buf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    buf.push(path);
    buf.to_string_lossy().to_string()
}

fn compile_check(_options: &Options, source: &str) -> String {
    match compile::solc_yul(source, false) {
        Ok(_) => "!! Succeeded compiling Yul\n".to_string(),
        Err(msg) => format!("!! Errors compiling Yul\n\n{}", msg),
    }
}

fn run_tests(
    env: &GlobalEnv,
    test_cases: &BTreeMap<QualifiedId<FunId>, String>,
) -> anyhow::Result<String> {
    let mut res = String::new();
    res.push_str("!! Unit tests\n\n");
    for (fun, source) in test_cases {
        writeln!(
            &mut res,
            "// test of {}",
            env.get_function(*fun).get_full_name_str()
        )
        .unwrap();
        res.push_str(source);
        writeln!(
            &mut res,
            "===> Test result of {}: {}\n",
            env.get_function(*fun).get_full_name_str(),
            execute_test(env, source)?
        )?;
    }
    Ok(res)
}

fn execute_test(_env: &GlobalEnv, source: &str) -> anyhow::Result<String> {
    // Compile source
    let (code, _) =
        compile::solc_yul(source, false).with_context(|| format!("Yul source:\n {}", source))?;

    // Create executor.
    let vicinity = MemoryVicinity {
        gas_price: 0.into(),
        origin: H160::zero(),
        chain_id: 0.into(),
        block_hashes: vec![],
        block_number: 0.into(),
        block_coinbase: H160::zero(),
        block_timestamp: 0.into(),
        block_difficulty: 0.into(),
        block_gas_limit: U256::MAX,
        block_base_fee_per_gas: 0.into(),
    };
    let mut exec = Executor::new(&vicinity);
    let res = if std::env::var("EVM_STEP_LISTENER").is_ok() {
        tracing::trace_runtime(|| {
            exec.execute_custom_code(H160::zero(), H160::zero(), code, vec![])
        })
    } else {
        exec.execute_custom_code(H160::zero(), H160::zero(), code, vec![])
    };
    Ok(res.to_string())
}

datatest_stable::harness!(test_runner, "tests", r".*\.move$");
