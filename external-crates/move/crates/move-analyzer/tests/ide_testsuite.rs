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
use lsp_types::{InlayHintKind, InlayHintLabel, InlayHintTooltip, Position};
use move_analyzer::{
    completion::completion_items,
    inlay_hints::inlay_hints_internal,
    symbols::{
        def_info_doc_string, get_symbols, maybe_convert_for_guard, PrecompiledPkgDeps, Symbols,
        UseDefMap,
    },
};
use move_command_line_common::testing::{
    add_update_baseline_fix, format_diff, read_env_update_baseline, EXP_EXT,
};
use move_compiler::linters::LintLevel;
use serde::{Deserialize, Serialize};
use vfs::{MemoryFS, VfsPath};

//**************************************************************************************************
// Test Suites
//**************************************************************************************************

#[derive(Serialize, Deserialize)]
enum TestSuite {
    UseDef {
        project: String,
        file_tests: BTreeMap<String, Vec<UseDefTest>>,
    },
    Completion {
        project: String,
        file_tests: BTreeMap<String, Vec<CompletionTest>>,
    },
    Cursor {
        project: String,
        file_tests: BTreeMap<String, Vec<CursorTest>>,
    },
    Hint {
        project: String,
        file_tests: BTreeMap<String, Vec<HintTest>>,
    },
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

#[derive(Serialize, Deserialize)]
struct CursorTest {
    line: u32,
    character: u32,
    description: String,
}

#[derive(Serialize, Deserialize)]
struct HintTest {
    use_line: u32,
    use_col: u32,
}

//**************************************************************************************************
// Test Impls
//**************************************************************************************************
// These do the actual testing work.

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
        ide_files_root: VfsPath,
        pkg_dependencies: Arc<Mutex<BTreeMap<PathBuf, PrecompiledPkgDeps>>>,
        symbols: &Symbols,
        output: &mut dyn std::io::Write,
        use_file_path: &Path,
    ) -> anyhow::Result<()> {
        let lsp_use_line = self.use_line - 1; // 0th-based
        let lsp_use_col = self.use_col - 1; // 0th-based
        let use_pos = Position {
            line: lsp_use_line,
            character: lsp_use_col,
        };
        let items = completion_items(
            symbols,
            ide_files_root,
            pkg_dependencies,
            use_file_path,
            use_pos,
        );
        writeln!(output, "-- test {test_idx} -------------------")?;
        writeln!(
            output,
            "use line: {}, use_col: {}",
            self.use_line, self.use_col
        )?;
        for i in items {
            writeln!(output, "{:?} '{}'", i.kind.unwrap(), i.label)?;
            writeln!(output, "    INSERT TEXT: '{}'", i.insert_text.unwrap())?;
            if let Some(label_details) = i.label_details {
                if let Some(detail) = label_details.detail {
                    writeln!(output, "    TARGET     : '{}'", detail.trim())?;
                }
                if let Some(description) = label_details.description {
                    writeln!(output, "    TYPE       : '{description}'")?;
                }
            }
        }
        writeln!(output)?;
        Ok(())
    }
}

impl CursorTest {
    fn test(
        &self,
        test_ndx: usize,
        ide_files_root: VfsPath,
        pkg_dependencies: Arc<Mutex<BTreeMap<PathBuf, PrecompiledPkgDeps>>>,
        output: &mut dyn std::io::Write,
        path: &Path,
    ) -> anyhow::Result<()> {
        let CursorTest {
            line,
            character,
            description,
        } = self;
        let line = line - 1; // 0th-based
        let character = character - 1; // 0th-based

        let (symbols_opt, _) = get_symbols(
            pkg_dependencies,
            ide_files_root,
            path,
            LintLevel::None,
            Some((&path.to_path_buf(), Position { line, character })),
        )?;
        let symbols = symbols_opt.unwrap();

        writeln!(
            output,
            "-- test {test_ndx} @ {line}:{character} ------------"
        )?;
        writeln!(output, "expected: {description}")?;
        writeln!(output, "{}", symbols.cursor_context.unwrap())?;
        Ok(())
    }
}

