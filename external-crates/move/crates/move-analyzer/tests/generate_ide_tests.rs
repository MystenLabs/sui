// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, BufWriter},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use move_analyzer::symbols::{get_symbols, DefInfo, Symbols, UseDefMap};
use move_command_line_common::{
    env::read_bool_env_var,
    testing::{add_update_baseline_fix, format_diff, read_env_update_baseline, EXP_EXT, OUT_EXT},
};
use move_compiler::{
    command_line::compiler::move_check_for_errors,
    diagnostics::*,
    editions::{Edition, Flavor},
    linters::{self, LintLevel},
    shared::{Flags, NumericalAddress, PackageConfig, PackagePaths},
    sui_mode, Compiler, PASS_PARSER,
};
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

#[test]
fn generate_macros_test() {
    #[cfg(test)]
    fn assert_use_def(
        mod_symbols: &mut Vec<TestEntry>,
        _symbols: &Option<()>,
        use_idx: usize,
        use_line: u32,
        _use_col: u32,
        _use_file: &str,
        _def_line: u32,
        _def_col: u32,
        _def_file: &str,
        _type_str: &str,
        _type_def: Option<(u32, u32, &str)>,
    ) {
        let def = UseDefTest {
            use_ndx: use_idx,
            use_line,
        };
        mod_symbols.push(TestEntry::UseDefTest(def));
    }

    let mut file_tests = BTreeMap::new();
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    path.push("tests/macros.ide");

    let project = "tests/macros".to_string();

    let file = "macros.move".to_string();

    let mut out = vec![];
    let mod_symbols = &mut out;
    let symbols = None;

    // macro definitions - the signature should be symbolicated including lambda types etc.

    // macro name
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        6,
        14,
        "macros.move",
        6,
        14,
        "macros.move",
        "macro fun Macros::macros::foo($i: u64, $body: |u64| -> u64): u64",
        None,
    );
    // first non-lambda param (primitive type)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        6,
        18,
        "macros.move",
        6,
        18,
        "macros.move",
        "$i: u64",
        None,
    );
    // second lambda param (using primitive types)
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        6,
        27,
        "macros.move",
        6,
        27,
        "macros.move",
        "$body: |u64| -> u64",
        None,
    );

    // macro name
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        14,
        14,
        "macros.move",
        14,
        14,
        "macros.move",
        "macro fun Macros::macros::bar($i: Macros::macros::SomeStruct, $body: |Macros::macros::SomeStruct| -> Macros::macros::SomeStruct): Macros::macros::SomeStruct",
        None,
    );
    // first non-lambda param (struct type)
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        14,
        18,
        "macros.move",
        14,
        18,
        "macros.move",
        "$i: Macros::macros::SomeStruct",
        Some((2, 18, "macros.move")),
    );
    // first non-lambda param type (struct type)
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        14,
        22,
        "macros.move",
        2,
        18,
        "macros.move",
        "public struct Macros::macros::SomeStruct has drop {\n\tsome_field: u64\n}",
        Some((2, 18, "macros.move")),
    );
    // second lambda param (using struct types)
    assert_use_def(
        mod_symbols,
        &symbols,
        3,
        14,
        34,
        "macros.move",
        14,
        34,
        "macros.move",
        "$body: |Macros::macros::SomeStruct| -> Macros::macros::SomeStruct",
        None,
    );
    // lambda param type (struct type)
    assert_use_def(
        mod_symbols,
        &symbols,
        4,
        14,
        42,
        "macros.move",
        2,
        18,
        "macros.move",
        "public struct Macros::macros::SomeStruct has drop {\n\tsome_field: u64\n}",
        Some((2, 18, "macros.move")),
    );
    // lambda param type (struct type)
    assert_use_def(
        mod_symbols,
        &symbols,
        5,
        14,
        57,
        "macros.move",
        2,
        18,
        "macros.move",
        "public struct Macros::macros::SomeStruct has drop {\n\tsome_field: u64\n}",
        Some((2, 18, "macros.move")),
    );
    // macro ret type (struct type)
    assert_use_def(
        mod_symbols,
        &symbols,
        6,
        14,
        70,
        "macros.move",
        2,
        18,
        "macros.move",
        "public struct Macros::macros::SomeStruct has drop {\n\tsome_field: u64\n}",
        Some((2, 18, "macros.move")),
    );

    // macro name
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        18,
        14,
        "macros.move",
        18,
        14,
        "macros.move",
        "macro fun Macros::macros::for_each<$T>($v: &vector<$T>, $body: |&$T| -> ())",
        None,
    );
    // macro's generic type
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        18,
        23,
        "macros.move",
        18,
        23,
        "macros.move",
        "$T",
        None,
    );
    // first non-lambda param (parameterized vec type)
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        18,
        27,
        "macros.move",
        18,
        27,
        "macros.move",
        "let $v: &vector<u64>",
        None,
    );
    // first non-lambda param type's generic type
    assert_use_def(
        mod_symbols,
        &symbols,
        3,
        18,
        39,
        "macros.move",
        18,
        23,
        "macros.move",
        "$T",
        None,
    );
    // second lambda param (using generic types)
    assert_use_def(
        mod_symbols,
        &symbols,
        4,
        18,
        44,
        "macros.move",
        18,
        44,
        "macros.move",
        "$body: |&$T| -> ()",
        None,
    );
    // lambda param type (struct type)
    assert_use_def(
        mod_symbols,
        &symbols,
        5,
        18,
        53,
        "macros.move",
        18,
        23,
        "macros.move",
        "$T",
        None,
    );

    // macro uses

    // module in macro call
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        32,
        16,
        "macros.move",
        0,
        15,
        "macros.move",
        "module Macros::macros",
        None,
    );
    // function name in macro call
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        32,
        24,
        "macros.move",
        6,
        14,
        "macros.move",
        "macro fun Macros::macros::foo($i: u64, $body: |u64| -> u64): u64",
        None,
    );
    // first non-lambda argument in macro call
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        32,
        29,
        "macros.move",
        31,
        12,
        "macros.move",
        "let p: u64",
        None,
    );
    // lambda param in second lambda argument in macro call
    assert_use_def(
        mod_symbols,
        &symbols,
        3,
        32,
        33,
        "macros.move",
        32,
        33,
        "macros.move",
        "let x: u64",
        None,
    );
    // lambda body (its param) in second lambda argument in macro call
    assert_use_def(
        mod_symbols,
        &symbols,
        4,
        32,
        36,
        "macros.move",
        32,
        33,
        "macros.move",
        "let x: u64",
        None,
    );

    // lambda in macro call containing another macro call

    // lambda param
    assert_use_def(
        mod_symbols,
        &symbols,
        5,
        37,
        49,
        "macros.move",
        37,
        49,
        "macros.move",
        "let y: u64",
        None,
    );
    // macro name in macro call in lambda body
    assert_use_def(
        mod_symbols,
        &symbols,
        7,
        37,
        68,
        "macros.move",
        6,
        14,
        "macros.move",
        "macro fun Macros::macros::foo($i: u64, $body: |u64| -> u64): u64",
        None,
    );
    // non-lambda argument nested in macro call in lambda body
    assert_use_def(
        mod_symbols,
        &symbols,
        8,
        37,
        73,
        "macros.move",
        37,
        49,
        "macros.move",
        "let y: u64",
        None,
    );
    // lambda param of lambda argument nested in macro call in lambda body
    assert_use_def(
        mod_symbols,
        &symbols,
        9,
        37,
        77,
        "macros.move",
        37,
        77,
        "macros.move",
        "let z: u64",
        None,
    );
    // lambda body (its param) of lambda argument nested in macro call in lambda body
    assert_use_def(
        mod_symbols,
        &symbols,
        10,
        37,
        80,
        "macros.move",
        37,
        77,
        "macros.move",
        "let z: u64",
        None,
    );

    // part of lambda's body in macro call that represents captured variable
    assert_use_def(
        mod_symbols,
        &symbols,
        4,
        43,
        48,
        "macros.move",
        42,
        16,
        "macros.move",
        "let mut sum: u64",
        None,
    );
    // first macro argument in macro call, receiver-syntax style
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        44,
        8,
        "macros.move",
        41,
        12,
        "macros.move",
        "let es: vector<u64>",
        None,
    );
    // aliased macro name in macro call, receiver-syntax style
    assert_use_def(
        mod_symbols,
        &symbols,
        1,
        44,
        11,
        "macros.move",
        18,
        14,
        "macros.move",
        "macro fun Macros::macros::for_each<$T>($v: &vector<$T>, $body: |&$T| -> ())",
        None,
    );

    // type parameter in macro call
    assert_use_def(
        mod_symbols,
        &symbols,
        2,
        51,
        34,
        "macros.move",
        2,
        18,
        "macros.move",
        "public struct Macros::macros::SomeStruct has drop {\n\tsome_field: u64\n}",
        Some((2, 18, "macros.move")),
    );

    file_tests.insert(file, out);

    let mut out = vec![];
    let mod_symbols = &mut out;
    let file = "fun_type.move".to_string();

    // entry function definition
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        2,
        14,
        "fun_type.move",
        2,
        14,
        "fun_type.move",
        "entry fun Macros::fun_type::entry_fun()",
        None,
    );
    // macro function definition
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        5,
        14,
        "fun_type.move",
        5,
        14,
        "fun_type.move",
        "macro fun Macros::fun_type::macro_fun()",
        None,
    );

    // entry function call
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        9,
        8,
        "fun_type.move",
        2,
        14,
        "fun_type.move",
        "entry fun Macros::fun_type::entry_fun()",
        None,
    );
    // macro function call
    assert_use_def(
        mod_symbols,
        &symbols,
        0,
        10,
        8,
        "fun_type.move",
        5,
        14,
        "fun_type.move",
        "macro fun Macros::fun_type::macro_fun()",
        None,
    );

    file_tests.insert(file, out);

    let value = TestSuite {
        project,
        file_tests,
    };
    let result = serde_json::to_string_pretty(&value).unwrap();
    fs::write(path, result).unwrap();
}
