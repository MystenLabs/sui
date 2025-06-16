use crate::stackless::ast::{
    BasicBlock, Function, Instruction, Operand, RValue, Var
};

use std::{collections::BTreeMap, vec};

struct Env {
    immediates: BTreeMap<Var, Operand>,
}

pub fn inline_immediates(function: &mut Function) {
    let mut env = Env {
        immediates: BTreeMap::new(),
    };

    let basic_blocks = function.basic_blocks.iter_mut();
    basic_blocks.for_each(|(_, bb)| {
        inline_block(bb, &mut env);
    });
}

fn inline_block(
    block: &mut BasicBlock,
    env: &mut Env,
) {
    let instructions = std::mem::take(&mut block.instructions);
    let instructions = instructions.iter().flat_map(|instr| process_instruction(instr, env)).collect::<Vec<_>>();
    println!("New instructions for block {}: {:?}", block.label, instructions);
    debug_assert!(std::mem::replace(&mut block.instructions, instructions).is_empty());
}

fn process_instruction(
    inst: &Instruction,
    env: &mut Env,
) -> Option<Instruction>{
    match inst {
        Instruction::Assign { lhs, rhs } => {
            match rhs {
                RValue::Operand(operand) => {
                    let register = lhs
                        .last()
                        .expect("Register expected in constant operand Assign")
                        .clone();
                    match operand {
                        // Looking for an immediate to be inlined
                        Operand::Immediate(_) => {
                            env.immediates.insert(register, operand.clone());
                            None
                        }
                        // Matching a variable to be substituted
                        Operand::Var(var) => {
                            if env.immediates.contains_key(&var) {
                                Some(Instruction::Assign { lhs: lhs.to_vec(), rhs: RValue::Operand(env.immediates.remove(&var).expect("Immediate not found")) })
                            }
                            else {
                                Some(inst.clone())
                            }
                        }
                        Operand::Constant(_) => {
                            Some(inst.clone())
                        }
                    }
                    
                }
                // Substituting constants and immediates in RValue
                RValue::Primitive { op, args } => {
                    let mut new_args = vec![];
                    for arg in args {
                        match arg {
                            Operand::Var(var) => {
                                if env.immediates.contains_key(&var) {
                                    new_args.push(env.immediates.remove(&var).expect("Immediate not found"));
                                }
                                else {
                                    new_args.push(Operand::Var(var.clone()));
                                }
                            }
                            _ => {new_args.push(arg.clone());}
                        }
                    }
                    Some(Instruction::Assign { lhs: lhs.to_vec(), rhs: RValue::Primitive { op: op.clone(), args: new_args } })
                },
                _ => Some(inst.clone())
            }
        }
        // Substituting constants and immediates in return instruction
        Instruction::Return(operands) => {
            let mut new_operands = vec![];
            for operand in operands {
                match operand {
                    Operand::Var(var) => {
                        if env.immediates.contains_key(&var) {
                            new_operands.push(env.immediates.remove(&var).expect("Immediate not found"));
                        }
                        else {
                            new_operands.push(Operand::Var(var.clone()));
                        }
                    }
                    _ => {new_operands.push(operand.clone());}
                }
            }
            Some(Instruction::Return(new_operands))
        }
        _ => Some(inst.clone())
    }
}