use crate::stackless::ast::{
    BasicBlock, Function, Instruction, Label, Operand, RValue,
    Value, Var,
};

use std::{collections::BTreeMap, vec};

struct Env {
    constants: BTreeMap<Var, Operand>,
    immediates: BTreeMap<Var, Operand>,
    to_be_removed: Vec<(Label, Label)>,
}

pub fn inline_constants_and_immediates(function: &mut Function) {
    let mut env = Env {
        constants: BTreeMap::new(),
        immediates: BTreeMap::new(),
        to_be_removed: vec![],
    };

    let basic_blocks = function.basic_blocks.iter_mut();
    basic_blocks.for_each(|(_, bb)| {
        inline_block(bb, &mut env);
    });

    for (bb_label, key) in env.to_be_removed {
        let bb = function
            .basic_blocks
            .get_mut(&bb_label)
            .expect("Basic block not found");
        bb.instructions.remove(&key);
    }
}

fn inline_block<'a>(
    block: &'a mut BasicBlock,
    env: & mut Env,
) {
    block.instructions.iter_mut().for_each(|(index, inst)| {
        process_instruction(
            block.label,
            *index,
            inst,
            env
        );
    });
}

fn process_instruction<'a>(
    bb_label: Label,
    index: Label,
    inst: &'a mut Instruction,
    env: &mut Env,
) {
    match inst {
        Instruction::Assign { lhs, rhs } => {
            match rhs {
                RValue::Operand(operand) => {
                    let register = lhs
                        .last()
                        .expect("Register expected in constant operand Assign")
                        .clone();
                    match operand {
                        // Looking for a constant to be inlined
                        Operand::Constant(_) => {
                            env.constants.insert(register, operand.clone());
                            env.to_be_removed.push((bb_label, index));
                        }
                        // Looking for an immediate to be inlined
                        Operand::Immediate(_) => {
                            env.immediates.insert(register, operand.clone());
                            env.to_be_removed.push((bb_label, index));
                        }
                        // Matching a variable to be substituted
                        Operand::Var(var) => {
                            if env.constants.contains_key(var) {
                                *operand = env.constants.get(var).expect("Constant not found").clone();
                            }
                            else if env.immediates.contains_key(var) {
                                *operand = env.immediates.get(var).expect("Immediate not found").clone();
                            }
                        }
                    }
                    
                }
                // Substituting constants and immediates in RValue
                RValue::Primitive { op, args } => match op {
                    _ => {
                        for arg in args {
                            match arg {
                                Operand::Var(var) => {
                                    if env.constants.contains_key(var) {
                                        *arg = env.constants.get(var).expect("Constant not found").clone();
                                    }
                                    else if env.immediates.contains_key(var) {
                                        *arg = env.immediates.get(var).expect("Immediate not found").clone();
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                },
                _ => {}
            }
        }
        // Substituting constants and immediates in return instruction
        Instruction::Return(operands) => {
            for operand in operands {
                match operand {
                    Operand::Var(var) => {
                        if env.constants.contains_key(var) {
                            *operand = env.constants.get(var).expect("Constant not found").clone();
                        }
                        else if env.immediates.contains_key(var) {
                            *operand = env.immediates.get(var).expect("Immediate not found").clone();
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}