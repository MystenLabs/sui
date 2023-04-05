// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fs, path::Path};

use move_command_line_common::{
    env::read_bool_env_var,
    testing::{add_update_baseline_fix, format_diff, read_env_update_baseline, EXP_EXT, OUT_EXT},
};
use move_compiler::{
    compiled_unit::AnnotatedCompiledUnit,
    diagnostics::*,
    shared::{Flags, NumericalAddress},
    unit_test, CommentMap, Compiler, SteppedCompiler, PASS_CFGIR, PASS_PARSER,
};

/// Shared flag to keep any temporary results of the test
const KEEP_TMP: &str = "KEEP";

const TEST_EXT: &str = "unit_test";
const VERIFICATION_EXT: &str = "verification";

/// Root of tests which require to set flavor flags.
const FLAVOR_PATH: &str = "flavors/";

fn default_testing_addresses() -> BTreeMap<String, NumericalAddress> {
    let mapping = [
        ("std", "0x1"),
        ("M", "0x1"),
        ("A", "0x42"),
        ("B", "0x42"),
        ("K", "0x19"),
        ("Async", "0x20"),
    ];
    mapping
        .iter()
        .map(|(name, addr)| (name.to_string(), NumericalAddress::parse_str(addr).unwrap()))
        .collect()
}

fn move_check_testsuite(path: &Path) -> datatest_stable::Result<()> {
    // A test is marked that it should also be compiled in test mode by having a `path.unit_test`
    // file.
    if path.with_extension(TEST_EXT).exists() {
        let test_exp_path = format!(
            "{}.unit_test.{}",
            path.with_extension("").to_string_lossy(),
            EXP_EXT
        );
        let test_out_path = format!(
            "{}.unit_test.{}",
            path.with_extension("").to_string_lossy(),
            OUT_EXT
        );
        run_test(
            path,
            Path::new(&test_exp_path),
            Path::new(&test_out_path),
            Flags::testing(),
        )?;
    }

    // A verification case is marked that it should also be compiled in verification mode by having
    // a `path.verification` file.
    if path.with_extension(VERIFICATION_EXT).exists() {
        let verification_exp_path = format!(
            "{}.verification.{}",
            path.with_extension("").to_string_lossy(),
            EXP_EXT
        );
        let verification_out_path = format!(
            "{}.verification.{}",
            path.with_extension("").to_string_lossy(),
            OUT_EXT
        );
        run_test(
            path,
            Path::new(&verification_exp_path),
            Path::new(&verification_out_path),
            Flags::verification(),
        )?;
    }

    let exp_path = path.with_extension(EXP_EXT);
    let out_path = path.with_extension(OUT_EXT);

    let mut flags = Flags::empty();
    match path.to_str() {
        Some(p) if p.contains(FLAVOR_PATH) => {
            // Extract the flavor from the path. Its the directory name of the file.
            let flavor = path
                .parent()
                .expect("has parent")
                .file_name()
                .expect("has name")
                .to_string_lossy()
                .to_string();
            flags = flags.set_flavor(flavor)
        }
        _ => {}
    };
    run_test(path, &exp_path, &out_path, flags)?;
    Ok(())
}

// Runs all tests under the test/testsuite directory.
fn run_test(path: &Path, exp_path: &Path, out_path: &Path, flags: Flags) -> anyhow::Result<()> {
    let targets: Vec<String> = vec![path.to_str().unwrap().to_owned()];

    let (files, comments_and_compiler_res) = Compiler::from_files(
        targets,
        move_stdlib::move_stdlib_files(),
        default_testing_addresses(),
    )
    .set_flags(flags)
    .run::<PASS_PARSER>()?;
    let diags = move_check_for_errors(comments_and_compiler_res);

    let has_diags = !diags.is_empty();
    let diag_buffer = if has_diags {
        move_compiler::diagnostics::report_diagnostics_to_buffer(&files, diags)
    } else {
        vec![]
    };

    let save_diags = read_bool_env_var(KEEP_TMP);
    let update_baseline = read_env_update_baseline();

    let rendered_diags = std::str::from_utf8(&diag_buffer)?;
    if save_diags {
        fs::write(out_path, &diag_buffer)?;
    }

    if update_baseline {
        if has_diags {
            fs::write(exp_path, rendered_diags)?;
        } else if exp_path.is_file() {
            fs::remove_file(exp_path)?;
        }
        return Ok(());
    }

    let exp_exists = exp_path.is_file();
    match (has_diags, exp_exists) {
        (false, false) => Ok(()),
        (true, false) => {
            let msg = format!(
                "Expected success. Unexpected diagnostics:\n{}",
                rendered_diags
            );
            anyhow::bail!(add_update_baseline_fix(msg))
        }
        (false, true) => {
            let msg = format!(
                "Unexpected success. Expected diagnostics:\n{}",
                fs::read_to_string(exp_path)?
            );
            anyhow::bail!(add_update_baseline_fix(msg))
        }
        (true, true) => {
            let expected_diags = fs::read_to_string(exp_path)?;
            if rendered_diags != expected_diags {
                let msg = format!(
                    "Expected diagnostics differ from actual diagnostics:\n{}",
                    format_diff(expected_diags, rendered_diags),
                );
                anyhow::bail!(add_update_baseline_fix(msg))
            } else {
                Ok(())
            }
        }
    }
}

fn move_check_for_errors(
    comments_and_compiler_res: Result<(CommentMap, SteppedCompiler<'_, PASS_PARSER>), Diagnostics>,
) -> Diagnostics {
    fn try_impl(
        comments_and_compiler_res: Result<
            (CommentMap, SteppedCompiler<'_, PASS_PARSER>),
            Diagnostics,
        >,
    ) -> Result<(Vec<AnnotatedCompiledUnit>, Diagnostics), Diagnostics> {
        let (_, compiler) = comments_and_compiler_res?;
        let (mut compiler, cfgir) = compiler.run::<PASS_CFGIR>()?.into_ast();
        let compilation_env = compiler.compilation_env();
        if compilation_env.flags().is_testing() {
            unit_test::plan_builder::construct_test_plan(compilation_env, None, &cfgir);
        }

        let (units, diags) = compiler.at_cfgir(cfgir).build()?;
        Ok((units, diags))
    }

    let (units, inner_diags) = match try_impl(comments_and_compiler_res) {
        Ok((units, inner_diags)) => (units, inner_diags),
        Err(inner_diags) => return inner_diags,
    };
    let mut diags = move_compiler::compiled_unit::verify_units(&units);
    diags.extend(inner_diags);
    diags
}

datatest_stable::harness!(move_check_testsuite, "tests/move_check", r".*\.move$");
