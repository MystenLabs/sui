// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, BufWriter},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use move_analyzer::symbols::{get_symbols, Symbols, UseDefMap};
use move_command_line_common::testing::{
    add_update_baseline_fix, format_diff, read_env_update_baseline, EXP_EXT,
};
use move_compiler::linters::LintLevel;
use serde::{Deserialize, Serialize};
use vfs::{MemoryFS, VfsPath};

#[derive(Serialize, Deserialize)]
struct TestSuite {
    project: String,
    file_tests: BTreeMap<String, Vec<TestEntry>>,
}

#[derive(Serialize, Deserialize)]
enum TestEntry {
    UseDefTest(UseDefTest),
}

#[derive(Serialize, Deserialize)]
struct UseDefTest {
    use_line: u32,
    use_ndx: usize,
}

impl UseDefTest {
    fn test(
        &self,
        mod_symbols: &UseDefMap,
        symbols: &Symbols,
        output: &mut dyn std::io::Write,
        use_file: &str,
        ndx: usize,
    ) -> anyhow::Result<()> {
        // let file_name_mapping = &symbols.file_name_mapping;
        let def_info = &symbols.def_info;
        let UseDefTest { use_ndx, use_line } = self;
        writeln!(output, "-- test {ndx} -------------------")?;
        writeln!(output, "use line: {use_line}, use_ndx: {use_ndx}")?;
        let Some(uses) = mod_symbols.get(*use_line) else {
            writeln!(
                output,
                "ERROR: No use_line {use_line} in mod_symbols {mod_symbols:#?} for file {use_file}"
            )?;
            return Ok(());
        };
        let Some(use_def) = uses.iter().nth(*use_ndx) else {
            writeln!(
                output,
                "ERROR: No use_line {use_ndx} in uses {uses:#?} for file {use_file}"
            )?;
            return Ok(());
        };
        writeln!(output, "Use Def: {}", use_def)?;
        let Some(def) = def_info.get(&use_def.def_loc()) else {
            writeln!(output, "ERROR: No def loc found")?;
            return Ok(());
        };
        writeln!(output, "Def Info: {}", def)?;
        Ok(())
    }
}

fn check_expected(expected_path: &Path, result: &str) -> anyhow::Result<()> {
    let update_baseline = read_env_update_baseline();

    if update_baseline {
        fs::write(expected_path, result)?;
        return Ok(());
    } else {
        let exp_exists = expected_path.is_file();
        if exp_exists {
            let expected = fs::read_to_string(expected_path)?;
            if result != expected {
                let msg = format!(
                    "Expected output differ from actual output:\n{}",
                    format_diff(result, expected),
                );
                anyhow::bail!(add_update_baseline_fix(msg))
            } else {
                Ok(())
            }
        } else {
            anyhow::bail!(add_update_baseline_fix("No baseline file found."))
        }
    }
}

fn move_ide_testsuite(test_path: &Path) -> datatest_stable::Result<()> {
    let suite_file = io::BufReader::new(File::open(test_path)?);
    let suite: TestSuite = serde_json::from_reader(suite_file)?;

    let base_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut project_path = base_path.clone();
    project_path.push(suite.project);

    let ide_files_layer: VfsPath = MemoryFS::new().into();
    let (symbols_opt, _) = get_symbols(
        Arc::new(Mutex::new(BTreeMap::new())),
        ide_files_layer,
        &project_path.as_path(),
        LintLevel::None,
    )?;
    let symbols = symbols_opt.ok_or("DID NOT FIND SYMBOLS")?;

    let mut output: BufWriter<_> = BufWriter::new(Vec::new());
    let writer: &mut dyn io::Write = output.get_mut();

    for (file, tests) in suite.file_tests {
        writeln!(writer, "== {file} ========================================================")?;

        let mut fpath = project_path.clone();

        let mut ndx = 0;

        fpath.push(format!("sources/{file}"));
        let cpath = dunce::canonicalize(&fpath).unwrap();
        let mod_symbols = symbols
            .file_use_defs
            .get(&cpath)
            .ok_or(format!("NO SYMBOLS FOR {}", cpath.to_str().unwrap()))?;

        for test in tests.iter() {
            match test {
                TestEntry::UseDefTest(use_def_test) => {
                    use_def_test.test(mod_symbols, &symbols, writer, &file, ndx)?
                }
            };
            ndx = ndx + 1;
            writeln!(writer, "")?;
        }
    }

    let exp_string = test_path
        .with_extension(EXP_EXT)
        .to_string_lossy()
        .to_string();
    let exp_path = Path::new(&exp_string);
    let result: String = String::from_utf8(output.into_inner().unwrap()).unwrap();

    check_expected(exp_path, &result)?;
    Ok(())
}

datatest_stable::harness!(move_ide_testsuite, "tests/", r".*\.ide$");