impl HintTest {
    fn test(
        &self,
        test_idx: usize,
        symbols: &Symbols,
        output: &mut dyn std::io::Write,
        use_file_path: &Path,
    ) -> anyhow::Result<()> {
        let inlay_hints = inlay_hints_internal(
            symbols,
            use_file_path.to_path_buf(),
            /* type_hints */ true,
            /* param_hints */ true,
        )
        .unwrap();
        let lsp_line = self.use_line - 1; // 0th-based
        let lsp_col = self.use_col - 1; // 0th-based

        writeln!(output, "-- test {test_idx} -------------------")?;
        let Some((hint, label_parts)) = inlay_hints.iter().find_map(|h| {
            if h.position.line == lsp_line && h.position.character == lsp_col {
                if let InlayHintLabel::LabelParts(parts) = &h.label {
                    return Some((h, parts));
                }
            }
            None
        }) else {
            writeln!(output, "NO INLAY HINT FOUND")?;
            return Ok(());
        };

        let tooltip = hint.tooltip.as_ref().map(|tip| match tip {
            InlayHintTooltip::String(s) => s.clone(),
            InlayHintTooltip::MarkupContent(m) => m.value.clone(),
        });

        match hint.kind {
            Some(InlayHintKind::TYPE) => {
                writeln!(output, "INLAY TYPE HINT : {}", label_parts[1].value)?
            }
            Some(InlayHintKind::PARAMETER) => {
                writeln!(output, "INLAY PARAM HINT: {}", label_parts[0].value)?
            }
            _ => writeln!(output, "INLAY HINT OF UNKNOWN TYPE")?,
        }
        if let Some(tip) = tooltip {
            writeln!(output, "ON HOVER:\n{}", tip)?;
        }

        Ok(())
    }
}

//**************************************************************************************************
// Test Suite Runner Code
//**************************************************************************************************

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

fn initial_symbols(
    project: String,
) -> datatest_stable::Result<(
    PathBuf,
    VfsPath,
    Arc<Mutex<BTreeMap<PathBuf, PrecompiledPkgDeps>>>,
    Symbols,
)> {
    let base_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut project_path = base_path.clone();
    project_path.push(project);

    let ide_files_root: VfsPath = MemoryFS::new().into();
    let pkg_deps = Arc::new(Mutex::new(BTreeMap::new()));
    let (symbols_opt, _) = get_symbols(
        pkg_deps.clone(),
        ide_files_root.clone(),
        project_path.as_path(),
        LintLevel::None,
        None,
    )?;
    let symbols = symbols_opt.ok_or("DID NOT FIND SYMBOLS")?;
    Ok((project_path, ide_files_root, pkg_deps, symbols))
}

fn use_def_test_suite(
    project: String,
    file_tests: BTreeMap<String, Vec<UseDefTest>>,
) -> datatest_stable::Result<String> {
    let (project_path, _, _, symbols) = initial_symbols(project)?;

    let mut output: BufWriter<_> = BufWriter::new(Vec::new());
    let writer: &mut dyn io::Write = output.get_mut();

    for (file, tests) in file_tests {
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
            test.test(idx, mod_symbols, &symbols, writer, &file, &cpath)?;
            writeln!(writer)?;
        }
    }

    let result: String = String::from_utf8(output.into_inner().unwrap()).unwrap();
    Ok(result)
}

fn completion_test_suite(
    project: String,
    file_tests: BTreeMap<String, Vec<CompletionTest>>,
) -> datatest_stable::Result<String> {
    let (project_path, ide_files_root, pkg_deps, symbols) = initial_symbols(project)?;

    let mut output: BufWriter<_> = BufWriter::new(Vec::new());
    let writer: &mut dyn io::Write = output.get_mut();

    for (file, tests) in file_tests {
        writeln!(
            writer,
            "== {file} ========================================================"
        )?;

        let mut fpath = project_path.clone();

        fpath.push(format!("sources/{file}"));
        let cpath = dunce::canonicalize(&fpath).unwrap();

        for (idx, test) in tests.iter().enumerate() {
            test.test(
                idx,
                ide_files_root.clone(),
                pkg_deps.clone(),
                &symbols,
                writer,
                &cpath,
            )?;
        }
    }

    let result: String = String::from_utf8(output.into_inner().unwrap()).unwrap();
    Ok(result)
}

