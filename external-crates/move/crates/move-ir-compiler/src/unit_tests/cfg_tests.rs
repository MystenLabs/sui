// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::unit_tests::testutils::compile_module_string;
use move_binary_format::{
    control_flow_graph::{ControlFlowGraph, VMControlFlowGraph},
    file_format::Bytecode,
};

#[test]
fn cfg_compile_script_ret() {
    let text = "
        module 0x42.m { entry foo() {
        label b0:
            return;
        } }
        ";
    let code = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code);
    cfg.display();
    assert_eq!(cfg.blocks().len(), 1);
    assert_eq!(cfg.num_blocks(), 1);
    assert_eq!(cfg.reachable_from(0).len(), 1);
}

#[test]
fn cfg_compile_script_let() {
    let text = "
        module 0x42.m { entry foo() {
            let x: u64;
            let y: u64;
            let z: u64;
        label b0:
            x = 3;
            y = 5;
            z = move(x) + copy(y) * 5 - copy(y);
            return;
        } }
        ";
    let code = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code);
    assert_eq!(cfg.blocks().len(), 1);
    assert_eq!(cfg.num_blocks(), 1);
    assert_eq!(cfg.reachable_from(0).len(), 1);
}

#[test]
fn cfg_compile_if() {
    let text = "
        module 0x42.m { entry foo() {
            let x: u64;
        label b0:
            x = 0;
            jump_if (42 > 0) b2;
        label b1:
            jump b3;
        label b2:
            x = 1;
            jump b3;
        label b3:
            return;
        } }
        ";
    let code = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code);
    assert_eq!(cfg.blocks().len(), 4);
    assert_eq!(cfg.num_blocks(), 4);
    assert_eq!(cfg.reachable_from(0).len(), 4);
}

#[test]
fn cfg_compile_if_else() {
    let text = "
        module 0x42.m { entry foo() {
            let x: u64;
            let y: u64;
        label b0:
            jump_if (42 > 0) b2;
        label b1:
            y = 2;
            x = 1;
            jump b3;
        label b2:
            x = 1;
            y = 2;
            jump b3;
        label b3:
            return;
        } }
        ";
    let code = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code);
    assert_eq!(cfg.blocks().len(), 4);
    assert_eq!(cfg.num_blocks(), 4);
    assert_eq!(cfg.reachable_from(0).len(), 4);
}

#[test]
fn cfg_compile_if_else_with_else_return() {
    let text = "
        module 0x42.m { entry foo() {
            let x: u64;
        label b0:
            jump_if (42 > 0) b2;
        label b1:
            return;
        label b2:
            x = 1;
            jump b3;
        label b3:
            return;
        } }
        ";
    let code = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code);
    assert_eq!(cfg.blocks().len(), 4);
    assert_eq!(cfg.num_blocks(), 4);
    assert_eq!(cfg.reachable_from(0).len(), 4);
}

#[test]
fn cfg_compile_nested_if() {
    let text = "
        module 0x42.m { entry foo() {
            let x: u64;
        label entry:
            jump_if (42 > 0) if_0_then;
        label if_0_else:
            jump_if (5 > 10) if_1_then;
        label if_1_else:
            x = 3;
            jump if_1_cont;
        label if_1_then:
            x = 2;
            jump if_1_cont;
        label if_1_cont:
            jump if_0_cont;
        label if_0_then:
            x = 1;
            jump if_0_cont;
        label if_0_cont:
            return;
        } }
        ";
    let code = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code);
    assert_eq!(cfg.blocks().len(), 7);
    assert_eq!(cfg.num_blocks(), 7);
    assert_eq!(cfg.reachable_from(8).len(), 3);
}

#[test]
fn cfg_compile_if_else_with_if_return() {
    let text = "
        module 0x42.m { entry foo() {
            let x: u64;
        label b0:
            jump_if (42 > 0) b2;
        label b1:
            x = 1;
            jump b3;
        label b2:
            return;
        label b3:
            return;
        } }
        ";
    let code = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code);
    assert_eq!(cfg.blocks().len(), 4);
    assert_eq!(cfg.num_blocks(), 4);
    assert_eq!(cfg.reachable_from(0).len(), 4);
    assert_eq!(cfg.reachable_from(4).len(), 2);
    assert_eq!(cfg.reachable_from(8).len(), 1);
}

#[test]
fn cfg_compile_if_else_with_two_returns() {
    let text = "
        module 0x42.m { entry foo() {
        label b0:
            jump_if (42 > 0) b2;
        label b1:
            return;
        label b2:
            return;
        label b3:
            return;
        } }
        ";
    let code = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code);
    assert_eq!(cfg.blocks().len(), 4);
    assert_eq!(cfg.num_blocks(), 4);
    assert_eq!(cfg.reachable_from(0).len(), 3);
    assert_eq!(cfg.reachable_from(4).len(), 1);
    assert_eq!(cfg.reachable_from(5).len(), 1);
    assert_eq!(cfg.reachable_from(6).len(), 1);
}

#[test]
fn cfg_compile_if_else_with_else_abort() {
    let text = "
        module 0x42.m { entry foo() {
            let x: u64;
        label b0:
            jump_if (42 > 0) b2;
        label b1:
            abort 0;
        label b2:
            x = 1;
            jump b3;
        label b3:
            abort 0;
        } }
        ";
    let code = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code);
    cfg.display();
    assert_eq!(cfg.blocks().len(), 4);
    assert_eq!(cfg.num_blocks(), 4);
    assert_eq!(cfg.reachable_from(0).len(), 4);
}

#[test]
fn cfg_compile_if_else_with_if_abort() {
    let text = "
        module 0x42.m { entry foo() {
            let x: u64;
        label b0:
            jump_if (42 > 0) b2;
        label b1:
            x = 1;
            jump b3;
        label b2:
            abort 0;
        label b3:
            abort 0;
        } }
        ";
    let code = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code);
    cfg.display();
    assert_eq!(cfg.blocks().len(), 4);
    assert_eq!(cfg.num_blocks(), 4);
    assert_eq!(cfg.reachable_from(0).len(), 4);
    assert_eq!(cfg.reachable_from(4).len(), 2);
    assert_eq!(cfg.reachable_from(7).len(), 1);
}

#[test]
fn cfg_compile_if_else_with_two_aborts() {
    let text = "
        module 0x42.m { entry foo() {
        label b0:
            jump_if (42 > 0) b2;
        label b1:
            abort 0;
        label b2:
            abort 0;
        label b3:
            abort 0;
        } }
        ";
    let code = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code);
    cfg.display();
    assert_eq!(cfg.blocks().len(), 4);
    assert_eq!(cfg.num_blocks(), 4);
    assert_eq!(cfg.reachable_from(0).len(), 3);
    assert_eq!(cfg.reachable_from(4).len(), 1);
    assert_eq!(cfg.reachable_from(6).len(), 1);
    assert_eq!(cfg.reachable_from(8).len(), 1);
}

fn compile_module_with_single_function(text: &str) -> Vec<Bytecode> {
    let mut compiled_module = compile_module_string(text).unwrap();
    compiled_module
        .function_defs
        .pop()
        .unwrap()
        .code
        .unwrap()
        .code
}
