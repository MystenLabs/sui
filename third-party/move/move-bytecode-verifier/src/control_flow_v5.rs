// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module implements a checker to verify control flow in bytecode version 5 and below. The
//! following properties are ensured:
//! - All forward jumps do not enter into the middle of a loop
//! - All "breaks" (forward, loop-exiting jumps) go to the "end" of the loop
//! - All "continues" (back jumps in a loop) are only to the current loop
use crate::verifier::VerifierConfig;
use move_binary_format::{
    errors::{PartialVMError, PartialVMResult},
    file_format::{Bytecode, CodeOffset, CodeUnit, FunctionDefinitionIndex},
    safe_unwrap,
};
use move_core_types::vm_status::StatusCode;
use std::{collections::HashSet, convert::TryInto};

pub fn verify(
    verifier_config: &VerifierConfig,
    current_function_opt: Option<FunctionDefinitionIndex>,
    code: &CodeUnit,
) -> PartialVMResult<()> {
    let current_function = current_function_opt.unwrap_or(FunctionDefinitionIndex(0));

    // check fallthrough
    verify_fallthrough(current_function, &code.code)?;

    // check jumps
    let context = &ControlFlowVerifier {
        current_function,
        code: &code.code,
    };
    let labels = instruction_labels(context);
    check_jumps(verifier_config, context, labels)
}

fn verify_fallthrough(
    current_function: FunctionDefinitionIndex,
    code: &Vec<Bytecode>,
) -> PartialVMResult<()> {
    // Check to make sure that the bytecode vector ends with a branching instruction.
    match code.last() {
        None => Err(PartialVMError::new(StatusCode::EMPTY_CODE_UNIT)),
        Some(last) if !last.is_unconditional_branch() => {
            Err(PartialVMError::new(StatusCode::INVALID_FALL_THROUGH)
                .at_code_offset(current_function, (code.len() - 1) as CodeOffset))
        }
        Some(_) => Ok(()),
    }
}

#[derive(Clone, Copy)]
enum Label {
    Loop { last_continue: u16 },
    Code,
}

struct ControlFlowVerifier<'a> {
    current_function: FunctionDefinitionIndex,
    code: &'a Vec<Bytecode>,
}

impl<'a> ControlFlowVerifier<'a> {
    fn code(&self) -> impl Iterator<Item = (CodeOffset, &'a Bytecode)> {
        self.code
            .iter()
            .enumerate()
            .map(|(idx, instr)| (idx.try_into().unwrap(), instr))
    }

    fn labeled_code<'b: 'a>(
        &self,
        labels: &'b [Label],
    ) -> impl Iterator<Item = (CodeOffset, &'a Bytecode, &'b Label)> {
        self.code()
            .zip(labels)
            .map(|((i, instr), lbl)| (i, instr, lbl))
    }

    fn error(&self, status: StatusCode, offset: CodeOffset) -> PartialVMError {
        PartialVMError::new(status).at_code_offset(self.current_function, offset)
    }
}

fn instruction_labels(context: &ControlFlowVerifier) -> Vec<Label> {
    let mut labels: Vec<Label> = (0..context.code.len()).map(|_| Label::Code).collect();
    let mut loop_continue = |loop_idx: CodeOffset, last_continue: CodeOffset| {
        labels[loop_idx as usize] = Label::Loop { last_continue }
    };
    for (i, instr) in context.code() {
        match instr {
            // Back jump/"continue"
            Bytecode::Branch(prev) | Bytecode::BrTrue(prev) | Bytecode::BrFalse(prev)
                if is_back_edge(i, *prev) =>
            {
                loop_continue(*prev, i)
            }
            _ => (),
        }
    }
    labels
}

// Ensures the invariant:
//   - All forward jumps do not enter into the middle of a loop
//   - All "breaks" go to the "end" of the loop
//   - All back jumps are only to the current loop
//   - Nested loops do not exceed a given depth
fn check_jumps(
    verifier_config: &VerifierConfig,
    context: &ControlFlowVerifier,
    labels: Vec<Label>,
) -> PartialVMResult<()> {
    // All back jumps are only to the current loop
    check_continues(context, &labels)?;
    // All "breaks" go to the "end" of the loop
    check_breaks(context, &labels)?;

    let loop_depth = count_loop_depth(&labels);

    // All forward jumps do not enter into the middle of a loop
    check_no_loop_splits(context, &labels, &loop_depth)?;
    // Nested loops do not exceed a given depth
    check_loop_depth(verifier_config, context, &labels, &loop_depth)
}

fn check_code<
    F: FnMut(&Vec<(CodeOffset, CodeOffset)>, CodeOffset, &Bytecode) -> PartialVMResult<()>,
