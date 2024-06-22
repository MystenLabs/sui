// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_analyzer::symbols::get_symbols;
use move_command_line_common::testing::{
    add_update_baseline_fix, format_diff, read_env_update_baseline, EXP_EXT,
};
use move_compiler::linters::LintLevel;

use lsp_types::Position;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, BufWriter},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use vfs::{MemoryFS, VfsPath};

#[derive(Serialize, Deserialize)]
struct CursorTest {
    directory: String,
    file: String,
    line: u32,
    character: u32,
    description: String,
}

impl CursorTest {
    fn test(&self, output: &mut dyn std::io::Write) -> anyhow::Result<()> {
        let CursorTest {
            directory,
            file,
            line,
            character,
            description,
        } = self;

        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

        path.push(directory);

        let mut fpath = path.clone();
        fpath.push(file);
        let cursor_path = dunce::canonicalize(&fpath).unwrap();

        let ide_files_layer: VfsPath = MemoryFS::new().into();
        let (symbols_opt, _) = get_symbols(
            Arc::new(Mutex::new(BTreeMap::new())),
            ide_files_layer.clone(),
            path.as_path(),
            LintLevel::None,
            Some((
                &cursor_path,
                Position {
                    line: *line,
                    character: *character,
                },
            )),
        )?;
        let symbols = symbols_opt.unwrap();

        writeln!(output, "-- {line}:{character} ------------")?;
        writeln!(output, "expected: {description}")?;
        writeln!(output, "{}", symbols.cursor_context.unwrap())?;
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

fn cursor_testsuite(test_path: &Path) -> datatest_stable::Result<()> {
    let cursor_file = io::BufReader::new(File::open(test_path)?);
    let cursor_test: CursorTest = serde_json::from_reader(cursor_file)?;

    let mut output: BufWriter<_> = BufWriter::new(Vec::new());
    let writer: &mut dyn io::Write = output.get_mut();
    cursor_test.test(writer)?;

    let exp_string = test_path
        .with_extension(EXP_EXT)
        .to_string_lossy()
        .to_string();
    let exp_path = Path::new(&exp_string);
    let result: String = String::from_utf8(output.into_inner().unwrap()).unwrap();

    check_expected(exp_path, &result)?;

    Ok(())
}

datatest_stable::harness!(cursor_testsuite, "tests/cursor_tests/", r".*\.json$");

/// Generates cursor tests as json -- useful for making a new batch of tests. Update this list,
/// set `harness = true` for this file in `Cargo.toml`, and run `cnr generate_cursor_tests`.
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

    for (line, character, description) in posn_pairs {
        let test = CursorTest {
            directory: "tests/move-2024".to_string(),
            file: "sources/dot_call.move".to_string(),
            line,
            character,
            description: description.to_string(),
        };
        let path_string = format!("dot_call_{line}_{character}.json");
        let file_path = Path::new(&path_string);
        let string = format!("{}\n", serde_json::to_string_pretty(&test).unwrap());
        let _ = fs::write(file_path, string);
    }
}
