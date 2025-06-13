// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfg::{ControlFlowGraph, StacklessControlFlowGraph},
    stackless::{
        ast::{
            self, BasicBlock, Instruction, Operand,
            Operand::{Constant, Var},
            PrimitiveOp, RValue, Value,
            Var::Local,
        },
        context::Context,
        optimization::inline_constants,
    },
};

use move_binary_format::{
    file_format::JumpTableInner, normalized as N, normalized::Bytecode as IB,
};
use move_core_types::{account_address::AccountAddress, u256::U256};

use move_model_2::{
    model::{Model as Model2, Module, Package},
    source_kind::SourceKind,
};
use move_symbol_pool::Symbol;

use std::{collections::BTreeMap, result::Result::Ok, vec};

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

    println!(
        "\nPackage: {} ({})",
        package_name.unwrap_or(Symbol::from("Package name not found")),
        package_address
    );

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
            .collect::<Result<Vec<Vec<_>>, _>>()?
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(i, inst)| {
                let label = blk_start as usize + i;
                (label, inst)
            })
            .collect::<BTreeMap<_, _>>();
        
        let label = block_id as usize;
        let bb = BasicBlock::from_instructions(label, block_instructions);
        if !ctxt.logical_stack.is_empty() {
            panic!("Logical stack not empty: {:#?}\n{}", ctxt.logical_stack, bb);
        }
        basic_blocks.insert(label, bb);
    }

    let mut function = ast::Function { name, basic_blocks };

    inline_constants(&mut function);

    Ok(function)
}

// If([stack_top], 28, 25)
//
// => BrFalse(25)
// => Branch(28)
//
// ~~>
//
// If([stack_top], [next_instr], 25)
// Jump(28)
//
// ==>
//
// If([stack_top], 28, 25)
//
// If(reg_3, 22, 25)

