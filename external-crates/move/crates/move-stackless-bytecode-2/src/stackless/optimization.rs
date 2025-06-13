use crate::{
    stackless::{
        ast::{
            BasicBlock, Function, Instruction, Label, Operand,
            Operand::{Constant},
            PrimitiveOp, RValue, Value, Var as AVar,
        },
    },
};

use std::{collections::BTreeMap, vec};

pub fn inline_constants(function: &mut Function) {
    let mut constants = BTreeMap::new();
    let mut to_be_removed = vec![];
    // let mut operands_to_change = BTreeMap::new();
    let basic_blocks = function.basic_blocks.iter_mut();
    basic_blocks.for_each(|(_, bb)| {
        inline_block(bb, &mut constants, &mut to_be_removed);
    });

    for (bb_label, key) in to_be_removed {
        let bb = function
            .basic_blocks
            .get_mut(&bb_label)
            .expect("Basic block not found");
        bb.instructions.remove(&key);
    }
}

fn inline_block<'a>(
    block: &'a mut BasicBlock,
    constants: &mut BTreeMap<AVar, Value>,
    to_be_removed: &mut Vec<(Label, Label)>,
) {
    inline_instructions(
        block.label,
        &mut block.instructions,
        constants,
        to_be_removed,
    );
}

fn inline_instructions<'a>(
    block_label: Label,
    instructions: &'a mut BTreeMap<Label, Instruction>,
    constants: &'_ mut BTreeMap<AVar, Value>,
    to_be_removed: &'_ mut Vec<(Label, Label)>,
) {
    instructions.iter_mut().for_each(|(index, instruction)| {
        process_instruction(block_label, *index, instruction, constants, to_be_removed)
    });
}

fn process_instruction<'a>(
    bb_label: Label,
    index: Label,
    inst: &'a mut Instruction,
    constants: &'_ mut BTreeMap<AVar, Value>,
    to_be_removed: &'_ mut Vec<(Label, Label)>,
) {
    match inst {
        Instruction::Assign { lhs, rhs } => {
            match rhs {
                // Looking for constants in the right-hand side of the assignment
                RValue::Constant(val) => {
                    let register = lhs
                        .last()
                        .expect("Register expected in constant Assign")
                        .clone();
                    constants.insert(register, val.clone());
                    to_be_removed.push((bb_label, index));
                }
                RValue::Operand(Operand::Constant(val)) => {
                    let register = lhs
                        .last()
                        .expect("Register expected in constant operand Assign")
                        .clone();
                    constants.insert(register, val.clone());
                    to_be_removed.push((bb_label, index));
                }
                RValue::Operand(Operand::Immediate(val)) => {
                    let register = lhs
                        .last()
                        .expect("Register expected in immediate operand Assign")
                        .clone();
                    constants.insert(register, val.clone());
                    to_be_removed.push((bb_label, index));
                }

                // Looking for vars to be replaced with constants
                RValue::Operand(op) => {
                    match op {
                        Operand::Var(var) => {
                            if constants.contains_key(var) {
                                //TODO check if we can replace the operand directly
                                // operands_to_change.insert(var, op);
                                let new_op = Operand::Constant(
                                    constants.get(var).expect("Constant not found").clone(),
                                );
                                *op = new_op;
                            }
                        }
                        _ => {}
                    }
                }
                RValue::Primitive { op, args } => match op {
                    PrimitiveOp::LdConst => {
                        if let Some(Constant(constant)) = args.first() {
                            let register = lhs
                                .last()
                                .expect("Register expected in primitive Assign")
                                .clone();
                            constants.insert(register, constant.clone());
                            to_be_removed.push((bb_label, index));
                        }
                    }
                    _ => {
                        for arg in args {
                            match arg {
                                Operand::Var(var) => {
                                    if constants.contains_key(var) {
                                        let new_op = Operand::Constant(
                                            constants.get(var).expect("Constant not found").clone(),
                                        );
                                        *arg = new_op;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                },
                _ => {
                    println!("Unhandled RValue in Assign: {rhs:?}");
                }
            }
        }

        Instruction::Return(_vars) => {
            for var in _vars {
                match var {
                    Operand::Var(v) => {
                        if constants.contains_key(v) {
                            let new_op = Operand::Constant(
                                constants.get(v).expect("Constant not found").clone(),
                            );
                            *var = new_op;
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}
