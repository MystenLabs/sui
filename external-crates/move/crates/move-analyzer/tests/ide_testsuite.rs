// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, BufWriter},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use json_comments::StripComments;
use lsp_types::Position;
use move_analyzer::{
    completion::completion_items,
    symbols::{def_info_doc_string, get_symbols, maybe_convert_for_guard, Symbols, UseDefMap},
};
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
    CompletionTest(CompletionTest),
}

#[derive(Serialize, Deserialize)]
struct UseDefTest {
    use_line: u32,
    use_ndx: usize,
}

#[derive(Serialize, Deserialize)]
struct CompletionTest {
    use_line: u32,
    use_col: u32,
}

impl UseDefTest {
    fn test(
        &self,
        test_idx: usize,
        mod_symbols: &UseDefMap,
        symbols: &Symbols,
        output: &mut dyn std::io::Write,
        use_file: &str,
        use_file_path: &Path,
    ) -> anyhow::Result<()> {
        let def_info = &symbols.def_info;
        let UseDefTest { use_ndx, use_line } = self;
        writeln!(output, "-- test {test_idx} -------------------")?;
        writeln!(output, "use line: {use_line}, use_ndx: {use_ndx}")?;
        let lsp_use_line = use_line - 1; // 0th-based
        let Some(uses) = mod_symbols.get(lsp_use_line) else {
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
        let Some(mod_defs) = symbols.file_mods.get(use_file_path) else {
            writeln!(
                output,
                "ERROR: No modules found for file at {use_file_path:?}"
            )?;
            return Ok(());
        };
        // symbols.file_mods only has an entry if there are actual modules in the file
        // (BTreeSet containing module defs is never empty)
        debug_assert!(!mod_defs.is_empty());
        let use_file_hash = mod_defs.first().unwrap().fhash;
        let Some((_, use_file_content)) = symbols.files.get(&use_file_hash) else {
            writeln!(
                output,
                "ERROR: No use file content for file at {use_file_path:?}"
            )?;
            return Ok(());
        };
        let Some((_, def_file_content)) = symbols.files.get(&use_def.def_loc().file_hash()) else {
            writeln!(output, "ERROR: No def file content")?;
            return Ok(());
        };
        use_def.render(
            output,
            symbols,
            lsp_use_line,
            &use_file_content,
            &def_file_content,
        )?;
        let Some(def) = def_info.get(&use_def.def_loc()) else {
            writeln!(output, "ERROR: No def loc found")?;
            return Ok(());
        };

        if let Some(guard_def) = maybe_convert_for_guard(
            def,
            use_file_path,
            &Position::new(lsp_use_line, use_def.col_start()),
            symbols,
        ) {
            writeln!(
                output,
                "On Hover:\n{}",
                if let Some(s) = def_info_doc_string(&guard_def) {
                    format!("{}\n\n{}", guard_def, s)
                } else {
                    format!("{}", guard_def)
                }
            )?;
        } else {
            writeln!(
                output,
                "On Hover:\n{}",
                if let Some(s) = def_info_doc_string(def) {
                    format!("{}\n\n{}", def, s)
                } else {
                    format!("{}", def)
                }
            )?;
        };
        Ok(())
    }
}

impl CompletionTest {
    fn test(
        &self,
        test_idx: usize,
        symbols: &Symbols,
        output: &mut dyn std::io::Write,
        use_file_path: &Path,
    ) -> anyhow::Result<()> {
        let lsp_use_line = self.use_line - 1; // 0th-based
        let use_pos = Position {
            line: lsp_use_line,
            character: self.use_col,
        };
        let items = completion_items(use_pos, use_file_path, symbols);
        writeln!(output, "-- test {test_idx} -------------------")?;
        writeln!(
            output,
            "use line: {}, use_col: {}",
            self.use_line, self.use_col
        )?;
        for i in items {
            writeln!(output, "{:?} '{}'", i.kind.unwrap(), i.label)?;
            writeln!(output, "\tINSERT TEXT: '{}'", i.insert_text.unwrap())?;
            if let Some(label_details) = i.label_details {
                if let Some(detail) = label_details.detail {
                    writeln!(output, "\tTARGET     : '{}'", detail.trim())?;
                }
                if let Some(description) = label_details.description {
                    writeln!(output, "\tTYPE       : '{description}'")?;
                }
            }
        }
        writeln!(output)?;
        Ok(())
    }
}

fn check_expected(expected_path: &Path, result: &str) -> anyhow::Result<()> {
    let update_baseline = read_env_update_baseline();

    if update_baseline {
        fs::write(expected_path, result)?;
        Ok(())
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
    let stripped = StripComments::new(suite_file);
    let suite: TestSuite = serde_json::from_reader(stripped)?;

    let base_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut project_path = base_path.clone();
    project_path.push(suite.project);

    let ide_files_layer: VfsPath = MemoryFS::new().into();
    let (symbols_opt, _) = get_symbols(
        Arc::new(Mutex::new(BTreeMap::new())),
        ide_files_layer,
        project_path.as_path(),
        LintLevel::None,
    )?;
    let symbols = symbols_opt.ok_or("DID NOT FIND SYMBOLS")?;

    let mut output: BufWriter<_> = BufWriter::new(Vec::new());
    let writer: &mut dyn io::Write = output.get_mut();

    for (file, tests) in suite.file_tests {
        writeln!(
            writer,
            "== {file} ========================================================"
        )?;

        let mut fpath = project_path.clone();

        fpath.push(format!("sources/{file}"));
        let cpath = dunce::canonicalize(&fpath).unwrap();
        let mod_symbols = symbols
            .file_use_defs
            .get(&cpath)
            .ok_or(format!("NO SYMBOLS FOR {}", cpath.to_str().unwrap()))?;

        for (idx, test) in tests.iter().enumerate() {
            match test {
                TestEntry::UseDefTest(use_def_test) => {
                    use_def_test.test(idx, mod_symbols, &symbols, writer, &file, &cpath)?
                }
                TestEntry::CompletionTest(completion_test) => {
                    completion_test.test(idx, &symbols, writer, &cpath)?;
                }
            };
            writeln!(writer)?;
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
