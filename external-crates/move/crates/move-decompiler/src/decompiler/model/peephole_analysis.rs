// Copyright (c) Verichains
// Copyright © Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

use move_model::model::FunctionEnv;

use move_stackless_bytecode::{
    function_target::FunctionData,
    function_target_pipeline::{FunctionTargetProcessor, FunctionTargetsHolder},
    stackless_bytecode::{Bytecode, Operation},
    stackless_control_flow_graph::StacklessControlFlowGraph,
};
use std::collections::BTreeMap;

pub struct PeepHoleProcessor {
    max_loop: usize,
}

impl PeepHoleProcessor {
    pub fn new(max_loop: usize) -> Box<Self> {
        Box::new(Self { max_loop })
    }
}

impl FunctionTargetProcessor for PeepHoleProcessor {
    fn process(
        &self,
        _targets: &mut FunctionTargetsHolder,
        func_env: &FunctionEnv,
        mut data: FunctionData,
        _scc_opt: Option<&[FunctionEnv]>,
    ) -> FunctionData {
        if func_env.is_native() {
            // Nothing to do
            return data;
        }

        let code = std::mem::take(&mut data.code);

        let code = Self::transform_code(code, self.max_loop);

        data.code = code;
        data
    }

    fn name(&self) -> String {
        "peephole".to_string()
    }
}

impl PeepHoleProcessor {
    // return true if this operation produces no side-effect - so we can
    // optimize it (ie. remove it)
    fn no_side_effect(oper: &Operation) -> bool {
        match oper {
            Operation::Function(..)
            | Operation::WriteRef
            | Operation::Unpack(..)
            | Operation::Pack(..)
            | Operation::MoveTo(_, _, _)
            | Operation::MoveFrom(_, _, _) => false,

            Operation::Exists(_, _, _)
            | Operation::BorrowLoc
            | Operation::BorrowField(_, _, _, _)
            | Operation::BorrowGlobal(_, _, _)
            | Operation::GetField(_, _, _, _)
            | Operation::Destroy
            | Operation::ReadRef
            | Operation::CastU8
            | Operation::CastU16
            | Operation::CastU32
            | Operation::CastU64
            | Operation::CastU128
            | Operation::Not
            | Operation::Add
            | Operation::Sub
            | Operation::Mul
            | Operation::Div
            | Operation::Mod
            | Operation::BitOr
            | Operation::BitAnd
            | Operation::Xor
            | Operation::Shl
            | Operation::Shr
            | Operation::Lt
            | Operation::Gt
            | Operation::Le
            | Operation::Ge
            | Operation::Or
            | Operation::And
            | Operation::Eq
            | Operation::Neq
            | Operation::CastU256 => true,

            // specification opcode - dont touch it
            Operation::OpaqueCallBegin(..) | Operation::OpaqueCallEnd(..) => false,
            Operation::TraceLocal(_)
            | Operation::Uninit
            | Operation::FreezeRef
            | Operation::PackVariant(..)
            | Operation::UnpackVariant(..)
            | Operation::TraceReturn(_)
            | Operation::TraceAbort
            | Operation::GetGlobal(..)
            | Operation::UnpackRef
            | Operation::PackRef
            | Operation::UnpackRefDeep
            | Operation::PackRefDeep
            | Operation::Stop
            | Operation::IsParent(_, _)
            | Operation::WriteBack(_, _)
            | Operation::Havoc(..) => false,
        }
    }

    // remove all Destroy insn that destroys the destination of the instruction right above it
    fn remove_destroy(code: Vec<Bytecode>) -> (Vec<Bytecode>, bool) {
        let mut changed = false;
        let cfg = StacklessControlFlowGraph::new_forward(&code);

        // save offsets of all basic blocks
        let block_offsets: Vec<_> = cfg
            .blocks()
            .iter()
            .filter_map(|&num| cfg.instr_indexes(num).and_then(|mut iter| iter.next()))
            .collect();

        // println!("block offsets = {:?}", block_offsets);

        // Transform code.
        let mut new_code = vec![];

        for (code_offset, insn) in code.iter().enumerate() {
            // println!(">>> {code_offset}: {:?}", insn);
            if let Bytecode::Call(_, _, oper, srcs, _) = insn {
                let offset: u16 = code_offset as u16;

                if !block_offsets.contains(&offset) && matches!(oper, Operation::Destroy) {
                    // This is NOT the first instruction in a basic block.
                    if let Some(last_insn) = new_code.last_mut() {
                        match last_insn {
                            // pattern: Load(AttrId(8), 8, U64(1));  Call(AttrId(9), [], Drop, [8], None)
                            Bytecode::Load(_, dest, _) | Bytecode::Assign(_, dest, _, _)
                                if srcs.contains(dest) =>
                            {
                                // We need to remove the previous insn as well
                                new_code.pop();

                                changed = true;

                                // Continue, so we do not take this insn - effectively removing it
                                continue;
                            }

                            // Call(AttrId(6), [7], Add, [5, 6], None); Call(AttrId(7), [], Drop, [7], None)
                            Bytecode::Call(_, dest, last_inst_oper, _, _)
                                if dest.len() > 0 && dest == srcs =>
                            {
                                if Self::no_side_effect(&last_inst_oper) {
                                    new_code.pop();
                                } else {
                                    dest.clear();
                                }

                                changed = true;

                                // Continue, so we do not take this insn - effectively removing it
                                continue;
                            }

                            _ => {}
                        }
                    }
                }
            }

            // This instruction should be included
            new_code.push(insn.clone());
        }

        (new_code, changed)
    }

