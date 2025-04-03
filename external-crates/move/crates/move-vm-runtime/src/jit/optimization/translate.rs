// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{jit::optimization::ast, validation::verification::ast as Input};

use move_abstract_interpreter::control_flow_graph::{ControlFlowGraph, VMControlFlowGraph};
use move_binary_format::file_format::{self as FF, FunctionDefinition, FunctionDefinitionIndex};

use std::collections::BTreeMap;

pub(crate) fn package(pkg: Input::Package) -> ast::Package {
    let Input::Package {
        original_id,
        modules: in_modules,
        version_id,
        type_origin_table,
        linkage_table,
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
        .iter()
        .map(|label| {
            let start = cfg.block_start(*label) as usize;
            let end = cfg.block_end(*label) as usize;
            let label = *label as ast::Label;
            let code = input[start..(end + 1)].iter().map(bytecode).collect();
            (label, code)
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
