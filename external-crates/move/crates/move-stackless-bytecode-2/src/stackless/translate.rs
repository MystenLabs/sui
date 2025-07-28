// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::stackless::{
    ast::{
        self, BasicBlock, Instruction, RValue,
        Trivial::{Immediate, Register},
        Value,
    },
    context::Context,
    optimizations::optimize,
};

use move_abstract_interpreter::control_flow_graph::ControlFlowGraph;
use move_binary_format::{
    file_format::JumpTableInner, normalized as N, normalized::Bytecode as IB,
};
use move_model_2::{
    model::{Model as Model2, Module, Package},
    source_kind::SourceKind,
};
use move_symbol_pool::Symbol;

use std::{collections::BTreeMap, result::Result::Ok, vec};

// -------------------------------------------------------------------------------------------------
// Stackless Bytecode Translation
// -------------------------------------------------------------------------------------------------
pub(crate) fn packages<K: SourceKind>(
    model: &Model2<K>,
    optimize: bool,
) -> anyhow::Result<Vec<ast::Package>> {
    let mut context = Context::new(model);
    context.optimize(optimize);
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

    let mut functions = BTreeMap::new();

    for fun in module.functions.values() {
        context.var_counter.reset();
        context.locals_counter.reset();
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
    let code = function.code();
    if code.is_empty() {
        return Ok(ast::Function {
            name,
            entry_label: 0,
            basic_blocks: BTreeMap::new(),
        });
    }

    let jump_tables = function.jump_tables();

    let cfg =
        move_abstract_interpreter::control_flow_graph::VMControlFlowGraph::new(code, jump_tables);

    let mut basic_blocks = BTreeMap::new();

    for block_id in cfg.blocks() {
        let blk_start = cfg.block_start(block_id);
        let blk_end = cfg.block_end(block_id);
        let code_range = &code[blk_start as usize..(blk_end + 1) as usize];
        let block_instructions = code_range
            .iter()
            .enumerate()
            .map(|(i, op)| bytecode(ctxt, op, blk_start as usize + i, function))
            .collect::<Vec<_>>();

        let label = block_id as usize;
        let bb = BasicBlock::from_instructions(label, block_instructions);
        if !ctxt.logical_stack.is_empty() {
            panic!("Logical stack not empty: {:#?}\n{}", ctxt.logical_stack, bb);
        }
        basic_blocks.insert(label, bb);
    }

    let mut function = ast::Function {
        name,
        entry_label: cfg.entry_block_id() as usize,
        basic_blocks,
    };

    if ctxt.optimize {
        optimize(&mut function)
    }

    Ok(function)
}

pub(crate) fn bytecode<K: SourceKind>(
    ctxt: &mut Context<'_, K>,
    op: &IB<Symbol>,
    pc: usize,
    function: &N::Function<Symbol>,
) -> Instruction {
    use ast::DataOp;
    use ast::LocalOp as LocOp;
    use ast::PrimitiveOp as Op;

    macro_rules! assign_reg {
        ([$($lhs:expr),*] = $rhs:expr) => {{
            let rhs = $rhs;
            Instruction::AssignReg {
                lhs: vec![$($lhs),*],
                rhs,
            }
        }};
    }

    macro_rules! imm {
        ($val:expr) => {
            RValue::Trivial(Immediate($val))
        };
    }

    macro_rules! primitive_op {
        ($op:expr, $($rval:expr),+ $(,)?) => {
            RValue::Primitive {
                op: $op,
                args: vec![$($rval),+],
            }
        };
    }

    macro_rules! data_op {
        ($op:expr, $($rval:expr),+ $(,)?) => {
            RValue::Data {
                op: $op,
                args: vec![$($rval),+],
            }
        };
    }

    macro_rules! make_vec {
        ($n:expr, $e:expr) => {{ (0..$n).map(|_| $e).collect::<Vec<_>>() }};
    }

    macro_rules! push {
        () => {
            ctxt.push_register()
        };
    }

    macro_rules! pop {
        () => {
            ctxt.pop_register()
        };
    }

    match op {
        IB::Pop => Instruction::Drop(pop!()),

        IB::Ret => {
            // TODO: check if this needs to be reversed?
            let returned_vars = make_vec!(function.return_.len(), Register(pop!()));
            Instruction::Return(returned_vars)
        }

        IB::BrTrue(code_offset) => Instruction::JumpIf {
            condition: Register(pop!()),
            then_label: *code_offset as usize,
            else_label: pc + 1,
        },

        IB::BrFalse(code_offset) => Instruction::JumpIf {
            condition: Register(pop!()),
            then_label: pc + 1,
            else_label: *code_offset as usize,
        },

        IB::Branch(code_offset) => Instruction::Jump(*code_offset as usize),

        IB::LdU8(value) => assign_reg!([push!()] = imm!(Value::U8(*value))),

        IB::LdU64(value) => assign_reg!([push!()] = imm!(Value::U64(*value))),

        IB::LdU128(bx) => assign_reg!([push!()] = imm!(Value::U128(*(*bx)))),

        IB::CastU8 => {
            assign_reg!([push!()] = primitive_op!(Op::CastU8, Register(pop!())))
        }

        IB::CastU64 => {
            assign_reg!([push!()] = primitive_op!(Op::CastU64, Register(pop!())))
        }

        IB::CastU128 => {
            assign_reg!([push!()] = primitive_op!(Op::CastU128, Register(pop!())))
        }

        IB::LdConst(const_ref) => {
            assign_reg!([push!()] = RValue::Constant(const_ref.clone()))
        }

        IB::LdTrue => assign_reg!([push!()] = imm!(Value::Bool(true))),

        IB::LdFalse => assign_reg!([push!()] = imm!(Value::Bool(false))),

        IB::CopyLoc(loc) => {
            assign_reg!(
                [push!()] = RValue::Local {
                    op: LocOp::Copy,
                    arg: *loc as usize
                }
            )
        }

        IB::MoveLoc(loc) => {
            assign_reg!(
                [push!()] = RValue::Local {
                    op: LocOp::Move,
                    arg: *loc as usize
                }
            )
        }

        IB::StLoc(loc) => Instruction::StoreLoc {
            loc: *loc as usize,
            value: Register(pop!()),
        },

        IB::Call(bx) => {
            let name = &bx.module.name;
            let mut modules = ctxt.model.modules();
            let module = modules
                .find(|m| {
                    m.compiled().name() == (&bx.module.name)
                        && *m.compiled().address() == bx.module.address
                })
                .unwrap_or_else(|| {
                    panic!(
                        "Module {} with address {} not found in the model",
                        name, bx.module.address
                    )
                });
            let compiled = module.compiled();
            let function = compiled
                .functions
                .get(&bx.function)
                .unwrap_or_else(|| panic!("Function {} not found in module {}", bx.function, name));

            let args = make_vec!(function.parameters.len(), Register(pop!()));

            let lhs = make_vec!(function.return_.len(), push!());

            Instruction::AssignReg {
                lhs,
                rhs: RValue::Call {
                    function: function.name,
                    args,
                },
            }
        }

        IB::Pack(struct_ref) => {
            let args = make_vec!(struct_ref.struct_.fields.0.len(), Register(pop!()));
            assign_reg!(
                [push!()] = RValue::Data {
                    op: DataOp::Pack,
                    args
                }
            )
        }

        IB::Unpack(bx) => {
            let rhs = RValue::Data {
                op: DataOp::Unpack,
                args: vec![Register(pop!())],
            };
            let lhs = make_vec!(bx.struct_.fields.0.len(), push!());
            Instruction::AssignReg { rhs, lhs }
        }

        IB::ReadRef => {
            assign_reg!([push!()] = data_op!(DataOp::ReadRef, Register(pop!())))
        }

        // TODO check if this is ok for the SSA
        IB::WriteRef => {
            assign_reg!([] = data_op!(DataOp::WriteRef, Register(pop!()), Register(pop!())))
        }

        IB::FreezeRef => Instruction::Nop,

        IB::MutBorrowLoc(loc) => {
            assign_reg!(
                [push!()] = RValue::Local {
                    op: LocOp::Borrow(ast::Mutability::Mutable),
                    arg: *loc as usize
                }
            )
        }

        IB::ImmBorrowLoc(loc) => {
            assign_reg!(
                [push!()] = RValue::Local {
                    op: LocOp::Borrow(ast::Mutability::Immutable),
                    arg: *loc as usize
                }
            )
        }

        IB::MutBorrowField(field_ref) => {
            assign_reg!(
                [push!()] = data_op!(DataOp::MutBorrowField(field_ref.clone()), Register(pop!()))
            )
        }

        IB::ImmBorrowField(field_ref) => {
            assign_reg!(
                [push!()] = data_op!(DataOp::ImmBorrowField(field_ref.clone()), Register(pop!()))
            )
        }

        IB::Add => {
            assign_reg!([push!()] = primitive_op!(Op::Add, Register(pop!()), Register(pop!())))
        }

        IB::Sub => {
            let subtraend = pop!();
            let minuend = pop!();
            assign_reg!(
                [push!()] = primitive_op!(Op::Subtract, Register(minuend), Register(subtraend))
            )
        }

        IB::Mul => {
            let multiplier = pop!();
            let multiplicand = pop!();
            assign_reg!(
                [push!()] =
                    primitive_op!(Op::Multiply, Register(multiplicand), Register(multiplier))
            )
        }

        IB::Mod => {
            let divisor = pop!();
            let dividend = pop!();
            assign_reg!(
                [push!()] = primitive_op!(Op::Modulo, Register(dividend), Register(divisor))
            )
        }

        IB::Div => {
            let divisor = pop!();
            let dividend = pop!();
            assign_reg!(
                [push!()] = primitive_op!(Op::Divide, Register(dividend), Register(divisor))
            )
        }

        IB::BitOr => {
            assign_reg!([push!()] = primitive_op!(Op::BitOr, Register(pop!()), Register(pop!())))
        }

        IB::BitAnd => {
            assign_reg!([push!()] = primitive_op!(Op::BitAnd, Register(pop!()), Register(pop!())))
        }

        IB::Xor => {
            assign_reg!([push!()] = primitive_op!(Op::Xor, Register(pop!()), Register(pop!())))
        }

        IB::Or => {
            assign_reg!([push!()] = primitive_op!(Op::Or, Register(pop!()), Register(pop!())))
        }

        IB::And => {
            assign_reg!([push!()] = primitive_op!(Op::And, Register(pop!()), Register(pop!())))
        }

        IB::Not => {
            assign_reg!([push!()] = primitive_op!(Op::Not, Register(pop!())))
        }

        IB::Eq => {
            assign_reg!([push!()] = primitive_op!(Op::Equal, Register(pop!()), Register(pop!())))
        }

        IB::Neq => {
            assign_reg!([push!()] = primitive_op!(Op::NotEqual, Register(pop!()), Register(pop!())))
        }

        IB::Lt => {
            assign_reg!([push!()] = primitive_op!(Op::LessThan, Register(pop!()), Register(pop!())))
        }

        IB::Gt => {
            assign_reg!(
                [push!()] = primitive_op!(Op::GreaterThan, Register(pop!()), Register(pop!()))
            )
        }

        IB::Le => assign_reg!(
            [push!()] = primitive_op!(Op::LessThanOrEqual, Register(pop!()), Register(pop!()))
        ),

        IB::Ge => {
            assign_reg!(
                [push!()] =
                    primitive_op!(Op::GreaterThanOrEqual, Register(pop!()), Register(pop!()))
            )
        }

        IB::Abort => Instruction::Abort(Register(ctxt.pop_register())),

        IB::Nop => Instruction::Nop,

        IB::Shl => {
            assign_reg!(
                [push!()] = primitive_op!(Op::ShiftLeft, Register(pop!()), Register(pop!()))
            )
        }

        IB::Shr => {
            assign_reg!(
                [push!()] = primitive_op!(Op::ShiftRight, Register(pop!()), Register(pop!()))
            )
        }

        IB::VecPack(bx) => {
            let mut args = vec![];
            for _ in 0..bx.1 {
                args.push(Register(pop!()));
            }
            assign_reg!(
                [push!()] = RValue::Data {
                    op: DataOp::VecPack,
                    args,
                }
            )
        }

        IB::VecLen(_rc) => {
            assign_reg!([push!()] = data_op!(DataOp::VecLen, Register(pop!())))
        }

        IB::VecImmBorrow(_rc) => {
            assign_reg!(
                [push!()] = data_op!(DataOp::VecImmBorrow, Register(pop!()), Register(pop!()))
            )
        }

        IB::VecMutBorrow(_rc) => {
            assign_reg!(
                [push!()] = data_op!(DataOp::VecMutBorrow, Register(pop!()), Register(pop!()))
            )
        }

        // TODO check if this is ok for the SSA
        IB::VecPushBack(_rc) => {
            assign_reg!([] = data_op!(DataOp::VecPushBack, Register(pop!()), Register(pop!())))
        }

        IB::VecPopBack(_rc) => {
            assign_reg!([push!()] = data_op!(DataOp::VecPopBack, Register(pop!())))
        }

        IB::VecUnpack(bx) => {
            let rhs = data_op!(DataOp::VecUnpack, Register(pop!()));
            let mut lhs = vec![];
            for _i in 0..bx.1 {
                lhs.push(push!());
            }
            Instruction::AssignReg { rhs, lhs }
        }

        IB::VecSwap(_rc) => {
            let args = make_vec!(3, Register(pop!()));
            Instruction::AssignReg {
                rhs: RValue::Data {
                    op: DataOp::VecSwap,
                    args,
                },
                // TODO check if this is ok for the SSA
                lhs: vec![],
            }
        }

        IB::LdU16(value) => assign_reg!([push!()] = imm!(Value::U16(*value))),

        IB::LdU32(value) => assign_reg!([push!()] = imm!(Value::U32(*value))),

        IB::LdU256(_bx) => assign_reg!([push!()] = imm!(Value::U256(*(*_bx)))),

        IB::CastU16 => {
            assign_reg!([push!()] = primitive_op!(Op::CastU16, Register(pop!())))
        }

        IB::CastU32 => {
            assign_reg!([push!()] = primitive_op!(Op::CastU32, Register(pop!())))
        }

        IB::CastU256 => {
            assign_reg!([push!()] = primitive_op!(Op::CastU256, Register(pop!())))
        }

        IB::PackVariant(bx) => {
            let args = make_vec!(bx.variant.fields.0.len(), Register(pop!()));
            Instruction::AssignReg {
                lhs: vec![push!()],
                rhs: RValue::Data {
                    op: DataOp::PackVariant,
                    args,
                },
            }
        }
        IB::UnpackVariant(bx) => {
            let rhs = RValue::Data {
                op: DataOp::UnpackVariant,
                args: vec![Register(pop!())],
            };
            let lhs = make_vec!(bx.variant.fields.0.len(), push!());
            Instruction::AssignReg { lhs, rhs }
        }

        IB::UnpackVariantImmRef(bx) => {
            let rhs = RValue::Data {
                op: DataOp::UnpackVariantImmRef,
                args: vec![Register(pop!())],
            };
            let lhs = make_vec!(bx.variant.fields.0.len(), push!());
            Instruction::AssignReg { lhs, rhs }
        }

        IB::UnpackVariantMutRef(bx) => {
            let rhs = RValue::Data {
                op: DataOp::UnpackVariant,
                args: vec![Register(pop!())],
            };
            let lhs = make_vec!(bx.variant.fields.0.len(), push!());
            Instruction::AssignReg { lhs, rhs }
        }

        IB::VariantSwitch(jt) => {
            let JumpTableInner::Full(offsets) = &jt.jump_table;
            Instruction::VariantSwitch {
                cases: offsets
                    .iter()
                    .map(|offset| *offset as usize)
                    .collect::<Vec<_>>(),
            }
        }

        // ******** DEPRECATED BYTECODES ********
        IB::MutBorrowGlobalDeprecated(_bx) => Instruction::NotImplemented(format!("{:?}", op)),
        IB::ImmBorrowGlobalDeprecated(_bx) => Instruction::NotImplemented(format!("{:?}", op)),
        IB::ExistsDeprecated(_bx) => Instruction::NotImplemented(format!("{:?}", op)),
        IB::MoveFromDeprecated(_bx) => Instruction::NotImplemented(format!("{:?}", op)),
        IB::MoveToDeprecated(_bx) => Instruction::NotImplemented(format!("{:?}", op)),
    }
}
