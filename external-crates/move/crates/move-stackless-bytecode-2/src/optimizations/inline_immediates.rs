// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::ast::{BasicBlock, Function, Instruction, RValue, RegId, Trivial};

use std::collections::BTreeMap;

struct Env {
    immediates: BTreeMap<RegId, Trivial>,
}

pub fn optimize(function: &mut Function) {
    let basic_blocks = function.basic_blocks.iter_mut();
    basic_blocks.for_each(|(_, bb)| {
        let mut env = Env {
            immediates: BTreeMap::new(),
        };
        inline_block(&mut env, bb);
    });
}

fn inline_block(env: &mut Env, block: &mut BasicBlock) {
    let instructions = std::mem::take(&mut block.instructions);
    let instructions = instructions
        .into_iter()
        .flat_map(|instr| process_instruction(env, instr))
        .collect::<Vec<_>>();
    assert!(std::mem::replace(&mut block.instructions, instructions).is_empty());
}

fn process_instruction(env: &mut Env, mut inst: Instruction) -> Option<Instruction> {
    match &mut inst {
        Instruction::AssignReg { lhs, rhs } if matches!(lhs[..], [_]) => {
            rvalue(env, rhs);
            match rhs {
                RValue::Trivial(imm @ Trivial::Immediate(_)) => {
                    let register = &lhs[0];
                    env.immediates.insert(register.name, imm.clone());
                    None
                }
                RValue::Trivial(Trivial::Register(reg)) => {
                    if let Some(imm) = env.immediates.remove(&reg.name) {
                        assert!(env.immediates.insert(lhs[0].name, imm).is_none());
                        None
                    } else {
                        Some(inst)
                    }
                }
                RValue::Call { .. }
                | RValue::Primitive { .. }
                | RValue::Data { .. }
                | RValue::Local { .. }
                | RValue::Constant(..) => Some(inst),
            }
        }
        Instruction::AssignReg { lhs: _, rhs } => {
            rvalue(env, rhs);
            Some(inst)
        }
        Instruction::Return(operands) => {
            for operand in operands {
                if let Trivial::Register(reg) = operand
                    && let Some(imm) = env.immediates.remove(&reg.name)
                {
                    let _ = std::mem::replace(operand, imm);
                }
            }
            Some(inst)
        }
        Instruction::StoreLoc { loc: _, value } => {
            if let Trivial::Register(reg) = value
                && let Some(imm) = env.immediates.remove(&reg.name)
            {
                let _ = std::mem::replace(value, imm);
            }
            Some(inst)
        }
        Instruction::JumpIf {
            condition,
            then_label: _,
            else_label: _,
        } => {
            if let Trivial::Register(reg) = condition
                && let Some(imm) = env.immediates.remove(&reg.name)
            {
                let _ = std::mem::replace(condition, imm);
            }
            Some(inst)
        }
        Instruction::Abort(trivial) => {
            if let Trivial::Register(reg) = trivial
                && let Some(imm) = env.immediates.remove(&reg.name)
            {
                let _ = std::mem::replace(trivial, imm);
            }
            Some(inst)
        }
        Instruction::Jump(_) | Instruction::Nop | Instruction::NotImplemented(_) => Some(inst),
        Instruction::VariantSwitch {
            condition,
            enum_: _,
            variants: _,
            labels: _,
        } => {
            if let Trivial::Register(reg) = condition
                && let Some(imm) = env.immediates.remove(&reg.name)
            {
                let _ = std::mem::replace(condition, imm);
            }
            Some(inst)
        }
        Instruction::Drop(reg) => {
            if let Some(_imm) = env.immediates.remove(&reg.name) {
                None
            } else {
                Some(inst)
            }
        }
    }
}

fn rvalue(env: &mut Env, rv: &mut RValue) {
    match rv {
        RValue::Trivial(Trivial::Register(reg)) => {
            if let Some(imm) = env.immediates.remove(&reg.name) {
                let _ = std::mem::replace(rv, RValue::Trivial(imm));
            }
        }
        RValue::Primitive { op: _, args }
        | RValue::Data { op: _, args }
        | RValue::Call { target: _, args } => {
            for arg in args {
                if let Trivial::Register(reg) = arg
                    && let Some(imm) = env.immediates.remove(&reg.name)
                {
                    let _ = std::mem::replace(arg, imm);
                }
            }
        }
        RValue::Trivial(Trivial::Immediate(_))
        | RValue::Constant(_)
        | RValue::Local { op: _, arg: _ } => {}
    }
}
