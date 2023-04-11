// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::unit_tests::testutils::compile_module_string;
use std::fmt::Write;

fn generate_function(name: &str, num_formals: usize, num_locals: usize) -> String {
    let mut code = format!("public {}(", name);

    code.reserve(30 * (num_formals + num_locals));

    for i in 0..num_formals {
        write!(&mut code, "formal_{}: u64", i).unwrap();
        if i < num_formals - 1 {
            code.push_str(", ");
        }
    }

    code.push_str(") {\n");

    for i in 0..num_locals {
        writeln!(&mut code, "let x_{}: u64;", i).unwrap();
    }
    code.push_str("label b0:\n");
    for i in 0..num_locals {
        writeln!(&mut code, "x_{} = {};", i, i).unwrap();
    }

    code.push_str("return;");

    code.push('}');

    code
}

#[test]
fn compile_module_with_large_frame() {
    let mut code = String::from(
        "
        module 0x16.Foobar {
            struct FooCoin { value: u64 }
        ",
    );

    // Default metering in place, so use reasonable values. This may need to be changed
    // when the metering changes, and gives a useful signal.
    code.push_str(&generate_function("foo_func", 64, 90));

    code.push('}');

    let compiled_module_res = compile_module_string(&code);
    assert!(compiled_module_res.is_ok());
}