    // find all the JUMP right after LABEL
    fn remove_hops(code: Vec<Bytecode>) -> (Vec<Bytecode>, bool) {
        let mut changed = false;

        let mut label_map = BTreeMap::new();

        // find all the pair LABEL & JUMP as consecutive instructions
        for (code_offset, insn) in code.iter().enumerate() {
            if let Bytecode::Jump(_, new_target) = insn {
                if code_offset > 0 {
                    // let offset: u16 = code_offset as u16;

                    let last_insn = &code[code_offset - 1];
                    match last_insn {
                        // pattern: Label(AttrId(30), Label(5)); Jump(AttrId(33), Label(6))
                        Bytecode::Label(_, old_target) => {
                            if old_target != new_target {
                                label_map.insert(old_target.clone(), new_target.clone());
                            }
                        }

                        _ => {}
                    }
                }
            }
        }

        // patch all the branch instructions jumping to old target, to go to new target
        let mut new_code = vec![];

        for insn in code {
            match insn {
                Bytecode::Branch(id, then_label, else_label, cond) => {
                    if label_map.contains_key(&then_label) || label_map.contains_key(&else_label) {
                        changed = true;
                    }

                    let then_new = label_map.get(&then_label).unwrap_or(&then_label);
                    let else_new = label_map.get(&else_label).unwrap_or(&else_label);
                    let insn_new = Bytecode::Branch(id, *then_new, *else_new, cond);

                    new_code.push(insn_new);
                }

                Bytecode::Jump(id, label) => {
                    if label_map.contains_key(&label) {
                        changed = true;
                    }

                    let label_new = label_map.get(&label).unwrap_or(&label);
                    let insn_new = Bytecode::Jump(id, *label_new);

                    new_code.push(insn_new);
                }

                _ => {
                    new_code.push(insn.clone());
                }
            }
        }

        (new_code, changed)
    }

    // Change all the branches with the same else & then branch with a simple jump
    // Branch(AttrId(65), Label(13), Label(13), 23) -> Jump(AttrId(65), Label(13))
    fn patch_branch(code: Vec<Bytecode>) -> (Vec<Bytecode>, bool) {
        let mut changed = false;

        let mut new_code = vec![];

        for insn in &code {
            match insn {
                Bytecode::Branch(id, then_label, else_label, _) if then_label == else_label => {
                    let insn_new = Bytecode::Jump(*id, *then_label);

                    new_code.push(insn_new);

                    changed = true;
                }

                _ => {
                    new_code.push(insn.clone());
                }
            }
        }

        (new_code, changed)
    }

    // find consecutive labels, then change all of them to the last one
    // 17: Label(AttrId(30), Label(5))
    // 18: Label(AttrId(34), Label(4))
    // 19: Label(AttrId(37), Label(6))
    fn patch_sequence_labels(code: Vec<Bytecode>) -> (Vec<Bytecode>, bool) {
        let mut changed = false;

        let mut label_map = BTreeMap::new();

        let mut sequence_length = 0;
        let mut start: usize = 0;

        for (i, insn) in code.iter().enumerate() {
            if let Bytecode::Label(..) = insn {
                if sequence_length == 0 {
                    start = i;
                }
                sequence_length += 1;
            } else {
                if sequence_length > 1 {
                    if let Bytecode::Label(_, label_to) = code[i - 1] {
                        for index in start..i - 1 {
                            if let Bytecode::Label(_, label_from) = code[index] {
                                label_map.insert(label_from, label_to);
                            }
                        }
                    }
                }

                // new sequence
                sequence_length = 0;
            }
        }

        // corner case: the last insn can be a label
        if sequence_length > 1 {
            let i = code.len() - 1;
            if let Bytecode::Label(_, label_to) = code[i] {
                for index in start..i {
                    if let Bytecode::Label(_, label_from) = code[index] {
                        label_map.insert(label_from, label_to);
                    }
                }
            }
        }

        // change labels in all the jumps in a sequence to the last label
        let mut new_code = vec![];

        for insn in &code {
            match insn {
                Bytecode::Branch(id, then_label, else_label, cond) => {
                    if label_map.contains_key(then_label) || label_map.contains_key(else_label) {
                        changed = true;
                    }

                    let then_new = label_map.get(&then_label).unwrap_or(&then_label);
                    let else_new = label_map.get(&else_label).unwrap_or(&else_label);
                    let insn_new = Bytecode::Branch(*id, *then_new, *else_new, *cond);

                    new_code.push(insn_new);
                }

                Bytecode::Jump(id, label) => {
                    if label_map.contains_key(label) {
                        changed = true;
                    }

                    let label_new = label_map.get(&label).unwrap_or(&label);
                    let insn_new = Bytecode::Jump(*id, *label_new);

                    new_code.push(insn_new);
                }

                Bytecode::Label(_, label) => {
                    // if this label has mapping, remove it
                    if label_map.contains_key(label) {
                        // skip this instruction
                        changed = true;
                    } else {
                        new_code.push(insn.clone());
                    }
                }

                _ => {
                    new_code.push(insn.clone());
                }
            }
        }

        (new_code, changed)
    }