fn cursor_test_suite(
    project: String,
    file_tests: BTreeMap<String, Vec<CursorTest>>,
) -> datatest_stable::Result<String> {
    let (project_path, ide_files_root, pkg_deps, _) = initial_symbols(project)?;

    let mut output: BufWriter<_> = BufWriter::new(Vec::new());
    let writer: &mut dyn io::Write = output.get_mut();

    for (file, tests) in file_tests {
        writeln!(
            writer,
            "== {file} ========================================================"
        )?;

        let mut fpath = project_path.clone();

        fpath.push(format!("sources/{file}"));
        let cpath = dunce::canonicalize(&fpath).unwrap();
        for (idx, test) in tests.iter().enumerate() {
            test.test(
                idx,
                ide_files_root.clone(),
                pkg_deps.clone(),
                writer,
                &cpath,
            )?;
        }
    }

    let result: String = String::from_utf8(output.into_inner().unwrap()).unwrap();
    Ok(result)
}

fn hint_test_suite(
    project: String,
    file_tests: BTreeMap<String, Vec<HintTest>>,
) -> datatest_stable::Result<String> {
    let (project_path, _, _, symbols) = initial_symbols(project)?;

    let mut output: BufWriter<_> = BufWriter::new(Vec::new());
    let writer: &mut dyn io::Write = output.get_mut();

    for (file, tests) in file_tests {
        writeln!(
            writer,
            "== {file} ========================================================"
        )?;

        let mut fpath = project_path.clone();

        fpath.push(format!("sources/{file}"));
        let cpath = dunce::canonicalize(&fpath).unwrap();

        for (idx, test) in tests.iter().enumerate() {
            test.test(idx, &symbols, writer, &cpath)?;
        }
    }

    let result: String = String::from_utf8(output.into_inner().unwrap()).unwrap();
    Ok(result)
}

fn move_ide_testsuite(test_path: &Path) -> datatest_stable::Result<()> {
    let suite_file = io::BufReader::new(File::open(test_path)?);
    let stripped = StripComments::new(suite_file);
    let suite: TestSuite = serde_json::from_reader(stripped)?;

    let output = match suite {
        TestSuite::UseDef {
            project,
            file_tests,
        } => use_def_test_suite(project, file_tests),
        TestSuite::Completion {
            project,
            file_tests,
        } => completion_test_suite(project, file_tests),
        TestSuite::Cursor {
            project,
            file_tests,
        } => cursor_test_suite(project, file_tests),
        TestSuite::Hint {
            project,
            file_tests,
        } => hint_test_suite(project, file_tests),
    }?;

    let exp_string = test_path
        .with_extension(EXP_EXT)
        .to_string_lossy()
        .to_string();
    let exp_path = Path::new(&exp_string);

    check_expected(exp_path, &output)?;
    Ok(())
}

datatest_stable::harness!(move_ide_testsuite, "tests/", r".*\.ide$");

/// Generates cursor tests as json -- useful for making a new batch of tests. Update this list,
/// set `harness = true` for this file in `Cargo.toml`,
/// and run `cargo nextest run generate_cursor_tests`.
#[allow(unused)]
#[test]
fn generate_cursor_test() {
    let posn_pairs = vec![
        (5, 23, "at a struct definition name"),
        (6, 13, "at a struct field name"),
        (6, 19, "in a struct definition at nowhere in particular"),
        (6, 20, "at a struct field type"),
        (6, 21, "at a struct field type"),
        (6, 22, "at a struct field type"),
        (6, 23, "in a struct definition at nowhere in particular"),
        (17, 45, "at a function return type"),
        (28, 14, "at a function definition at nowhere in particular"),
        (28, 14, "at a function definition name"),
        (28, 15, "at a function definition name"),
        (28, 16, "at a function definition name"),
        (28, 17, "at a function definition name"),
        (28, 18, "at a function definition at nowhere in particular"),
        (28, 19, "at a function definition at nowhere in particular"),
        (31, 5, "inside a function body -- unknown"),
        (31, 9, "let binding"),
        (31, 19, "binding in the lhs of a let-binding"),
        (31, 50, "name in module access"),
        (33, 29, "name in a dot call"),
        (33, 34, "equal sign in a binop"),
    ];

    let tests = posn_pairs
        .into_iter()
        .map(|(line, character, description)| CursorTest {
            line,
            character,
            description: description.to_string(),
        })
        .collect::<Vec<_>>();
    let test = TestSuite::Cursor {
        project: "tests/move-2024".to_string(),
        file_tests: BTreeMap::from([("dot_call.move".to_string(), tests)]),
    };

    let string = format!("{}\n", serde_json::to_string_pretty(&test).unwrap());
    let _ = fs::write("cursor_dot_call_tests.ide", string);
}
