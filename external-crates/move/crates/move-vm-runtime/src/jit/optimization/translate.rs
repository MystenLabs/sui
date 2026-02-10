// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{jit::optimization::ast, validation::verification::ast as Input};
use move_abstract_interpreter::control_flow_graph::{ControlFlowGraph, VMControlFlowGraph};
use move_binary_format::file_format::{self as FF, FunctionDefinition, FunctionDefinitionIndex};
use move_core_types::gas_algebra::AbstractMemorySize;
use std::collections::BTreeMap;

/// Accumulated fixed gas costs for a basic block.
#[derive(Default)]
struct BlockGasCost {
    instructions: u64,
    pushes: u64,
    pops: u64,
}

impl BlockGasCost {
    fn has_fixed_costs(&self) -> bool {
        self.instructions > 0
    }

    fn add(&mut self, pops: u64, pushes: u64, _pop_size: AbstractMemorySize, _push_size: AbstractMemorySize) {
        self.instructions += 1;
        self.pushes += pushes;
        self.pops += pops;
    }
}

/// Size constants for gas computation (matching the gas meter)
const REFERENCE_SIZE: AbstractMemorySize = AbstractMemorySize::new(8);
const BOOL_SIZE: AbstractMemorySize = AbstractMemorySize::new(1);
const U8_SIZE: AbstractMemorySize = AbstractMemorySize::new(1);
const U16_SIZE: AbstractMemorySize = AbstractMemorySize::new(2);
const U32_SIZE: AbstractMemorySize = AbstractMemorySize::new(4);
const U64_SIZE: AbstractMemorySize = AbstractMemorySize::new(8);
const U128_SIZE: AbstractMemorySize = AbstractMemorySize::new(16);
const U256_SIZE: AbstractMemorySize = AbstractMemorySize::new(32);