    // for consecutive Jumps, only keep the first Jump, but remove all the Jump right after it
    // 9: Jump(AttrId(13), Label(2)); 10: Jump(AttrId(17), Label(2))
    fn remove_sequence_jumps(code: Vec<Bytecode>) -> (Vec<Bytecode>, bool) {
        let mut changed = false;

        let mut new_code = vec![];

        for insn in &code {
            if let Bytecode::Jump(..) = insn {
                if let Some(last_insn) = new_code.last() {
                    match last_insn {
                        Bytecode::Jump(..) => {
                            // 2 consecutive Jumps, so do not keep this instruction
                            changed = true;
                            continue;
                        }

                        _ => {}
                    }
                }
            }

            // This instruction should be included
            new_code.push(insn.clone());
        }

        (new_code, changed)
    }

    // remove all JUMP code that jumps to the label right after it
    fn remove_jump(code: Vec<Bytecode>) -> (Vec<Bytecode>, bool) {
        let mut changed = false;

        let mut new_code = vec![];

        for insn in &code {
            if let Bytecode::Label(_, label) = insn {
                if let Some(last_insn) = new_code.last() {
                    match last_insn {
                        // Jump(AttrId(29), Label(6)); Label(AttrId(37), Label(6))
                        Bytecode::Jump(_, target) if target == label => {
                            // remove the previous Jump
                            new_code.pop();
                            changed = true;
                        }

                        _ => {}
                    }
                }
            }

            // This instruction should be included
            new_code.push(insn.clone());
        }

        (new_code, changed)
    }

    fn remove_labels(code: Vec<Bytecode>) -> (Vec<Bytecode>, bool) {
        let mut changed = false;

        // find all used labels
        let mut used_labels = vec![];

        for insn in &code {
            match insn {
                Bytecode::Branch(_, then_label, else_label, _) => {
                    used_labels.push(then_label);
                    used_labels.push(else_label);
                }

                Bytecode::Jump(_, label) => {
                    used_labels.push(label);
                }

                _ => {}
            }
        }

        // now remove all labels unused
        let mut new_code = vec![];

        for (_, insn) in code.iter().enumerate() {
            match insn {
                Bytecode::Label(_, label) => {
                    // if this label is not used, remove it
                    if used_labels.contains(&label) {
                        new_code.push(insn.clone());
                    } else {
                        // skip this instruction
                        changed = true;
                    }
                }

                _ => {
                    new_code.push(insn.clone());
                }
            }
        }

        (new_code, changed)
    }

    fn transform_code(code: Vec<Bytecode>, max_loop: usize) -> Vec<Bytecode> {
        let mut new_code = code;

        let mut loop_count = 0;

        while loop_count < max_loop {
            loop_count += 1;

            let (updated_code, changed1) = Self::remove_destroy(new_code);
            new_code = updated_code;

            let (updated_code, changed2) = Self::remove_hops(new_code);
            new_code = updated_code;

            let (updated_code, changed3) = Self::patch_branch(new_code);
            new_code = updated_code;

            let (updated_code, changed4) = Self::patch_sequence_labels(new_code);
            new_code = updated_code;

            let (updated_code, changed5) = Self::remove_sequence_jumps(new_code);
            new_code = updated_code;

            let (updated_code, changed6) = Self::remove_jump(new_code);
            new_code = updated_code;

            let (updated_code, changed7) = Self::remove_labels(new_code);
            new_code = updated_code;

            // continue optimizing until a fixed point is reached
            if !changed1
                && !changed2
                && !changed3
                && !changed4
                && !changed5
                && !changed6
                && !changed7
            {
                break;
            }
        }

        new_code
    }
}