>(
    context: &ControlFlowVerifier,
    labels: &[Label],
    mut check: F,
) -> PartialVMResult<()> {
    let mut loop_stack: Vec<(CodeOffset, CodeOffset)> = vec![];
    for (cur_instr, instr, label) in context.labeled_code(labels) {
        // Add loop to stack
        if let Label::Loop { last_continue } = label {
            loop_stack.push((cur_instr, *last_continue));
        }

        check(&loop_stack, cur_instr, instr)?;

        // Pop if last continue
        match instr {
            // Back jump/"continue"
            Bytecode::Branch(target) | Bytecode::BrTrue(target) | Bytecode::BrFalse(target)
                if is_back_edge(cur_instr, *target) =>
            {
                let (_cur_loop_head, last_continue) = safe_unwrap!(loop_stack.last());
                if cur_instr == *last_continue {
                    loop_stack.pop();
                }
            }
            _ => (),
        }
    }
    Ok(())
}

fn is_back_edge(cur_instr: CodeOffset, target_instr: CodeOffset) -> bool {
    target_instr <= cur_instr
}

// All back jumps are only to the current loop
fn check_continues(context: &ControlFlowVerifier, labels: &[Label]) -> PartialVMResult<()> {
    check_code(context, labels, |loop_stack, cur_instr, instr| {
        match instr {
            // Back jump/"continue"
            Bytecode::Branch(target) | Bytecode::BrTrue(target) | Bytecode::BrFalse(target)
                if is_back_edge(cur_instr, *target) =>
            {
                let (cur_loop_head, _last_continue) = safe_unwrap!(loop_stack.last());
                if target != cur_loop_head {
                    // Invalid back jump. Cannot back jump outside of the current loop
                    Err(context.error(StatusCode::INVALID_LOOP_CONTINUE, cur_instr))
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    })
}

fn check_breaks(context: &ControlFlowVerifier, labels: &[Label]) -> PartialVMResult<()> {
    check_code(context, labels, |loop_stack, cur_instr, instr| {
        match instr {
            // Forward jump/"break"
            Bytecode::Branch(target) | Bytecode::BrTrue(target) | Bytecode::BrFalse(target)
                if !is_back_edge(cur_instr, *target) =>
            {
                match loop_stack.last() {
                    Some((_cur_loop_head, last_continue))
                        if target > last_continue && *target != last_continue + 1 =>
                    {
                        // Invalid loop break. Must break immediately to the instruction after
                        // the last continue
                        Err(context.error(StatusCode::INVALID_LOOP_BREAK, cur_instr))
                    }
                    _ => Ok(()),
                }
            }
            _ => Ok(()),
        }
    })
}

fn check_no_loop_splits(
    context: &ControlFlowVerifier,
    labels: &[Label],
    loop_depth: &[usize],
) -> PartialVMResult<()> {
    let is_break = |loop_stack: &Vec<(CodeOffset, CodeOffset)>, jump_target: CodeOffset| -> bool {
        match loop_stack.last() {
            None => false,
            Some((_cur_loop_head, last_continue)) => jump_target > *last_continue,
        }
    };
    check_code(context, labels, |loop_stack, i, instr| {
        match instr {
            // Forward jump/"break"
            Bytecode::Branch(j) | Bytecode::BrTrue(j) | Bytecode::BrFalse(j)
                if *j > i && !is_break(loop_stack, *j) =>
            {
                let j = *j;
                let before_depth = loop_depth[i as usize];
                let after_depth = match &labels[j as usize] {
                    Label::Loop { .. } => loop_depth[j as usize] - 1,
                    Label::Code => loop_depth[j as usize],
                };
                if before_depth != after_depth {
                    // Invalid forward jump. Entered the middle of a loop
                    Err(context.error(StatusCode::INVALID_LOOP_SPLIT, i))
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    })
}

fn check_loop_depth(
    verifier_config: &VerifierConfig,
    context: &ControlFlowVerifier,
    labels: &[Label],
    loop_depth: &[usize],
) -> PartialVMResult<()> {
    let max_depth = match verifier_config.max_loop_depth {
        Some(depth) => depth,
        None => return Ok(()),
    };
    check_code(context, labels, |_loop_stack, i, _instr| {
        if loop_depth[i as usize] > max_depth {
            return Err(context.error(StatusCode::LOOP_MAX_DEPTH_REACHED, i));
        }
        Ok(())
    })
}

// Only called after continues are verified, so we can assume that loops are well nested
fn count_loop_depth(labels: &[Label]) -> Vec<usize> {
    let last_continues: HashSet<CodeOffset> = labels
        .iter()
        .filter_map(|label| match label {
            Label::Loop { last_continue } => Some(*last_continue),
            Label::Code => None,
        })
        .collect();
    let mut count = 0;
    let mut counts = vec![];
    for (idx, label) in labels.iter().enumerate() {
        if let Label::Loop { .. } = label {
            count += 1
        }
        counts.push(count);
        if last_continues.contains(&idx.try_into().unwrap()) {
            count -= 1;
        }
    }
    counts
}
