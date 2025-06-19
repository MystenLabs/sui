// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfg::{ControlFlowGraph, StacklessControlFlowGraph},
    stackless::{
        ast::{
            self, BasicBlock, Instruction,
            Operand::{Constant, Immediate, Var},
            RValue, Value,
            Var::Local,
        },
        context::Context,
        optimizations::optimize,
    },
};

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
            basic_blocks: BTreeMap::new(),
        });
    }
    let cfg = StacklessControlFlowGraph::new(code, function.jump_tables());

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

    let mut function = ast::Function { name, basic_blocks };

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
    use ast::PrimitiveOp as Op;

    // macro_rules! assign {
    //     ($rhs:expr $(, $lhs:expr)* $(,)?) => {{
    //         Instruction::Assign {
    //             lhs: vec![$($lhs),*],
    //             rhs: $rhs,
    //         }
    //     }};
    // }

    macro_rules! assign {
        ([$($lhs:expr),*] = $rhs:expr) => {{
            let rhs = $rhs;
            Instruction::Assign {
                lhs: vec![$($lhs),*],
                rhs: rhs,
            }
        }};
    }

    macro_rules! imm {
        ($val:expr) => {
            RValue::Operand(Immediate($val))
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
            let returned_vars = make_vec!(function.return_.len(), Var(pop!()));
            let inst = Instruction::Return(returned_vars);
            inst
        }

        IB::BrTrue(code_offset) => {
            let inst = Instruction::JumpIf {
                condition: pop!(),
                then_label: *code_offset as usize,
                else_label: pc + 1,
            };
            inst
        }

        IB::BrFalse(code_offset) => {
            let inst = Instruction::JumpIf {
                condition: pop!(),
                then_label: pc + 1,
                else_label: *code_offset as usize,
            };
            inst
        }

        IB::Branch(code_offset) => {
            let inst = Instruction::Jump(*code_offset as usize);
            inst
        }

        IB::LdU8(value) => assign!([push!()] = imm!(Value::U8(*value))),

        IB::LdU64(value) => assign!([push!()] = imm!(Value::U64(*value))),

        IB::LdU128(bx) => assign!([push!()] = imm!(Value::U128(*(*bx)))),

        IB::CastU8 => {
            assign!([push!()] = primitive_op!(Op::CastU8, Var(pop!())))
        }

        IB::CastU64 => {
            assign!([push!()] = primitive_op!(Op::CastU64, Var(pop!())))
        }

        IB::CastU128 => {
            assign!([push!()] = primitive_op!(Op::CastU128, Var(pop!())))
        }

        IB::LdConst(const_ref) => {
            assign!([push!()] = RValue::Operand(Constant(const_ref.data.clone())))
        }

        IB::LdTrue => assign!([push!()] = imm!(Value::True)),

        IB::LdFalse => assign!([push!()] = imm!(Value::False)),

        IB::CopyLoc(loc) => {
            assign!([push!()] = primitive_op!(Op::CopyLoc, Var(Local(*loc as usize))))
        }

        IB::MoveLoc(loc) => {
            assign!([push!()] = primitive_op!(Op::MoveLoc, Var(Local(*loc as usize))))
        }

        IB::StLoc(loc) => {
            assign!([Local(*loc as usize)] = RValue::Operand(Var(pop!())))
        }

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

            let args = make_vec!(function.parameters.len(), Var(pop!()));

            let lhs = make_vec!(function.return_.len(), push!());

            Instruction::Assign {
                lhs,
                rhs: RValue::Call {
                    function: function.name,
                    args,
                },
            }
        }

        IB::Pack(_struct_ref) => {
            let args = make_vec!(_struct_ref.struct_.fields.0.len(), Var(pop!()));
            assign!([push!()] = RValue::Primitive { op: Op::Pack, args })
        }

        IB::Unpack(bx) => {
            let rhs = RValue::Primitive {
                op: Op::Unpack,
                args: vec![Var(pop!())],
            };
            let lhs = make_vec!(bx.struct_.fields.0.len(), push!());
            let inst = Instruction::Assign { rhs, lhs };
            inst
        }

        IB::ReadRef => {
            assign!([push!()] = primitive_op!(Op::ReadRef, Var(pop!())))
        }

        // TODO check if this is ok for the SSA
        IB::WriteRef => assign!([] = primitive_op!(Op::WriteRef, Var(pop!()), Var(pop!()))),

        IB::FreezeRef => Instruction::Nop,

        IB::MutBorrowLoc(local_index) => {
            assign!([push!()] = primitive_op!(Op::MutBorrowLoc, Var(Local(*local_index as usize))))
        }

        IB::ImmBorrowLoc(local_index) => {
            assign!([push!()] = primitive_op!(Op::ImmBorrowLoc, Var(Local(*local_index as usize))))
        }

        IB::MutBorrowField(_field_ref) => {
            assign!([push!()] = primitive_op!(Op::MutBorrowField, Var(pop!())))
        }

        IB::ImmBorrowField(_field_ref) => {
            assign!([push!()] = primitive_op!(Op::ImmBorrowField, Var(pop!())))
        }

        IB::Add => assign!([push!()] = primitive_op!(Op::Add, Var(pop!()), Var(pop!()))),

        IB::Sub => {
            let subtraend = pop!();
            let minuend = pop!();
            assign!([push!()] = primitive_op!(Op::Subtract, Var(minuend), Var(subtraend)))
        }

        IB::Mul => {
            let multiplier = pop!();
            let multiplicand = pop!();
            assign!([push!()] = primitive_op!(Op::Multiply, Var(multiplicand), Var(multiplier)))
        }

        IB::Mod => {
            let divisor = pop!();
            let dividend = pop!();
            assign!([push!()] = primitive_op!(Op::Modulo, Var(dividend), Var(divisor)))
        }

        IB::Div => {
            let divisor = pop!();
            let dividend = pop!();
            assign!([push!()] = primitive_op!(Op::Divide, Var(dividend), Var(divisor)))
        }

        IB::BitOr => assign!([push!()] = primitive_op!(Op::BitOr, Var(pop!()), Var(pop!()))),

        IB::BitAnd => assign!([push!()] = primitive_op!(Op::BitAnd, Var(pop!()), Var(pop!()))),

        IB::Xor => assign!([push!()] = primitive_op!(Op::Xor, Var(pop!()), Var(pop!()))),

        IB::Or => assign!([push!()] = primitive_op!(Op::Or, Var(pop!()), Var(pop!()))),

        IB::And => assign!([push!()] = primitive_op!(Op::And, Var(pop!()), Var(pop!()))),

        IB::Not => {
            assign!([push!()] = primitive_op!(Op::Not, Var(pop!())))
        }

        IB::Eq => assign!([push!()] = primitive_op!(Op::Equal, Var(pop!()), Var(pop!()))),

        IB::Neq => assign!([push!()] = primitive_op!(Op::NotEqual, Var(pop!()), Var(pop!()))),

        IB::Lt => assign!([push!()] = primitive_op!(Op::LessThan, Var(pop!()), Var(pop!()))),

        IB::Gt => assign!([push!()] = primitive_op!(Op::GreaterThan, Var(pop!()), Var(pop!()))),

        IB::Le => assign!([push!()] = primitive_op!(Op::LessThanOrEqual, Var(pop!()), Var(pop!()))),

        IB::Ge => {
            assign!([push!()] = primitive_op!(Op::GreaterThanOrEqual, Var(pop!()), Var(pop!())))
        }

        IB::Abort => {
            ctxt.empty_stack();
            Instruction::Abort
        }

        IB::Nop => Instruction::Nop,

        IB::Shl => assign!([push!()] = primitive_op!(Op::ShiftLeft, Var(pop!()), Var(pop!()))),

        IB::Shr => assign!([push!()] = primitive_op!(Op::ShiftRight, Var(pop!()), Var(pop!()))),

        IB::VecPack(_bx) => {
            let mut args = vec![];
            for _ in 0.._bx.1 {
                args.push(Var(pop!()));
            }
            assign!(
                [push!()] = RValue::Primitive {
                    op: Op::VecPack,
                    args,
                }
            )
        }

        IB::VecLen(_rc) => {
            assign!([push!()] = primitive_op!(Op::VecLen, Var(pop!())))
        }

        IB::VecImmBorrow(_rc) => {
            assign!([push!()] = primitive_op!(Op::VecImmBorrow, Var(pop!()), Var(pop!())))
        }

        IB::VecMutBorrow(_rc) => {
            assign!([push!()] = primitive_op!(Op::VecMutBorrow, Var(pop!()), Var(pop!())))
        }

        // TODO check if this is ok for the SSA
        IB::VecPushBack(_rc) => {
            assign!([] = primitive_op!(Op::VecPushBack, Var(pop!()), Var(pop!())))
        }

        IB::VecPopBack(_rc) => assign!([push!()] = primitive_op!(Op::VecPopBack, Var(pop!()))),

        IB::VecUnpack(bx) => {
            let rhs = primitive_op!(Op::VecUnpack, Var(pop!()));
            let mut lhs = vec![];
            for _i in 0..bx.1 {
                lhs.push(push!());
            }
            let inst = Instruction::Assign { rhs, lhs };
            inst
        }

        IB::VecSwap(_rc) => {
            let args = make_vec!(3, Var(pop!()));
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: Op::VecSwap,
                    args,
                },
                // TODO check if this is ok for the SSA
                lhs: vec![],
            };
            inst
        }

        IB::LdU16(value) => assign!([push!()] = imm!(Value::U16(*value))),

        IB::LdU32(value) => assign!([push!()] = imm!(Value::U32(*value))),

        IB::LdU256(_bx) => assign!([push!()] = imm!(Value::U256(*(*_bx)))),

        IB::CastU16 => {
            assign!([push!()] = primitive_op!(Op::CastU16, Var(pop!())))
        }

        IB::CastU32 => {
            assign!([push!()] = primitive_op!(Op::CastU32, Var(pop!())))
        }

        IB::CastU256 => {
            assign!([push!()] = primitive_op!(Op::CastU256, Var(pop!())))
        }

        IB::PackVariant(bx) => {
            let args = make_vec!(bx.variant.fields.0.len(), Var(pop!()));
            let inst = Instruction::Assign {
                lhs: vec![push!()],
                rhs: RValue::Primitive {
                    op: Op::PackVariant,
                    args,
                },
            };
            inst
        }
        IB::UnpackVariant(bx) => {
            let rhs = RValue::Primitive {
                op: Op::UnpackVariant,
                args: vec![Var(pop!())],
            };
            let lhs = make_vec!(bx.variant.fields.0.len(), push!());
            let inst = Instruction::Assign { lhs, rhs };
            inst
        }

        IB::UnpackVariantImmRef(bx) => {
            let rhs = RValue::Primitive {
                op: Op::UnpackVariantImmRef,
                args: vec![Var(pop!())],
            };
            let lhs = make_vec!(bx.variant.fields.0.len(), push!());
            let inst = Instruction::Assign { lhs, rhs };
            inst
        }

        IB::UnpackVariantMutRef(bx) => {
            let rhs = RValue::Primitive {
                op: Op::UnpackVariant,
                args: vec![Var(pop!())],
            };
            let lhs = make_vec!(bx.variant.fields.0.len(), push!());
            let inst = Instruction::Assign { lhs, rhs };
            inst
        }

        IB::VariantSwitch(jt) => {
            let JumpTableInner::Full(offsets) = &jt.jump_table;
            let inst = Instruction::VariantSwitch {
                cases: offsets
                    .iter()
                    .map(|offset| *offset as usize)
                    .collect::<Vec<_>>(),
            };
            inst
        }

        // ******** DEPRECATED BYTECODES ********
        IB::MutBorrowGlobalDeprecated(_bx) => Instruction::NotImplemented(format!("{:?}", op)),
        IB::ImmBorrowGlobalDeprecated(_bx) => Instruction::NotImplemented(format!("{:?}", op)),
        IB::ExistsDeprecated(_bx) => Instruction::NotImplemented(format!("{:?}", op)),
        IB::MoveFromDeprecated(_bx) => Instruction::NotImplemented(format!("{:?}", op)),
        IB::MoveToDeprecated(_bx) => Instruction::NotImplemented(format!("{:?}", op)),
    }
}
