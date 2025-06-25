// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::stackless::ast::{BasicBlock, Function, Instruction, RValue, RegId, Trivial};

use std::collections::BTreeMap;

struct Env {
    immediates: BTreeMap<RegId, Trivial>,
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
        Instruction::AssignReg { lhs, rhs } if matches!(lhs[..], [_]) => match rhs {
            RValue::Trivial(imm @ Trivial::Immediate(_)) => {
                let register = lhs[0];
                env.immediates.insert(register, imm.clone());
                None
            }
            RValue::Trivial(Trivial::Register(var)) => {
                if let Some(imm) = env.immediates.remove(var) {
                    let _ = std::mem::replace(rhs, RValue::Trivial(imm));
                }
                Some(inst)
            }
            RValue::Primitive { op: _, args }
            | RValue::Data { op: _, args }
            | RValue::Call { function: _, args } => {
                for arg in args {
                    if let Trivial::Register(var) = arg {
                        if let Some(imm) = env.immediates.remove(var) {
                            let _ = std::mem::replace(arg, imm);
                        }
                    }
                }
                Some(inst)
            }
            RValue::Constant(_) | RValue::Local { op: _, arg: _ } => Some(inst),
        },
        // Substituting immediates in return instruction
        Instruction::Return(operands) => {
            for operand in operands {
                if let Trivial::Register(var) = operand {
                    if let Some(imm) = env.immediates.remove(var) {
                        let _ = std::mem::replace(operand, imm);
                    }
                }
            }
            Some(inst)
        }
        Instruction::StoreLoc { loc: _, value } => {
            if let Trivial::Register(var) = value {
                if let Some(imm) = env.immediates.remove(var) {
                    let _ = std::mem::replace(value, imm);
                }
            }
            Some(inst)
        }
        _ => Some(inst),
    }
}
