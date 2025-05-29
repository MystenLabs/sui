// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::stackless::{
    ast::{
        self, Instruction, Operand::Var, PrimitiveOp, RValue, Var::Register,
    },
    context::Context,
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
        // TODO: Check we did not clobber an old function?
        functions.insert(function_name, function(&mut context, &fun)?);
    }

    let module = ast::Module { name, functions };

    Ok(module)
}

// TODO: Use the CFG to generate basic blocks instead
pub(crate) fn function(
    ctxt: &mut Context,
    function: &N::Function<Symbol>,
) -> anyhow::Result<ast::Function> {
    let name = function.name;
    // println!("\nFunction: {}", function_name);
    let code = function.code();

    // TODO call the CFG and get blocks, then translate hose instead.

    let instructions = code
        .into_iter()
        .map(|op| bytecode(ctxt, op))
        .collect::<Result<Vec<_>, _>>()?;

    let function = ast::Function { name, instructions };

    Ok(function)
}

pub(crate) fn bytecode<S: Hash + Eq + Display + Debug>(
    ctxt: &mut Context,
    op: &IB<S>,
) -> anyhow::Result<Instruction> {
    match op {
        // MoveLoc
        IB::MoveLoc(loc) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::MoveLoc,
                    args: vec![Var(Register((*loc).into()))],
                },
            };
            return Ok(inst);
        }

        // CopyLoc
        IB::CopyLoc(loc) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::CopyLoc,
                    args: vec![Var(Register((*loc).into()))],
                },
            };
            return Ok(inst);
        }

        // StoreLoc
        IB::StLoc(loc) => {
            if ctxt.var_counter.current() < 1 {
                panic!("Not enough variables to perform StLoc operation");
            }
            let inst = Instruction::Assign {
                lhs: vec![Register((*loc).into())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::StoreLoc,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            return Ok(inst);
        }

        // ImmBorrowField
        IB::ImmBorrowField(_field_ref) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::ImmBorrowField,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            return Ok(inst);
        }

        // MutBorrowField
        IB::MutBorrowField(_field_ref) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::MutBorrowField,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            return Ok(inst);
        }

        // Pack
        IB::Pack(_struct_ref) => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Pack,
                    // TODO get how many args are needed
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            return Ok(inst);
        }

        // ReadRef
        IB::ReadRef => {
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::ReadRef,
                    args: vec![Var(Register(ctxt.var_counter.last()))],
                },
            };
            return Ok(inst);
        }

        // WriteRef
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
            return Ok(inst);
        }

        // Add
        IB::Add => {
            if ctxt.var_counter.current() < 2 {
                panic!("Not enough variables to perform Add operation");
            }
            let rhs = Var(Register(ctxt.var_counter.prev()));
            let lhs = Var(Register(ctxt.var_counter.last()));
            ctxt.var_counter.increment();
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Add,
                    args: vec![lhs, rhs],
                },
            };
            return Ok(inst);
        }

        // Mul
        IB::Mul => {
            if ctxt.var_counter.current() < 2 {
                panic!("Not enough variables to perform Mul operation");
            }
            let rhs = Var(Register(ctxt.var_counter.prev()));
            let lhs = Var(Register(ctxt.var_counter.last()));
            ctxt.var_counter.increment();
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Multiply,
                    args: vec![lhs, rhs],
                },
            };
            return Ok(inst);
        }

        // Mod
        IB::Mod => {
            if ctxt.var_counter.current() < 2 {
                panic!("Not enough variables to perform Mod operation");
            }
            let rhs = Var(Register(ctxt.var_counter.prev()));
            let lhs = Var(Register(ctxt.var_counter.last()));
            ctxt.var_counter.increment();
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Primitive {
                    op: PrimitiveOp::Modulo,
                    args: vec![lhs, rhs],
                },
            };
            return Ok(inst);
        }

        // LdU64
        IB::LdU64(value) => {
            // let newReg = value;
            let inst = Instruction::Assign {
                lhs: vec![Register(ctxt.var_counter.next())],
                rhs: RValue::Immediate(Immediate::U64(*value)),
            };
            return Ok(inst);
        }

        // Ret
        IB::Ret => {
            // TODO: This should look at the function's return arity and grab values off the
            // logical stack accordingly
            let inst = Instruction::Return(vec![Register(ctxt.var_counter.last())]);
            return Ok(inst);
        }

        _ => {
            // Handle other bytecode operations as needed
            // println!("Not implemented: {:?}", op);
            Ok(Instruction::NotImplemented(format!("{:?}", op)))
        }
    }
}
