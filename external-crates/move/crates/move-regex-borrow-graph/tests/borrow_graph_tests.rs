// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_command_line_common::insta_assert;

use move_regex_borrow_graph::tests::graph_file_test_harness::run_borrow_arrangement_test;

fn run_graph_test(file_path: &std::path::Path) -> datatest_stable::Result<()> {
    let msg = match run_borrow_arrangement_test(file_path) {
        Ok(_) => "".to_string(),
        Err(e) => e,
    };
    insta_assert! {
        input_path: file_path,
        contents: msg,
    };
    Ok(())
}

// Hand in each move path
datatest_stable::harness!(run_graph_test, "tests/graph", r"\.bgt$",);