/// Returns Some((pops, pushes, pop_size, push_size)) for fixed-cost instructions,
/// None for variable-cost instructions that need runtime charging.
fn get_fixed_instruction_cost(instr: &ast::Bytecode) -> Option<(u64, u64, AbstractMemorySize, AbstractMemorySize)> {
    use ast::Bytecode::*;
    match instr {
        // No-op instructions
        Nop | Ret => Some((0, 0, AbstractMemorySize::zero(), AbstractMemorySize::zero())),

        // Branch instructions
        BrTrue(_) | BrFalse(_) => Some((1, 0, BOOL_SIZE, AbstractMemorySize::zero())),
        Branch(_) => Some((0, 0, AbstractMemorySize::zero(), AbstractMemorySize::zero())),

        // Load integer constants
        LdU8(_) => Some((0, 1, AbstractMemorySize::zero(), U8_SIZE)),
        LdU16(_) => Some((0, 1, AbstractMemorySize::zero(), U16_SIZE)),
        LdU32(_) => Some((0, 1, AbstractMemorySize::zero(), U32_SIZE)),
        LdU64(_) => Some((0, 1, AbstractMemorySize::zero(), U64_SIZE)),
        LdU128(_) => Some((0, 1, AbstractMemorySize::zero(), U128_SIZE)),
        LdU256(_) => Some((0, 1, AbstractMemorySize::zero(), U256_SIZE)),
        LdTrue | LdFalse => Some((0, 1, AbstractMemorySize::zero(), BOOL_SIZE)),

        // Reference operations with fixed cost
        FreezeRef => Some((1, 1, REFERENCE_SIZE, REFERENCE_SIZE)),
        MutBorrowLoc(_) | ImmBorrowLoc(_) => Some((0, 1, AbstractMemorySize::zero(), REFERENCE_SIZE)),
        MutBorrowField(_) | ImmBorrowField(_) | MutBorrowFieldGeneric(_) | ImmBorrowFieldGeneric(_) => {
            Some((1, 1, REFERENCE_SIZE, REFERENCE_SIZE))
        }

        // Cast operations - conservative estimate: smallest input, actual output
        CastU8 => Some((1, 1, U8_SIZE, U8_SIZE)),
        CastU16 => Some((1, 1, U8_SIZE, U16_SIZE)),
        CastU32 => Some((1, 1, U8_SIZE, U32_SIZE)),
        CastU64 => Some((1, 1, U8_SIZE, U64_SIZE)),
        CastU128 => Some((1, 1, U8_SIZE, U128_SIZE)),
        CastU256 => Some((1, 1, U8_SIZE, U256_SIZE)),

        // Arithmetic operations - conservative: smallest inputs, largest output
        Add | Sub | Mul | Mod | Div => Some((2, 1, U8_SIZE + U8_SIZE, U256_SIZE)),
        BitOr | BitAnd | Xor => Some((2, 1, U8_SIZE + U8_SIZE, U256_SIZE)),
        Shl | Shr => Some((2, 1, U8_SIZE + U8_SIZE, U256_SIZE)),

        // Boolean operations
        Or | And => Some((2, 1, BOOL_SIZE + BOOL_SIZE, BOOL_SIZE)),
        Not => Some((1, 1, BOOL_SIZE, BOOL_SIZE)),

        // Comparison operations
        Lt | Gt | Le | Ge => Some((2, 1, U8_SIZE + U8_SIZE, BOOL_SIZE)),

        // Abort
        Abort => Some((1, 0, U64_SIZE, AbstractMemorySize::zero())),

        // --- Variable cost instructions (need runtime size info) ---
        // LdConst: depends on constant size
        LdConst(_) => None,
        // CopyLoc, MoveLoc, StLoc: depends on local value size
        CopyLoc(_) | MoveLoc(_) | StLoc(_) => None,
        // Pop: depends on value size
        Pop => None,
        // ReadRef, WriteRef: depends on referenced value size
        ReadRef | WriteRef => None,
        // Eq, Neq: depends on operand sizes
        Eq | Neq => None,
        // Pack/Unpack: depends on field count/sizes
        Pack(_) | PackGeneric(_) | Unpack(_) | UnpackGeneric(_) => None,
        // Vector operations: depend on element sizes
        VecPack(_, _) | VecLen(_) | VecImmBorrow(_) | VecMutBorrow(_) |
        VecPushBack(_) | VecPopBack(_) | VecUnpack(_, _) | VecSwap(_) => None,
        // Variant operations: depend on field sizes
        PackVariant(_) | PackVariantGeneric(_) |
        UnpackVariant(_) | UnpackVariantImmRef(_) | UnpackVariantMutRef(_) |
        UnpackVariantGeneric(_) | UnpackVariantGenericImmRef(_) | UnpackVariantGenericMutRef(_) => None,
        // VariantSwitch: depends on value size
        VariantSwitch(_) => None,
        // Call operations: handled separately
        Call(_) | CallGeneric(_) => None,
        // Charge itself should not be in input
        Charge(..) => None,
    }
}

/// Compute the total fixed gas costs for a basic block.
fn compute_block_fixed_costs(code: &[ast::Bytecode]) -> BlockGasCost {
    let mut cost = BlockGasCost::default();
    for instr in code {
        if let Some((pops, pushes, pop_size, push_size)) = get_fixed_instruction_cost(instr) {
            cost.add(pops, pushes, pop_size, push_size);
        }
    }
    cost
}

pub(crate) fn package(pkg: Input::Package) -> ast::Package {
    let Input::Package {
        original_id,
        modules: in_modules,
        version_id,
        type_origin_table,
        linkage_table,
        version: _,
    } = pkg;
    let mut modules = BTreeMap::new();
    for (module_id, d_module) in in_modules {
        modules.insert(module_id, module(d_module));
    }
    ast::Package {
        original_id,
        modules,
        version_id,
        type_origin_table,
        linkage_table,
    }
}

fn module(m: Input::Module) -> ast::Module {
    let Input::Module {
        value: compiled_module,
    } = m;
    let functions = compiled_module
        .function_defs()
        .iter()
        .enumerate()
        .map(|(ndx, fun)| {
            let index = FF::FunctionDefinitionIndex::new(ndx as u16);
            (index, function(index, fun))
        })
        .collect();
    ast::Module {
        compiled_module,
        functions,
    }
}