pub(crate) fn bytecode<K: SourceKind>(
    ctxt: &mut Context<'_, K>,
    op: &IB<Symbol>,
    pc: usize,
    function: &N::Function<Symbol>,
) -> anyhow::Result<Vec<Instruction>> {
    match op {
        IB::Pop => {
            ctxt.pop_register();
            Ok(vec![])
        }

        // push(0)    [reg_0: 0]
        // push(10)   [reg_1: 10, reg_0: 0]
        // push(20)   [reg_2: 20, reg_1: 10, reg_0: 0]
        // pop        [reg_1: 10, reg_0: 0]
        // ret(2)     (reg_1, reg_0)
        IB::Ret => {
            // TODO: check if this needs to be reversed? 
            let returned_vars = function
                .return_
                .iter()
                .map(|_| Operand::Var(ctxt.pop_register()))
                .collect::<Vec<_>>();
            let inst = Instruction::Return(returned_vars);
            Ok(vec![inst])
        }

        IB::BrTrue(code_offset) => {
            let inst = Instruction::JumpIf {
                condition: ctxt.pop_register(),
                then_label: *code_offset as usize,
                else_label: pc + 1,
            };
            Ok(vec![inst])
        }
        IB::BrFalse(code_offset) => {
            let inst = Instruction::JumpIf {
                condition: ctxt.pop_register(),
                then_label: pc + 1,
                else_label: *code_offset as usize,
            };
            Ok(vec![inst])
        }
        IB::Branch(code_offset) => {
            let inst = Instruction::Jump(*code_offset as usize);
            Ok(vec![inst])
        }
        IB::LdU8(value) => {
            let inst = Instruction::Assign {
                rhs: RValue::Operand(Operand::Immediate(Value::U8(*value))),
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::LdU64(value) => {
            let inst = Instruction::Assign {
                rhs: RValue::Operand(Operand::Immediate(Value::U64(*value))),
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::LdU128(bx) => {
            let inst = Instruction::Assign {
                rhs: RValue::Operand(Operand::Immediate(Value::U128(*(*bx)))),
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::CastU8 => {
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::CastU8,
                    args: vec![Var(ctxt.pop_register())],
                },
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::CastU64 => {
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::CastU64,
                    args: vec![Var(ctxt.pop_register())],
                },
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::CastU128 => {
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::CastU128,
                    args: vec![Var(ctxt.pop_register())],
                },
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::LdConst(const_ref) => {
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::LdConst,
                    args: vec![Constant(deserialize_constant(const_ref))],
                },
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::LdTrue => {
            let inst = Instruction::Assign {
                rhs: RValue::Operand(Operand::Immediate(Value::True)),
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::LdFalse => {
            let inst = Instruction::Assign {
                rhs: RValue::Operand(Operand::Immediate(Value::False)),
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::CopyLoc(loc) => {
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::CopyLoc,
                    args: vec![Var(Local(*loc as usize))],
                },
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::MoveLoc(loc) => {
            let inst = Instruction::Assign {
                lhs: vec![ctxt.push_register()],
                rhs: RValue::Operand(Operand::Var(Local(*loc as usize))),
            };
            Ok(vec![inst])
        }

        IB::StLoc(loc) => {
            let inst = Instruction::Assign {
                lhs: vec![Local(*loc as usize)],
                rhs: RValue::Operand(Operand::Var(ctxt.pop_register())),
            };
            Ok(vec![inst])
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

            let args = function
                .parameters
                .iter()
                .map(|_| Var(ctxt.pop_register()))
                .collect::<Vec<_>>();

            let lhs = function
                .return_
                .iter()
                .map(|_| ctxt.push_register())
                .collect::<Vec<_>>();

            let inst = Instruction::Assign {
                lhs,
                rhs: RValue::Call {
                    function: function.name,
                    args,
                },
            };
            Ok(vec![inst])
        }

        IB::Pack(_struct_ref) => {
            let args = _struct_ref
                .struct_
                .fields
                .0
                .iter()
                .map(|_| Var(ctxt.pop_register()))
                .collect::<Vec<_>>();
            let inst = Instruction::Assign {
                lhs: vec![ctxt.push_register()],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Pack,
                    args,
                },
            };
            Ok(vec![inst])
        }

        IB::Unpack(bx) => {
            let rhs = RValue::Primitive {
                op: PrimitiveOp::Unpack,
                args: vec![Var(ctxt.pop_register())],
            };
            let lhs = bx
                .struct_
                .fields
                .0
                .iter()
                .map(|_| ctxt.push_register())
                .collect::<Vec<_>>();
            let inst = Instruction::Assign { rhs, lhs };
            Ok(vec![inst])
        }

        IB::ReadRef => {
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::ReadRef,
                    args: vec![Var(ctxt.pop_register())],
                },
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::WriteRef => {
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::WriteRef,
                    args: vec![Var(ctxt.pop_register()), Var(ctxt.pop_register())],
                },
                lhs: vec![],
            };
            Ok(vec![inst])
        }

        IB::FreezeRef => {
            // TODO check FreezeRef
            let inst = Instruction::Nop;
            Ok(vec![inst])
        }

        IB::MutBorrowLoc(local_index) => {
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::MutBorrowLoc,
                    args: vec![Var(Local(*local_index as usize))],
                },
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::ImmBorrowLoc(local_index) => {
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::ImmBorrowLoc,
                    args: vec![Var(Local(*local_index as usize))],
                },
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::MutBorrowField(_field_ref) => {
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::MutBorrowField,
                    args: vec![Var(ctxt.pop_register())],
                },
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::ImmBorrowField(_field_ref) => {
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::ImmBorrowField,
                    args: vec![Var(ctxt.pop_register())],
                },
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::Add => {
            let args = vec![Var(ctxt.pop_register()), Var(ctxt.pop_register())];
            let inst = Instruction::Assign {
                lhs: vec![ctxt.push_register()],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Add,
                    args,
                },
            };
            Ok(vec![inst])
        }

        IB::Sub => {
            let subtraend = ctxt.pop_register();
            let minuend = ctxt.pop_register();
            let args = vec![Var(minuend), Var(subtraend)];
            let inst = Instruction::Assign {
                lhs: vec![ctxt.push_register()],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Subtract,
                    args,
                },
            };
            Ok(vec![inst])
        }

        IB::Mul => {
            let multiplier = ctxt.pop_register();
            let multiplicand = ctxt.pop_register();
            let args = vec![Var(multiplicand), Var(multiplier)];
            let inst = Instruction::Assign {
                lhs: vec![ctxt.push_register()],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Multiply,
                    args,
                },
            };
            Ok(vec![inst])
        }

        IB::Mod => {
            let divisor = ctxt.pop_register();
            let dividend = ctxt.pop_register();
            let args = vec![Var(dividend), Var(divisor)];
            let inst = Instruction::Assign {
                lhs: vec![ctxt.push_register()],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Modulo,
                    args,
                },
            };
            Ok(vec![inst])
        }

        IB::Div => {
            let divisor = ctxt.pop_register();
            let dividend = ctxt.pop_register();
            let args = vec![Var(dividend), Var(divisor)];
            let inst = Instruction::Assign {
                lhs: vec![ctxt.push_register()],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Divide,
                    args,
                },
            };
            Ok(vec![inst])
        }
        IB::BitOr => {
            let args = vec![Var(ctxt.pop_register()), Var(ctxt.pop_register())];
            let inst = Instruction::Assign {
                lhs: vec![ctxt.push_register()],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::BitOr,
                    args,
                },
            };
            Ok(vec![inst])
        }

        IB::BitAnd => {
            let args = vec![Var(ctxt.pop_register()), Var(ctxt.pop_register())];
            let inst = Instruction::Assign {
                lhs: vec![ctxt.push_register()],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::BitAnd,
                    args,
                },
            };
            Ok(vec![inst])
        }

        IB::Xor => {
            let args = vec![Var(ctxt.pop_register()), Var(ctxt.pop_register())];
            let inst = Instruction::Assign {
                lhs: vec![ctxt.push_register()],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Xor,
                    args,
                },
            };
            Ok(vec![inst])
        }

        IB::Or => {
            let args = vec![Var(ctxt.pop_register()), Var(ctxt.pop_register())];
            let inst = Instruction::Assign {
                lhs: vec![ctxt.push_register()],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Or,
                    args,
                },
            };
            Ok(vec![inst])
        }

        IB::And => {
            let args = vec![Var(ctxt.pop_register()), Var(ctxt.pop_register())];
            let inst = Instruction::Assign {
                lhs: vec![ctxt.push_register()],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::And,
                    args,
                },
            };
            Ok(vec![inst])
        }

        IB::Not => {
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Not,
                    args: vec![Var(ctxt.pop_register())],
                },
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }
        IB::Eq => {
            let args = vec![Var(ctxt.pop_register()), Var(ctxt.pop_register())];
            let inst = Instruction::Assign {
                lhs: vec![ctxt.push_register()],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Equal,
                    args,
                },
            };
            Ok(vec![inst])
        }

        IB::Neq => {
            let args = vec![Var(ctxt.pop_register()), Var(ctxt.pop_register())];
            let inst = Instruction::Assign {
                lhs: vec![ctxt.push_register()],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::NotEqual,
                    args,
                },
            };
            Ok(vec![inst])
        }

        IB::Lt => {
            let args = vec![Var(ctxt.pop_register()), Var(ctxt.pop_register())];
            let inst = Instruction::Assign {
                lhs: vec![ctxt.push_register()],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::LessThan,
                    args,
                },
            };
            Ok(vec![inst])
        }

        IB::Gt => {
            let args = vec![Var(ctxt.pop_register()), Var(ctxt.pop_register())];
            let inst = Instruction::Assign {
                lhs: vec![ctxt.push_register()],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::GreaterThan,
                    args,
                },
            };
            Ok(vec![inst])
        }
        IB::Le => {
            let args = vec![Var(ctxt.pop_register()), Var(ctxt.pop_register())];
            let inst = Instruction::Assign {
                lhs: vec![ctxt.push_register()],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::LessThanOrEqual,
                    args,
                },
            };
            Ok(vec![inst])
        }

        IB::Ge => {
            let args = vec![Var(ctxt.pop_register()), Var(ctxt.pop_register())];
            let inst = Instruction::Assign {
                lhs: vec![ctxt.push_register()],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::GreaterThanOrEqual,
                    args,
                },
            };
            Ok(vec![inst])
        }

        IB::Abort => {
            ctxt.empty_stack();
            let inst = Instruction::Abort;
            Ok(vec![inst])
        }

        IB::Nop => {
            let inst = Instruction::Nop;
            Ok(vec![inst])
        }

        IB::Shl => {
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::ShiftLeft,
                    args: vec![Var(ctxt.pop_register()), Var(ctxt.pop_register())],
                },
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::Shr => {
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::ShiftRight,
                    args: vec![Var(ctxt.pop_register()), Var(ctxt.pop_register())],
                },
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::VecPack(_bx) => {
            let mut args = vec![];
            for _ in 0.._bx.1 {
                args.push(Var(ctxt.pop_register()));
            }
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::VecPack,
                    // VecPack will always take one arg only
                    args,
                },
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::VecLen(_rc) => {
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::VecLen,
                    args: vec![Var(ctxt.pop_register())],
                },
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::VecImmBorrow(_rc) => {
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::VecImmBorrow,
                    args: vec![Var(ctxt.pop_register()), Var(ctxt.pop_register())],
                },
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::VecMutBorrow(_rc) => {
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::VecMutBorrow,
                    args: vec![Var(ctxt.pop_register()), Var(ctxt.pop_register())],
                },
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::VecPushBack(_rc) => {
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::VecPushBack,
                    // TODO check if this is ok for the SSA
                    args: vec![Var(ctxt.pop_register()), Var(ctxt.pop_register())],
                },
                lhs: vec![],
            };
            Ok(vec![inst])
        }

        IB::VecPopBack(_rc) => {
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::VecPopBack,
                    args: vec![Var(ctxt.pop_register())],
                },
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::VecUnpack(bx) => {
            let rhs = RValue::Primitive {
                op: PrimitiveOp::VecUnpack,
                args: vec![Var(ctxt.pop_register())],
            };
            let mut lhs = vec![];
            for _i in 0..bx.1 {
                lhs.push(ctxt.push_register());
            }
            let inst = Instruction::Assign { rhs, lhs };
            Ok(vec![inst])
        }

        IB::VecSwap(_rc) => {
            let args = [0, 1, 2]
                .iter()
                .map(|_| Var(ctxt.pop_register()))
                .collect::<Vec<_>>();
            let inst = Instruction::Assign {
                // TODO  check order of the registers
                rhs: RValue::Primitive {
                    op: PrimitiveOp::VecSwap,
                    args,
                },
                // TODO check if this is ok for the SSA
                lhs: vec![],
            };
            Ok(vec![inst])
        }

        IB::LdU16(value) => {
            let inst = Instruction::Assign {
                rhs: RValue::Operand(Operand::Immediate(Value::U16(*value))),
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }
        IB::LdU32(value) => {
            let inst = Instruction::Assign {
                rhs: RValue::Operand(Operand::Immediate(Value::U32(*value))),
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::LdU256(_bx) => {
            let inst = Instruction::Assign {
                rhs: RValue::Operand(Operand::Immediate(Value::U256(*(*_bx)))),
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::CastU16 => {
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::CastU16,
                    args: vec![Var(ctxt.pop_register())],
                },
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::CastU32 => {
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::CastU32,
                    args: vec![Var(ctxt.pop_register())],
                },
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::CastU256 => {
            let inst = Instruction::Assign {
                rhs: RValue::Primitive {
                    op: PrimitiveOp::CastU256,
                    args: vec![Var(ctxt.pop_register())],
                },
                lhs: vec![ctxt.push_register()],
            };
            Ok(vec![inst])
        }

        IB::PackVariant(bx) => {
            let args = bx
                .variant
                .fields
                .0
                .iter()
                .enumerate()
                .map(|_| Operand::Var(ctxt.pop_register()))
                .collect::<Vec<_>>();
            let inst = Instruction::Assign {
                lhs: vec![ctxt.push_register()],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::PackVariant,
                    args,
                },
            };
            Ok(vec![inst])
        }
        IB::UnpackVariant(bx) => {
            let rhs = RValue::Primitive {
                op: PrimitiveOp::UnpackVariant,
                args: vec![Var(ctxt.pop_register())],
            };
            let lhs = bx
                .variant
                .fields
                .0
                .iter()
                .map(|_| ctxt.push_register())
                .collect::<Vec<_>>();
            let inst = Instruction::Assign { lhs, rhs };
            Ok(vec![inst])
        }

        IB::UnpackVariantImmRef(bx) => {
            let rhs = RValue::Primitive {
                op: PrimitiveOp::UnpackVariantImmRef,
                args: vec![Var(ctxt.pop_register())],
            };
            let lhs = bx
                .variant
                .fields
                .0
                .iter()
                .map(|_| ctxt.push_register())
                .collect::<Vec<_>>();
            let inst = Instruction::Assign { lhs, rhs };
            Ok(vec![inst])
        }

        IB::UnpackVariantMutRef(bx) => {
            let rhs = RValue::Primitive {
                op: PrimitiveOp::UnpackVariant,
                args: vec![Var(ctxt.pop_register())],
            };
            let lhs = bx
                .variant
                .fields
                .0
                .iter()
                .enumerate()
                .map(|_| ctxt.push_register())
                .collect::<Vec<_>>();
            let inst = Instruction::Assign { lhs, rhs };
            Ok(vec![inst])
        }

        IB::VariantSwitch(jt) => {
            let JumpTableInner::Full(offsets) = &jt.jump_table;
            let inst = Instruction::VariantSwitch {
                cases: offsets
                    .iter()
                    .map(|offset| *offset as usize)
                    .collect::<Vec<_>>(),
            };
            Ok(vec![inst])
        }

        // ******** DEPRECATED BYTECODES ********
        IB::MutBorrowGlobalDeprecated(_bx) => {
            Ok(vec![Instruction::NotImplemented(format!("{:?}", op))])
        }
        IB::ImmBorrowGlobalDeprecated(_bx) => {
            Ok(vec![Instruction::NotImplemented(format!("{:?}", op))])
        }
        IB::ExistsDeprecated(_bx) => Ok(vec![Instruction::NotImplemented(format!("{:?}", op))]),
        IB::MoveFromDeprecated(_bx) => Ok(vec![Instruction::NotImplemented(format!("{:?}", op))]),
        IB::MoveToDeprecated(_bx) => Ok(vec![Instruction::NotImplemented(format!("{:?}", op))]),
    }
}

fn deserialize_constant(constant: &N::Constant<Symbol>) -> Value {
    match &constant.type_ {
        N::Type::U8 => {
            Value::U8(bcs::from_bytes::<u8>(&constant.data).unwrap_or_else(|_| {
                panic!("Failed to deserialize U8 constant: {:?}", constant.data)
            }))
        }
        N::Type::U16 => {
            Value::U16(bcs::from_bytes::<u16>(&constant.data).unwrap_or_else(|_| {
                panic!("Failed to deserialize U16 constant: {:?}", constant.data)
            }))
        }
        N::Type::U32 => {
            Value::U32(bcs::from_bytes::<u32>(&constant.data).unwrap_or_else(|_| {
                panic!("Failed to deserialize U32 constant: {:?}", constant.data)
            }))
        }
        N::Type::U64 => {
            Value::U64(bcs::from_bytes::<u64>(&constant.data).unwrap_or_else(|_| {
                panic!("Failed to deserialize U64 constant: {:?}", constant.data)
            }))
        }
        N::Type::U128 => {
            Value::U128(bcs::from_bytes::<u128>(&constant.data).unwrap_or_else(|_| {
                panic!("Failed to deserialize U128 constant: {:?}", constant.data)
            }))
        }
        N::Type::U256 => {
            Value::U256(bcs::from_bytes::<U256>(&constant.data).unwrap_or_else(|_| {
                panic!("Failed to deserialize U256 constant: {:?}", constant.data)
            }))
        }
        N::Type::Address => Value::Address(
            bcs::from_bytes::<AccountAddress>(&constant.data).unwrap_or_else(|_| {
                panic!(
                    "Failed to deserialize Address constant: {:?}",
                    constant.data
                )
            }),
        ),
        N::Type::Bool => match bcs::from_bytes::<bool>(&constant.data) {
            Ok(value) => {
                if value {
                    Value::True
                } else {
                    Value::False
                }
            }
            Err(_) => panic!("Failed to deserialize Bool constant: {:?}", constant.data),
        },
        N::Type::Vector(bx) => {
            // TODO finish to implement nested vectors
            handle_vec(&bx, &constant.data)
        }
        N::Type::Datatype(_)
        | N::Type::Reference(_, _)
        | N::Type::Signer
        | N::Type::TypeParameter(_) => {
            Value::NotImplemented(format!("Unsupported constant type: {:?}", constant.type_))
        }
    }
}

fn handle_vec(ty: &Box<N::Type<Symbol>>, data: &Vec<u8>) -> Value {
    match &**ty {
        N::Type::U8 => {
            let data = bcs::from_bytes::<Vec<u8>>(data)
                .unwrap_or_else(|err| {
                    panic!(
                        "Failed to deserialize U8 vector: {:?}, Error: {}",
                        data, err
                    )
                })
                .iter()
                .map(|e| Value::U8(*e))
                .collect();
            Value::Vector(data)
        }
        N::Type::U16 => {
            let data = bcs::from_bytes::<Vec<u16>>(data)
                .unwrap_or_else(|_| panic!("Failed to deserialize U16 vector: {:?}", data))
                .iter()
                .map(|e| Value::U16(*e))
                .collect();
            Value::Vector(data)
        }
        N::Type::U32 => {
            let data = bcs::from_bytes::<Vec<u32>>(data)
                .unwrap_or_else(|_| panic!("Failed to deserialize U32 vector: {:?}", data))
                .iter()
                .map(|e| Value::U32(*e))
                .collect();
            Value::Vector(data)
        }
        N::Type::U64 => {
            let data = bcs::from_bytes::<Vec<u64>>(data)
                .unwrap_or_else(|_| panic!("Failed to deserialize U64 vector: {:?}", data))
                .iter()
                .map(|e| Value::U64(*e))
                .collect();
            Value::Vector(data)
        }
        N::Type::U128 => {
            let data = bcs::from_bytes::<Vec<u128>>(data)
                .unwrap_or_else(|_| panic!("Failed to deserialize U128 vector: {:?}", data))
                .iter()
                .map(|e| Value::U128(*e))
                .collect();
            Value::Vector(data)
        }
        N::Type::U256 => {
            let data = bcs::from_bytes::<Vec<U256>>(data)
                .unwrap_or_else(|_| panic!("Failed to deserialize U256 vector: {:?}", data))
                .iter()
                .map(|e| Value::U256(*e))
                .collect();
            Value::Vector(data)
        }
        N::Type::Address => {
            let data = bcs::from_bytes::<Vec<AccountAddress>>(data)
                .unwrap_or_else(|_| panic!("Failed to deserialize Address vector: {:?}", data))
                .iter()
                .map(|e| Value::Address(*e))
                .collect();
            Value::Vector(data)
        }
        N::Type::Bool => {
            let data = bcs::from_bytes::<Vec<bool>>(data)
                .unwrap_or_else(|_| panic!("Failed to deserialize Bool vector: {:?}", data))
                .iter()
                .map(|e| if *e { Value::True } else { Value::False })
                .collect();
            Value::Vector(data)
        }
        N::Type::Vector(bx) => {
            //TODO finish to implement nested vectors
            Value::NotImplemented(format!("Nested vector type not yet supported: {:?}", bx))
        }
        N::Type::Datatype(_)
        | N::Type::Reference(_, _)
        | N::Type::Signer
        | N::Type::TypeParameter(_) => {
            // These types are not supported for immediate values
            Value::NotImplemented(format!("Unsupported vector type: {:?}", ty))
        }
    }
}
