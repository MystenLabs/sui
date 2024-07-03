// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::unit_tests::testutils::compile_module_string;
use move_abstract_interpreter::control_flow_graph::{ControlFlowGraph, VMControlFlowGraph};
use move_binary_format::file_format::{Bytecode, VariantJumpTable};

#[test]
fn cfg_compile_script_ret() {
    let text = "
        module 0x42.m { entry foo() {
        label b0:
            return;
        } }
        ";
    let (code, jump_tables) = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code, &jump_tables);
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
    let (code, jump_tables) = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code, &jump_tables);
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
    let (code, jump_tables) = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code, &jump_tables);
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
    let (code, jump_tables) = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code, &jump_tables);
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
    let (code, jump_tables) = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code, &jump_tables);
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
    let (code, jump_tables) = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code, &jump_tables);
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
    let (code, jump_tables) = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code, &jump_tables);
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
    let (code, jump_tables) = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code, &jump_tables);
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
    let (code, jump_tables) = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code, &jump_tables);
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
    let (code, jump_tables) = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code, &jump_tables);
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
    let (code, jump_tables) = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code, &jump_tables);
    cfg.display();
    assert_eq!(cfg.blocks().len(), 4);
    assert_eq!(cfg.num_blocks(), 4);
    assert_eq!(cfg.reachable_from(0).len(), 3);
    assert_eq!(cfg.reachable_from(4).len(), 1);
    assert_eq!(cfg.reachable_from(6).len(), 1);
    assert_eq!(cfg.reachable_from(8).len(), 1);
}

#[test]
fn cfg_compile_variant_switch_simple() {
    let text = "
        module 0x42.m { 
            enum X has drop { V1 { x: u64 }, V2 { } }

            entry foo(x: Self.X) {
                let y: u64;
            label bv:
                variant_switch X (&x) {
                    V1 : b0,
                    V2 : b1,
                };
            label b0:
                return;
            label b1:
                return;
            } 
        }
        ";
    let (code, jump_tables) = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code, &jump_tables);
    assert_eq!(cfg.blocks().len(), 3);
    assert_eq!(cfg.num_blocks(), 3);
    assert_eq!(cfg.reachable_from(0).len(), 3);
}

#[test]
fn cfg_compile_variant_switch_simple_unconditional_jump() {
    let text = "
        module 0x42.m { 
            enum X has drop { V1 { x: u64 }, V2 { } }

            entry foo(x: Self.X) {
                let y: u64;
            label bv:
                variant_switch X (&x) {
                    V1 : b0,
                    V2 : b1,
                };
            // This block is unreachable because `variant_switch` is an unconditional jump.
            label fallthrough: 
                return;
            label b0:
                return;
            label b1:
                return;
            } 
        }
        ";
    let (code, jump_tables) = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code, &jump_tables);
    assert_eq!(cfg.blocks().len(), 4);
    assert_eq!(cfg.num_blocks(), 4);
    assert_eq!(cfg.reachable_from(0).len(), 3);
}

#[test]
fn cfg_compile_variant_switch() {
    let text = "
        module 0x42.m { 
            enum X { V1 { x: u64 }, V2 { } }

            entry foo(x: Self.X) {
                let y: u64;
            label bv:
                variant_switch X (&x) {
                    V1 : b0,
                    V2 : b4,
                };
            label b0:
                X.V1 { x: y } = move(x);
                jump_if (move(y) > 42) b2;
            label b1:
                jump b3;
            label b2:
                y = 0;
                jump b3;
            label b3:
                return;
            label b4: 
                X.V2 {} = move(x);
                jump b3;
            } 
        }
        ";
    let (code, jump_tables) = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code, &jump_tables);
    assert_eq!(cfg.blocks().len(), 6);
    assert_eq!(cfg.num_blocks(), 6);
    assert_eq!(cfg.reachable_from(0).len(), 6);
}

#[test]
fn cfg_compile_variant_switch_with_two_aborts() {
    let text = "
        module 0x42.m { 
            enum X { V1 { x: u64 }, V2 { } }

            entry foo(x: Self.X) {
                let y: u64;
            label bv:
                variant_switch X (&x) {
                    V1 : b0,
                    V2 : b4,
                };
            label b0:
                X.V1 { x: y } = move(x);
                jump_if (move(y) > 42) b3;
            label b1:
                jump b3;
            label b2: // This block is not reachable -- so should have 6 blocks and 5 reachable
                y = 0;
                jump b3;
            label b3:
                return;
            label b4: 
                X.V2 {} = move(x);
                abort 0;
            } 
        }
        ";
    let (code, jump_tables) = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code, &jump_tables);
    assert_eq!(cfg.blocks().len(), 6);
    assert_eq!(cfg.num_blocks(), 6);
    assert_eq!(cfg.reachable_from(0).len(), 5);
}

#[test]
fn cfg_compile_variant_switch_with_return() {
    let text = "
        module 0x42.m { 
            enum X { V1 { x: u64 }, V2 { } }

            entry foo(x: Self.X) {
                let y: u64;
            label bv:
                variant_switch X (&x) {
                    V1 : b0,
                    V2 : b4,
                };
            // This block is unreachable since `variant_switch` is an unconditional jump.
            // If `variant_switch` is a conditional jump, then this block is reachable, and would
            // raise an unused value without drop error. But, since we guarantee exhaustiveness, we
            // are guaranteed that we cannot fall through here and so this block is unreachable.
            label fallthrough:
                return;
            label b0:
                X.V1 { x: y } = move(x);
                jump_if (move(y) > 42) b3;
            label b1:
                return;
            label b2: // This block is not reachable -- so should have 6 blocks and 5 reachable
                y = 0;
                jump b3;
            label b3:
                return;
            label b4: 
                X.V2 {} = move(x);
                abort 0;
            } 
        }
        ";
    let (code, jump_tables) = compile_module_with_single_function(text);
    let cfg: VMControlFlowGraph = VMControlFlowGraph::new(&code, &jump_tables);
    assert_eq!(cfg.blocks().len(), 7);
    assert_eq!(cfg.num_blocks(), 7);
    assert_eq!(cfg.reachable_from(0).len(), 5);
}

fn compile_module_with_single_function(text: &str) -> (Vec<Bytecode>, Vec<VariantJumpTable>) {
    let mut compiled_module = compile_module_string(text).unwrap();
    let code_unit = compiled_module.function_defs.pop().unwrap().code.unwrap();
    (code_unit.code, code_unit.jump_tables)
}
