// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::stackless::ast::{BasicBlock, Function, Instruction, Operand, RValue, Var};

use std::collections::BTreeMap;

struct Env {
    immediates: BTreeMap<Var, Operand>,
}

pub fn optimize(function: &mut Function) {
    let mut env = Env {
        immediates: BTreeMap::new(),
    };

    let basic_blocks = function.basic_blocks.iter_mut();
    basic_blocks.for_each(|(_, bb)| {
        inline_block(bb, &mut env);
    });
}

fn inline_block(block: &mut BasicBlock, env: &mut Env) {
    let instructions = std::mem::take(&mut block.instructions);
    let instructions = instructions
        .into_iter()
        .flat_map(|instr| process_instruction(instr, env))
        .collect::<Vec<_>>();
    assert!(std::mem::replace(&mut block.instructions, instructions).is_empty());
}

fn process_instruction(mut inst: Instruction, env: &mut Env) -> Option<Instruction> {
    match &mut inst {
        Instruction::Assign { lhs, rhs } if matches!(lhs[..], [_]) => {
            match rhs {
                RValue::Operand(imm @ Operand::Immediate(_)) => {
                    let register = lhs[0].clone();
                    env.immediates.insert(register, imm.clone());
                    None
                }
                RValue::Operand(Operand::Var(var)) => {
                    if let Some(imm) = env.immediates.remove(var) {
                        let _ = std::mem::replace(rhs, RValue::Operand(imm));
                    }
                    Some(inst)
                }
                RValue::Operand(Operand::Constant(_)) => Some(inst),
                // Substituting constants and immediates in RValue
                RValue::Primitive { op: _, args } | RValue::Call { function: _, args } => {
                    for arg in args {
                        if let Operand::Var(var) = arg {
                            if let Some(imm) = env.immediates.remove(var) {
                                let _ = std::mem::replace(arg, imm);
                            }
                        }
                    }
                    Some(inst)
                }
            }
        }
        // Substituting constants and immediates in return instruction
        Instruction::Return(operands) => {
            for operand in operands {
                if let Operand::Var(var) = operand {
                    if let Some(imm) = env.immediates.remove(var) {
                        let _ = std::mem::replace(operand, imm);
                    }
                }
            }
            Some(inst)
        }
        _ => Some(inst),
    }
}
