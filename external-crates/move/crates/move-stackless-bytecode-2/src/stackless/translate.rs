// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfg::{ControlFlowGraph, StacklessControlFlowGraph},
    stackless::{
        ast::{self, BasicBlock, Instruction, Operand::Var, PrimitiveOp, RValue, Var::Register},
        context::Context,
    },
};

use move_binary_format::{normalized as N, normalized::Bytecode as IB, file_format::JumpTableInner};
use move_model_2::{
    model::{Model as Model2, Module, Package},
    source_kind::SourceKind,
};
use move_symbol_pool::Symbol;

use anyhow::Ok;
use std::{
    collections::BTreeMap,
    fmt::{Debug, Display},
    hash::Hash,
    vec,
};

use super::ast::Immediate;

// -------------------------------------------------------------------------------------------------
// Stackless Bytecode Translation
// -------------------------------------------------------------------------------------------------
pub(crate) fn packages<K: SourceKind>(model: &Model2<K>) -> anyhow::Result<Vec<ast::Package>> {
    let mut context = Context::new(model);
    let mut packages = vec![];
    let m_packages = model.packages();
    for m_package in m_packages {
        let package = package(&mut context, m_package)?;
        packages.push(package);
    }
    Ok(packages)
}

pub(crate) fn package<K: SourceKind>(
    context: &mut Context<'_, K>,
    package: Package<K>,
) -> anyhow::Result<ast::Package> {
    let package_name = package.name();
    let package_address = package.address();

    let m_modules = package.modules();

    let out_modules = m_modules
        .into_iter()
        .map(|m_module| module(context, m_module))
        .collect::<Result<Vec<_>, _>>()?;

    let package = ast::Package {
        name: package_name,
        address: package_address,
        modules: out_modules.into_iter().map(|m| (m.name, m)).collect(),
    };
    Ok(package)
}

pub(crate) fn module<K: SourceKind>(
    context: &mut Context<'_, K>,
    module: Module<K>,
) -> anyhow::Result<ast::Module> {
    let module = module.compiled();
    let name = *module.name();
    let _module_address = module.address();
    // println!("\nModule: {} ({})", module_name, module_address);

    let mut functions = BTreeMap::new();

    for fun in module.functions.values() {
        context.var_counter.reset();
        let function_name = fun.name;
        functions.insert(function_name, function(context, fun)?);
    }

    let module = ast::Module { name, functions };

    Ok(module)
}

pub(crate) fn function<K: SourceKind>(
    ctxt: &mut Context<'_, K>,
    function: &N::Function<Symbol>,
) -> anyhow::Result<ast::Function> {
    let name = function.name;
    // println!("\nFunction: {}", function_name);
    let code = function.code();
    let cfg = StacklessControlFlowGraph::new(code, function.jump_tables());

    let mut bbs = vec![];
    for block_id in cfg.blocks() {
        let blk_start = cfg.block_start(block_id);
        let blk_end = cfg.block_end(block_id);
        let code_range = &code[blk_start as usize..(blk_end + 1) as usize];
        println!("Code {:?}", code_range);
        let block_instructions = code_range
            .iter()
            .map(|op| bytecode(ctxt, op))
            .collect::<Result<Vec<_>, _>>()?;
        let bb = BasicBlock::from_instructions(block_id as usize, block_instructions);
        bbs.push(bb);
    }
    let function = ast::Function {
        name,
        basic_blocks: bbs,
    };

    Ok(function)
}