fn function(ndx: FunctionDefinitionIndex, fun: &FunctionDefinition) -> ast::Function {
    let Some(code) = &fun.code else {
        return ast::Function { ndx, code: None };
    };
    let FF::CodeUnit {
        locals: _,
        code,
        jump_tables,
    } = code;

    let code = generate_basic_blocks(code, jump_tables);
    let jump_tables = jump_tables.clone();
    let code = Some(ast::Code { code, jump_tables });
    ast::Function { ndx, code }
}

fn generate_basic_blocks(
    input: &[FF::Bytecode],
    jump_tables: &[FF::VariantJumpTable],
) -> BTreeMap<ast::Label, Vec<ast::Bytecode>> {
    let cfg = VMControlFlowGraph::new(input, jump_tables);
    cfg.blocks()
        .map(|label| {
            let start = cfg.block_start(label) as usize;
            let end = cfg.block_end(label) as usize;
            let label = label as ast::Label;
            let code: Vec<ast::Bytecode> = input[start..(end + 1)].iter().map(bytecode).collect();

            // Compute fixed costs for the block
            let block_cost = compute_block_fixed_costs(&code);

            // Prepend Charge instruction if there are fixed costs
            let final_code = if block_cost.has_fixed_costs() {
                let mut new_code = Vec::with_capacity(code.len() + 1);
                new_code.push(ast::Bytecode::Charge(Box::new(ast::ChargeInfo {
                    instructions: block_cost.instructions,
                    pushes: block_cost.pushes,
                    pops: block_cost.pops,
                })));
                new_code.extend(code);
                new_code
            } else {
                code
            };

            (label, final_code)
        })
        .collect::<BTreeMap<ast::Label, Vec<ast::Bytecode>>>()
}

