// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfg::{ControlFlowGraph, StacklessControlFlowGraph},
    stackless::{
        ast::{self, BasicBlock, Instruction, Operand::Var, PrimitiveOp, RValue, Var::Register},
        context::Context,
    },
};

use move_binary_format::{normalized as N, normalized::Bytecode as IB};
use move_model_2::{model::Module, source_kind::SourceKind};
use move_symbol_pool::Symbol;

use anyhow::Ok;
use std::{
    collections::BTreeMap,
    fmt::{Debug, Display},
    hash::Hash,
};

use super::ast::Immediate;

// -------------------------------------------------------------------------------------------------
// Stackless Bytecode Translation
// -------------------------------------------------------------------------------------------------

// TODO: Define a `Module` in the `AST` and fill it in (?)
pub(crate) fn module<K: SourceKind>(module: Module<K>) -> anyhow::Result<ast::Module> {
    let mut context = Context::new();
    let module = module.compiled();
    let name = *module.name();
    let _module_address = module.address();
    // println!("\nModule: {} ({})", module_name, module_address);

    let mut functions = BTreeMap::new();

    for fun in module.functions.values() {
        context.var_counter.reset();
        let function_name = fun.name;
        functions.insert(function_name, function(&mut context, fun)?);
    }

    let module = ast::Module { name, functions };

    Ok(module)
}

pub(crate) fn function(
    ctxt: &mut Context,
    function: &N::Function<Symbol>,
) -> anyhow::Result<ast::Function> {
    let name = function.name;
    // println!("\nFunction: {}", function_name);
    let code = function.code();

    let cfg = StacklessControlFlowGraph::new(code, function.jump_tables());
    let mut block_id = cfg.entry_block_id();

    let mut bbs = vec![];
    while cfg.next_block(block_id).is_some() {
        let blk_start = cfg.block_start(block_id);
        let blk_end = cfg.block_end(block_id);
        let code_range = &code[blk_start as usize..blk_end as usize];
        let block_instructions = code_range
            .iter()
            .map(|op| bytecode(ctxt, op))
            .collect::<Result<Vec<_>, _>>()?;
        let bb = BasicBlock::from_instructions(block_id as usize, block_instructions);
        bbs.push(bb);
        block_id = cfg.next_block(block_id).unwrap();
    }
    let function = ast::Function {
        name,
        basic_blocks: bbs,
    };

    Ok(function)
}

pub(crate) fn bytecode<S: Hash + Eq + Display + Debug>(
    ctxt: &mut Context,
    op: &IB<S>,
) -> anyhow::Result<Instruction> {
    match op {

        IB::Pop => {
            // TODO: how to handle Pop?
            let inst = Instruction::Assign { 
                lhs: vec![Register(ctxt.var_counter.next())], 
                rhs: RValue::Immediate(Immediate::Empty)
            };
            Ok(inst)
        }
        
        IB::Ret => {
            // TODO: This should look at the function's return arity and grab values off the
            // logical stack accordingly
            let inst = Instruction::Return(vec![Register(ctxt.var_counter.last())]);
            Ok(inst)
        }

        IB::BrTrue(code_offset) => {
            let inst = Instruction::Branch {
                condition: Register(ctxt.get_var_counter().last()),
                // TODO: get the instruction counter, from context maybe?
                then_label: 0 as usize,
                else_label: *code_offset as usize 
            };
            Ok(inst)
        }
        IB::BrFalse(code_offset) => {
            let inst = Instruction::Branch { 
                // TODO: should we swap the then and else labels?
                condition: Register(ctxt.get_var_counter().last()),
                // TODO: get the instruction counter, from context maybe?
                then_label: 0 as usize,
                else_label: *code_offset as usize 
            };
            Ok(inst)
        }
        IB::Branch(code_offset) => {
            let inst = Instruction::Branch {
                condition: Register(ctxt.get_var_counter().last()),
                // TODO: get the instruction counter, from context maybe?
                then_label: *code_offset as usize,
                else_label: *code_offset as usize 
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
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::CastU64 => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::CastU128 => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::LdConst(_const_ref) => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
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

        IB::CopyLoc(loc) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::CopyLoc,
                    args: vec![Var(Register((*loc).into()))],
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
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        
        
        IB::Pack(_struct_ref) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Pack,
                    // TODO get how many args are needed
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            Ok(inst)
        }

        IB::Unpack(_bx) => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
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
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::MutBorrowLoc(_local_index) => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::ImmBorrowLoc(_local_index) => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
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
        
                // Mul
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

        
        // Mod
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
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::Nop => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
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
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::VecLen(_rc) => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::VecImmBorrow(_rc) => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::VecMutBorrow(_rc) => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::VecPushBack(_rc) => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::VecPopBack(_rc) => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::VecUnpack(_bx) => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::VecSwap(_rc) => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
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
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::CastU32 => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::CastU256 => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::PackVariant(_bx) => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::UnpackVariant(_bx) => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::UnpackVariantImmRef(_bx) => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::UnpackVariantMutRef(_bx) => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::VariantSwitch(_jt) => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        // ******** DEPRECATED BYTECODES ********
        IB::MutBorrowGlobalDeprecated(_bx) => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::ImmBorrowGlobalDeprecated(_bx) => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::ExistsDeprecated(_bx) => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::MoveFromDeprecated(_bx) => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
        IB::MoveToDeprecated(_bx) => {
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
    }
}