pub(crate) fn bytecode<S: Hash + Eq + Display + Debug, K: SourceKind>(
    ctxt: &mut Context<'_, K>,
    op: &IB<S>,
) -> anyhow::Result<Instruction> {
    match op {
        IB::Pop => {
            // TODO: how to handle Pop?
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Immediate(Immediate::Empty),
            };
            Ok(inst)
        }

        IB::Ret => {
            // TODO: This should look at the function's return arity and grab values off the
            // logical stack accordingly.
            // TODO: ok for rarity, bu then whate do we do with the values? do we assigne them to a register?
            let inst = Instruction::Return(vec![Register(ctxt.var_counter.last())]);
            Ok(inst)
        }

        IB::BrTrue(code_offset) => {
            let inst = Instruction::Branch {
                condition: Register(ctxt.get_var_counter().last()),
                // TODO: get the instruction counter, from context maybe?
                then_label: 0 as usize,
                else_label: *code_offset as usize,
            };
            Ok(inst)
        }
        IB::BrFalse(code_offset) => {
            let inst = Instruction::Branch {
                // TODO: should we swap the then and else labels?
                condition: Register(ctxt.get_var_counter().last()),
                // TODO: get the instruction counter, from context maybe?
                then_label: 0 as usize,
                else_label: *code_offset as usize,
            };
            Ok(inst)
        }
        IB::Branch(code_offset) => {
            let inst = Instruction::Branch {
                condition: Register(ctxt.get_var_counter().last()),
                // TODO: get the instruction counter, from context maybe?
                then_label: *code_offset as usize,
                else_label: *code_offset as usize,
            };
            Ok(inst)
        }
        IB::LdU8(value) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Immediate(Immediate::U8(*value)),
            };
            Ok(inst)
        }

        IB::LdU64(value) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Immediate(Immediate::U64(*value)),
            };
            Ok(inst)
        }

        IB::LdU128(bx) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Immediate(Immediate::U128(*(*bx))),
            };
            Ok(inst)
        }

        IB::CastU8 => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::CastU8,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::CastU64 => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::CastU64,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::CastU128 => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::CastU128,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::LdConst(_const_ref) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::LdConst,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::LdTrue => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Immediate(Immediate::True),
            };
            Ok(inst)
        }

        IB::LdFalse => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Immediate(Immediate::False),
            };
            Ok(inst)
        }

        IB::CopyLoc(_loc) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::CopyLoc,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::MoveLoc(_loc) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::MoveLoc,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::StLoc(_loc) => {
            if ctxt.var_counter.current() < 1 {
                panic!("Not enough variables to perform StLoc operation");
            }
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::StoreLoc,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::Call(_bx) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Call {
                    // TODO get the function id from the context
                    function: 0 as usize,
                    args: vec![], // TODO get the args from the context
                },
            };
            Ok(inst)
        }

        IB::Pack(_struct_ref) => {
            let args = _struct_ref
                .struct_
                .fields
                .0
                .iter()
                .enumerate()
                .map(|(i, _)| Var(Register(ctxt.var_counter.last() - i)))
                .collect::<Vec<_>>();
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Pack,
                    args,
                },
            };
            Ok(inst)
        }

        IB::Unpack(bx) => {
            let lhs = bx
                .struct_
                .fields
                .0
                .iter()
                .enumerate()
                .map(|(i, _)| Register(ctxt.var_counter.next() + i))
                .collect::<Vec<_>>();
            let inst = Instruction::Assign {
                lhs,
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Unpack,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::ReadRef => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::ReadRef,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::WriteRef => {
            if ctxt.var_counter.current() < 1 {
                panic!("Not enough variables to perform WriteRef operation");
            }
            let inst = Instruction::Assign {
                lhs: vec![(Register(ctxt.var_counter.prev()))],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::WriteRef,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            ctxt.var_counter.increment();
            Ok(inst)
        }

        IB::FreezeRef => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::FreezeRef,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::MutBorrowLoc(_local_index) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::MutBorrowLoc,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }
        IB::ImmBorrowLoc(_local_index) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::ImmBorrowLoc,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::MutBorrowField(_field_ref) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::MutBorrowField,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::ImmBorrowField(_field_ref) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::ImmBorrowField,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::Add => {
            if ctxt.var_counter.current() < 2 {
                panic!("Not enough variables to perform Add operation");
            }
            let rhs = Var(Register(ctxt.var_counter.last()));
            let lhs = Var(Register(ctxt.var_counter.current()));
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Add,
                    args: vec![lhs, rhs],
                },
            };
            Ok(inst)
        }

        IB::Sub => {
            if ctxt.var_counter.current() < 2 {
                panic!("Not enough variables to perform Add operation");
            }
            // TODO: check operand order
            let rhs = Var(Register(ctxt.var_counter.last()));
            let lhs = Var(Register(ctxt.var_counter.current()));
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Subtract,
                    args: vec![lhs, rhs],
                },
            };
            Ok(inst)
        }

        IB::Mul => {
            if ctxt.var_counter.current() < 2 {
                panic!("Not enough variables to perform Mul operation");
            }
            let rhs = Var(Register(ctxt.var_counter.last()));
            let lhs = Var(Register(ctxt.var_counter.current()));
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Multiply,
                    args: vec![lhs, rhs],
                },
            };
            Ok(inst)
        }

        IB::Mod => {
            if ctxt.var_counter.current() < 2 {
                panic!("Not enough variables to perform Mod operation");
            }
            let rhs = Var(Register(ctxt.var_counter.last()));
            let lhs = Var(Register(ctxt.var_counter.current()));
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Modulo,
                    args: vec![lhs, rhs],
                },
            };
            Ok(inst)
        }

        IB::Div => {
            if ctxt.var_counter.current() < 2 {
                panic!("Not enough variables to perform Add operation");
            }
            // TODO: check operand order
            let rhs = Var(Register(ctxt.var_counter.last()));
            let lhs = Var(Register(ctxt.var_counter.current()));
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Divide,
                    args: vec![lhs, rhs],
                },
            };
            Ok(inst)
        }
        IB::BitOr => {
            if ctxt.var_counter.current() < 2 {
                panic!("Not enough variables to perform BitOr operation");
            }
            let rhs = Var(Register(ctxt.var_counter.last()));
            let lhs = Var(Register(ctxt.var_counter.current()));
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::BitOr,
                    args: vec![lhs, rhs],
                },
            };
            Ok(inst)
        }

        IB::BitAnd => {
            if ctxt.var_counter.current() < 2 {
                panic!("Not enough variables to perform BitAnd operation");
            }
            let rhs = Var(Register(ctxt.var_counter.last()));
            let lhs = Var(Register(ctxt.var_counter.current()));
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::BitAnd,
                    args: vec![lhs, rhs],
                },
            };
            Ok(inst)
        }

        IB::Xor => {
            if ctxt.var_counter.current() < 2 {
                panic!("Not enough variables to perform Xor operation");
            }
            let rhs = Var(Register(ctxt.var_counter.last()));
            let lhs = Var(Register(ctxt.var_counter.current()));
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Xor,
                    args: vec![lhs, rhs],
                },
            };
            Ok(inst)
        }

        IB::Or => {
            if ctxt.var_counter.current() < 2 {
                panic!("Not enough variables to perform Or operation");
            }
            let rhs = Var(Register(ctxt.var_counter.last()));
            let lhs = Var(Register(ctxt.var_counter.current()));
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Or,
                    args: vec![lhs, rhs],
                },
            };
            Ok(inst)
        }

        IB::And => {
            if ctxt.var_counter.current() < 2 {
                panic!("Not enough variables to perform And operation");
            }
            let rhs = Var(Register(ctxt.var_counter.last()));
            let lhs = Var(Register(ctxt.var_counter.current()));
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::And,
                    args: vec![lhs, rhs],
                },
            };
            Ok(inst)
        }

        IB::Not => {
            if ctxt.var_counter.current() < 1 {
                panic!("Not enough variables to perform Not operation");
            }
            let rhs = Var(Register(ctxt.var_counter.last()));
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Not,
                    args: vec![rhs],
                },
            };
            Ok(inst)
        }
        IB::Eq => {
            if ctxt.var_counter.current() < 2 {
                panic!("Not enough variables to perform Eq operation");
            }
            let rhs = Var(Register(ctxt.var_counter.last()));
            let lhs = Var(Register(ctxt.var_counter.current()));
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Equal,
                    args: vec![lhs, rhs],
                },
            };
            Ok(inst)
        }

        IB::Neq => {
            if ctxt.var_counter.current() < 2 {
                panic!("Not enough variables to perform Neq operation");
            }
            let rhs = Var(Register(ctxt.var_counter.last()));
            let lhs = Var(Register(ctxt.var_counter.current()));
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::NotEqual,
                    args: vec![lhs, rhs],
                },
            };
            Ok(inst)
        }

        IB::Lt => {
            if ctxt.var_counter.current() < 2 {
                panic!("Not enough variables to perform Lt operation");
            }
            let rhs = Var(Register(ctxt.var_counter.last()));
            let lhs = Var(Register(ctxt.var_counter.current()));
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::LessThan,
                    args: vec![lhs, rhs],
                },
            };
            Ok(inst)
        }

        IB::Gt => {
            if ctxt.var_counter.current() < 2 {
                panic!("Not enough variables to perform Gt operation");
            }
            let rhs = Var(Register(ctxt.var_counter.last()));
            let lhs = Var(Register(ctxt.var_counter.current()));
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::GreaterThan,
                    args: vec![lhs, rhs],
                },
            };
            Ok(inst)
        }
        IB::Le => {
            if ctxt.var_counter.current() < 2 {
                panic!("Not enough variables to perform Le operation");
            }
            let rhs = Var(Register(ctxt.var_counter.last()));
            let lhs = Var(Register(ctxt.var_counter.current()));
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::LessThanOrEqual,
                    args: vec![lhs, rhs],
                },
            };
            Ok(inst)
        }

        IB::Ge => {
            if ctxt.var_counter.current() < 2 {
                panic!("Not enough variables to perform Ge operation");
            }
            let rhs = Var(Register(ctxt.var_counter.last()));
            let lhs = Var(Register(ctxt.var_counter.current()));
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::GreaterThanOrEqual,
                    args: vec![lhs, rhs],
                },
            };
            Ok(inst)
        }

        IB::Abort => {
            let inst = Instruction::Abort;
            Ok(inst)
        }

        IB::Nop => {
            let inst = Instruction::Nop;
            Ok(inst)
        }

        IB::Shl => {
            if ctxt.var_counter.current() < 1 {
                panic!("Not enough variables to perform Not operation");
            }
            let rhs = Var(Register(ctxt.var_counter.last()));
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::ShiftLeft,
                    args: vec![rhs],
                },
            };
            Ok(inst)
        }

        IB::Shr => {
            if ctxt.var_counter.current() < 1 {
                panic!("Not enough variables to perform Not operation");
            }
            let rhs = Var(Register(ctxt.var_counter.last()));
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::ShiftRight,
                    args: vec![rhs],
                },
            };
            Ok(inst)
        }

        IB::VecPack(_bx) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::VecPack,
                    // VecPack will always take one arg only
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::VecLen(_rc) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::VecLen,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::VecImmBorrow(_rc) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::VecImmBorrow,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::VecMutBorrow(_rc) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::VecMutBorrow,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::VecPushBack(_rc) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::VecPushBack,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::VecPopBack(_rc) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::VecPopBack,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::VecUnpack(_bx) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::VecUnpack,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::VecSwap(_rc) => {
            let args = [0,1,2].iter()
                .map(|i| Var(Register(ctxt.var_counter.last() - i)))
                .collect::<Vec<_>>();
            let inst = Instruction::Assign {
                // TODO  check order of the registers
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::VecSwap,
                    args
                },
            };
            Ok(inst)
        }

        IB::LdU16(value) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Immediate(Immediate::U16(*value)),
            };
            Ok(inst)
        }
        IB::LdU32(value) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Immediate(Immediate::U32(*value)),
            };
            Ok(inst)
        }

        IB::LdU256(_bx) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Immediate(Immediate::U256(*(*_bx))),
            };
            Ok(inst)
        }

        IB::CastU16 => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::CastU16,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::CastU32 => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::CastU32,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::CastU256 => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::CastU256,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::PackVariant(bx) => {
            let args = bx
                .variant
                .fields
                .0
                .iter()
                .enumerate()
                .map(|(i, _)| Var(Register(ctxt.var_counter.last() - i)))
                .collect::<Vec<_>>();
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::PackVariant,
                    args,
                },
            };
            Ok(inst)
        }
        IB::UnpackVariant(bx) => {
            let lhs = bx
                .variant
                .fields
                .0
                .iter()
                .enumerate()
                .map(|(i, _)| Register(ctxt.var_counter.next() + i))
                .collect::<Vec<_>>();
            let inst = Instruction::Assign {
                lhs,
                rhs: RValue::Primitive {
                    op: PrimitiveOp::UnpackVariant,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::UnpackVariantImmRef(bx) => {
            let lhs = bx
                .variant
                .fields
                .0
                .iter()
                .enumerate()
                .map(|(i, _)| Register(ctxt.var_counter.next() + i))
                .collect::<Vec<_>>();
            let inst = Instruction::Assign {
                lhs,
                rhs: RValue::Primitive {
                    op: PrimitiveOp::UnpackVariant,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::UnpackVariantMutRef(bx) => {
            let lhs = bx
                .variant
                .fields
                .0
                .iter()
                .enumerate()
                .map(|(i, _)| Register(ctxt.var_counter.next() + i))
                .collect::<Vec<_>>();
            let inst = Instruction::Assign {
                lhs,
                rhs: RValue::Primitive {
                    op: PrimitiveOp::UnpackVariant,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::VariantSwitch(jt) => {
            let JumpTableInner::Full(offsets) = &jt.jump_table;
            let inst = Instruction::VariantSwitch {
                cases: offsets
                    .iter()
                    .map(|offset| *offset as usize)
                    .collect::<Vec<_>>(),
            };
            Ok(inst)
        }

        // ******** DEPRECATED BYTECODES ********
        IB::MutBorrowGlobalDeprecated(_bx) => Ok(Instruction::NotImplemented(format!("{:?}", op))),
        IB::ImmBorrowGlobalDeprecated(_bx) => Ok(Instruction::NotImplemented(format!("{:?}", op))),
        IB::ExistsDeprecated(_bx) => Ok(Instruction::NotImplemented(format!("{:?}", op))),
        IB::MoveFromDeprecated(_bx) => Ok(Instruction::NotImplemented(format!("{:?}", op))),
        IB::MoveToDeprecated(_bx) => Ok(Instruction::NotImplemented(format!("{:?}", op))),
    }
}