fn bytecode(code: &FF::Bytecode) -> ast::Bytecode {
    use ast::Bytecode;
    match code {
        FF::Bytecode::Call(ndx) => Bytecode::Call(*ndx),
        FF::Bytecode::CallGeneric(ndx) => Bytecode::CallGeneric(*ndx),

        // Standard Codes
        FF::Bytecode::Pop => Bytecode::Pop,
        FF::Bytecode::Ret => Bytecode::Ret,
        FF::Bytecode::BrTrue(n) => Bytecode::BrTrue(*n),
        FF::Bytecode::BrFalse(n) => Bytecode::BrFalse(*n),
        FF::Bytecode::Branch(n) => Bytecode::Branch(*n),

        FF::Bytecode::LdU256(n) => Bytecode::LdU256(n.clone()),
        FF::Bytecode::LdU128(n) => Bytecode::LdU128(n.clone()),
        FF::Bytecode::LdU16(n) => Bytecode::LdU16(*n),
        FF::Bytecode::LdU32(n) => Bytecode::LdU32(*n),
        FF::Bytecode::LdU64(n) => Bytecode::LdU64(*n),
        FF::Bytecode::LdU8(n) => Bytecode::LdU8(*n),

        FF::Bytecode::LdConst(ndx) => Bytecode::LdConst(*ndx),
        FF::Bytecode::LdTrue => Bytecode::LdTrue,
        FF::Bytecode::LdFalse => Bytecode::LdFalse,

        FF::Bytecode::CopyLoc(ndx) => Bytecode::CopyLoc(*ndx),
        FF::Bytecode::MoveLoc(ndx) => Bytecode::MoveLoc(*ndx),
        FF::Bytecode::StLoc(ndx) => Bytecode::StLoc(*ndx),
        FF::Bytecode::ReadRef => Bytecode::ReadRef,
        FF::Bytecode::WriteRef => Bytecode::WriteRef,
        FF::Bytecode::FreezeRef => Bytecode::FreezeRef,
        FF::Bytecode::MutBorrowLoc(ndx) => Bytecode::MutBorrowLoc(*ndx),
        FF::Bytecode::ImmBorrowLoc(ndx) => Bytecode::ImmBorrowLoc(*ndx),

        // Structs and Fields
        FF::Bytecode::Pack(ndx) => Bytecode::Pack(*ndx),
        FF::Bytecode::PackGeneric(ndx) => Bytecode::PackGeneric(*ndx),
        FF::Bytecode::Unpack(ndx) => Bytecode::Unpack(*ndx),
        FF::Bytecode::UnpackGeneric(ndx) => Bytecode::UnpackGeneric(*ndx),
        FF::Bytecode::MutBorrowField(ndx) => Bytecode::MutBorrowField(*ndx),
        FF::Bytecode::MutBorrowFieldGeneric(ndx) => Bytecode::MutBorrowFieldGeneric(*ndx),
        FF::Bytecode::ImmBorrowField(ndx) => Bytecode::ImmBorrowField(*ndx),
        FF::Bytecode::ImmBorrowFieldGeneric(ndx) => Bytecode::ImmBorrowFieldGeneric(*ndx),

        FF::Bytecode::Add => Bytecode::Add,
        FF::Bytecode::Sub => Bytecode::Sub,
        FF::Bytecode::Mul => Bytecode::Mul,
        FF::Bytecode::Mod => Bytecode::Mod,
        FF::Bytecode::Div => Bytecode::Div,
        FF::Bytecode::BitOr => Bytecode::BitOr,
        FF::Bytecode::BitAnd => Bytecode::BitAnd,
        FF::Bytecode::Xor => Bytecode::Xor,
        FF::Bytecode::Or => Bytecode::Or,
        FF::Bytecode::And => Bytecode::And,
        FF::Bytecode::Not => Bytecode::Not,
        FF::Bytecode::Eq => Bytecode::Eq,
        FF::Bytecode::Neq => Bytecode::Neq,
        FF::Bytecode::Lt => Bytecode::Lt,
        FF::Bytecode::Gt => Bytecode::Gt,
        FF::Bytecode::Le => Bytecode::Le,
        FF::Bytecode::Ge => Bytecode::Ge,
        FF::Bytecode::Abort => Bytecode::Abort,
        FF::Bytecode::Nop => Bytecode::Nop,
        FF::Bytecode::Shl => Bytecode::Shl,
        FF::Bytecode::Shr => Bytecode::Shr,

        FF::Bytecode::CastU256 => Bytecode::CastU256,
        FF::Bytecode::CastU128 => Bytecode::CastU128,
        FF::Bytecode::CastU16 => Bytecode::CastU16,
        FF::Bytecode::CastU32 => Bytecode::CastU32,
        FF::Bytecode::CastU64 => Bytecode::CastU64,
        FF::Bytecode::CastU8 => Bytecode::CastU8,

        // Vectors
        FF::Bytecode::VecPack(si, size) => Bytecode::VecPack(*si, *size),
        FF::Bytecode::VecLen(si) => Bytecode::VecLen(*si),
        FF::Bytecode::VecImmBorrow(si) => Bytecode::VecImmBorrow(*si),
        FF::Bytecode::VecMutBorrow(si) => Bytecode::VecMutBorrow(*si),
        FF::Bytecode::VecPushBack(si) => Bytecode::VecPushBack(*si),
        FF::Bytecode::VecPopBack(si) => Bytecode::VecPopBack(*si),
        FF::Bytecode::VecUnpack(si, size) => Bytecode::VecUnpack(*si, *size),
        FF::Bytecode::VecSwap(si) => Bytecode::VecSwap(*si),

        FF::Bytecode::PackVariant(ndx) => Bytecode::PackVariant(*ndx),
        FF::Bytecode::PackVariantGeneric(ndx) => Bytecode::PackVariantGeneric(*ndx),
        FF::Bytecode::UnpackVariant(ndx) => Bytecode::UnpackVariant(*ndx),
        FF::Bytecode::UnpackVariantImmRef(ndx) => Bytecode::UnpackVariantImmRef(*ndx),
        FF::Bytecode::UnpackVariantMutRef(ndx) => Bytecode::UnpackVariantMutRef(*ndx),
        FF::Bytecode::UnpackVariantGeneric(ndx) => Bytecode::UnpackVariantGeneric(*ndx),
        FF::Bytecode::UnpackVariantGenericImmRef(ndx) => Bytecode::UnpackVariantGenericImmRef(*ndx),
        FF::Bytecode::UnpackVariantGenericMutRef(ndx) => Bytecode::UnpackVariantGenericMutRef(*ndx),
        FF::Bytecode::VariantSwitch(ndx) => Bytecode::VariantSwitch(*ndx),

        // Deprecated bytecodes -- bail
        FF::Bytecode::ExistsDeprecated(_)
        | FF::Bytecode::ExistsGenericDeprecated(_)
        | FF::Bytecode::MoveFromDeprecated(_)
        | FF::Bytecode::MoveFromGenericDeprecated(_)
        | FF::Bytecode::MoveToDeprecated(_)
        | FF::Bytecode::MoveToGenericDeprecated(_)
        | FF::Bytecode::MutBorrowGlobalDeprecated(_)
        | FF::Bytecode::MutBorrowGlobalGenericDeprecated(_)
        | FF::Bytecode::ImmBorrowGlobalDeprecated(_)
        | FF::Bytecode::ImmBorrowGlobalGenericDeprecated(_) => {
            unreachable!("Global bytecodes deprecated")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jit::optimization::ast::{Bytecode, ChargeInfo};
    use move_binary_format::file_format::{FieldHandleIndex, FieldInstantiationIndex};

    fn assert_cost(code: &[Bytecode], expected_instrs: u64, expected_pushes: u64, expected_pops: u64) {
        let cost = compute_block_fixed_costs(code);
        assert_eq!(
            cost.instructions, expected_instrs,
            "instructions: expected {}, got {}",
            expected_instrs, cost.instructions
        );
        assert_eq!(
            cost.pushes, expected_pushes,
            "pushes: expected {}, got {}",
            expected_pushes, cost.pushes
        );
        assert_eq!(
            cost.pops, expected_pops,
            "pops: expected {}, got {}",
            expected_pops, cost.pops
        );
    }

    #[test]
    fn test_all_fixed_arithmetic() {
        // Each arithmetic op: 2 pops, 1 push
        let code = vec![
            Bytecode::Add,
            Bytecode::Sub,
            Bytecode::Mul,
            Bytecode::Div,
            Bytecode::Mod,
        ];
        assert_cost(&code, 5, 5, 10);
    }

    #[test]
    fn test_all_variable_cost() {
        let code = vec![
            Bytecode::CopyLoc(0),
            Bytecode::MoveLoc(0),
            Bytecode::StLoc(0),
            Bytecode::Pop,
        ];
        let cost = compute_block_fixed_costs(&code);
        assert_eq!(cost.instructions, 0);
        assert!(!cost.has_fixed_costs());
    }

    #[test]
    fn test_mixed_instructions() {
        // LdU64(42): 0 pops, 1 push
        // LdU64(7):  0 pops, 1 push
        // Add:       2 pops, 1 push
        // StLoc(0):  variable-cost, skipped
        let code = vec![
            Bytecode::LdU64(42),
            Bytecode::LdU64(7),
            Bytecode::Add,
            Bytecode::StLoc(0),
        ];
        assert_cost(&code, 3, 3, 2);
    }

    #[test]
    fn test_loads_and_booleans() {
        let code = vec![
            Bytecode::LdU8(1),
            Bytecode::LdU64(2),
            Bytecode::LdTrue,
            Bytecode::LdFalse,
        ];
        assert_cost(&code, 4, 4, 0);
    }

    #[test]
    fn test_branches() {
        // BrTrue: 1 pop, 0 push
        // BrFalse: 1 pop, 0 push
        // Branch: 0 pop, 0 push
        let code = vec![
            Bytecode::BrTrue(5),
            Bytecode::BrFalse(3),
            Bytecode::Branch(0),
        ];
        assert_cost(&code, 3, 0, 2);
    }

    #[test]
    fn test_comparisons_and_boolean_ops() {
        // Lt: 2 pops, 1 push
        // Gt: 2 pops, 1 push
        // Or: 2 pops, 1 push
        // And: 2 pops, 1 push
        // Not: 1 pop, 1 push
        let code = vec![
            Bytecode::Lt,
            Bytecode::Gt,
            Bytecode::Or,
            Bytecode::And,
            Bytecode::Not,
        ];
        assert_cost(&code, 5, 5, 9);
    }

    #[test]
    fn test_casts() {
        // Each cast: 1 pop, 1 push
        let code = vec![
            Bytecode::CastU8,
            Bytecode::CastU64,
            Bytecode::CastU256,
        ];
        assert_cost(&code, 3, 3, 3);
    }

    #[test]
    fn test_empty_block() {
        let cost = compute_block_fixed_costs(&[]);
        assert_eq!(cost.instructions, 0);
        assert!(!cost.has_fixed_costs());
    }

    #[test]
    fn test_single_ret() {
        // Ret: 0 pops, 0 pushes
        let code = vec![Bytecode::Ret];
        assert_cost(&code, 1, 0, 0);
    }

    #[test]
    fn test_charge_ignored() {
        let code = vec![Bytecode::Charge(Box::new(ChargeInfo {
            instructions: 99,
            pushes: 99,
            pops: 99,
        }))];
        let cost = compute_block_fixed_costs(&code);
        assert_eq!(cost.instructions, 0);
        assert!(!cost.has_fixed_costs());
    }

    #[test]
    fn test_reference_ops() {
        // FreezeRef: 1 pop, 1 push
        // MutBorrowLoc: 0 pops, 1 push
        // ImmBorrowLoc: 0 pops, 1 push
        let code = vec![
            Bytecode::FreezeRef,
            Bytecode::MutBorrowLoc(0),
            Bytecode::ImmBorrowLoc(1),
        ];
        assert_cost(&code, 3, 3, 1);
    }

    #[test]
    fn test_bitwise_ops() {
        // Each: 2 pops, 1 push
        let code = vec![
            Bytecode::BitOr,
            Bytecode::BitAnd,
            Bytecode::Xor,
            Bytecode::Shl,
            Bytecode::Shr,
        ];
        assert_cost(&code, 5, 5, 10);
    }

    #[test]
    fn test_abort() {
        // Abort: 1 pop, 0 pushes
        let code = vec![Bytecode::Abort];
        assert_cost(&code, 1, 0, 1);
    }

    #[test]
    fn test_field_borrow_ops() {
        // MutBorrowField: 1 pop, 1 push
        // ImmBorrowField: 1 pop, 1 push
        let code = vec![
            Bytecode::MutBorrowField(FieldHandleIndex::new(0)),
            Bytecode::ImmBorrowField(FieldHandleIndex::new(0)),
            Bytecode::MutBorrowFieldGeneric(FieldInstantiationIndex::new(0)),
            Bytecode::ImmBorrowFieldGeneric(FieldInstantiationIndex::new(0)),
        ];
        assert_cost(&code, 4, 4, 4);
    }

    #[test]
    fn test_all_load_sizes() {
        let code = vec![
            Bytecode::LdU8(0),
            Bytecode::LdU16(0),
            Bytecode::LdU32(0),
            Bytecode::LdU64(0),
            Bytecode::LdU128(Box::new(0)),
            Bytecode::LdU256(Box::new(move_core_types::u256::U256::zero())),
        ];
        assert_cost(&code, 6, 6, 0);
    }

    #[test]
    fn test_all_comparison_ops() {
        // Each: 2 pops, 1 push
        let code = vec![
            Bytecode::Lt,
            Bytecode::Gt,
            Bytecode::Le,
            Bytecode::Ge,
        ];
        assert_cost(&code, 4, 4, 8);
    }

    #[test]
    fn test_eq_neq_are_variable_cost() {
        let code = vec![Bytecode::Eq, Bytecode::Neq];
        let cost = compute_block_fixed_costs(&code);
        assert_eq!(cost.instructions, 0);
        assert!(!cost.has_fixed_costs());
    }

    #[test]
    fn test_nop() {
        let code = vec![Bytecode::Nop, Bytecode::Nop, Bytecode::Nop];
        assert_cost(&code, 3, 0, 0);
    }
}
