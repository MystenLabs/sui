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
        Charge { .. } => None,
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
                new_code.push(ast::Bytecode::Charge {
                    instructions: block_cost.instructions,
                    pushes: block_cost.pushes,
                    pops: block_cost.pops,
                });
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
