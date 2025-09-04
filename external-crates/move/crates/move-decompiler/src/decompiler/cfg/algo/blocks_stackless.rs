// Copyright (c) Verichains, 2023

use std::collections::HashMap;

use move_stackless_bytecode::stackless_bytecode::{AttrId, Bytecode, Label};

use super::super::datastructs::*;
use super::super::metadata::*;

#[derive(Debug, Clone)]
pub struct AnnotatedBytecodeData {
    pub removed: bool,
    pub original_offset: usize,
    pub jump_type: JumpType, // only when bytecode is Branch/Jump
    pub bytecode: Bytecode,
}

pub type AnnotatedBytecode = WithMetadata<AnnotatedBytecodeData>;

impl<'a> From<&'a AnnotatedBytecode> for &'a Bytecode {
    fn from(annotated: &'a AnnotatedBytecode) -> Self {
        &annotated.bytecode
    }
}

#[derive(Debug, Clone)]
pub struct StacklessBlockContent {
    pub code: Vec<AnnotatedBytecode>,
}
pub type StacklessBasicBlock = BasicBlock<usize, StacklessBlockContent>;
impl BlockIdentifierTrait for usize {}
impl Default for StacklessBlockContent {
    fn default() -> Self {
        StacklessBlockContent { code: Vec::new() }
    }
}

impl BlockContentTrait for StacklessBlockContent {}

pub fn split_basic_blocks_stackless_bytecode(
    insts: &[Bytecode],
) -> Result<Vec<StacklessBasicBlock>, anyhow::Error> {
    let mut block_id_from_label = HashMap::<Label, usize>::new();
    let mut bbs = allocate_basic_blocks(insts, &mut block_id_from_label)?;
    link_basic_blocks(insts, &mut bbs, block_id_from_label)?;
    update_block_var_usage(&mut bbs);

    Ok(bbs)
}

fn update_block_var_usage(bbs: &mut [BasicBlock<usize, StacklessBlockContent>]) {
    for block in bbs.iter_mut() {
        for inst in block.content.code.iter() {
            match &inst.bytecode {
                Bytecode::Call(_, dsts, _, srcs, _) => {
                    for dst in dsts {
                        block.has_assignment_variables.insert(dst.clone());
                    }
                    for src in srcs {
                        block.has_read_variables.insert(src.clone());
                    }
                }
                Bytecode::Ret(_, srcs) => {
                    for src in srcs {
                        block.has_read_variables.insert(src.clone());
                    }
                }
                Bytecode::Abort(_, src) => {
                    block.has_read_variables.insert(src.clone());
                }
                Bytecode::Branch(_, _, _, cond) | Bytecode::VariantSwitch(_, cond, _) => {
                    block.has_read_variables.insert(cond.clone());
                }
                Bytecode::Jump(_, _) => {}
                Bytecode::Label(_, _) => {}
                Bytecode::Assign(_, dst, src, _) => {
                    block.has_assignment_variables.insert(dst.clone());
                    block.has_read_variables.insert(src.clone());
                }
                Bytecode::Load(_, dst, _) => {
                    block.has_assignment_variables.insert(dst.clone());
                }
                Bytecode::Nop(_) => {}
            }
        }
    }
}

fn allocate_basic_blocks(
    insts: &[Bytecode],
    block_id_from_label: &mut HashMap<Label, usize>,
) -> Result<Vec<StacklessBasicBlock>, anyhow::Error> {
    let mut bbs = Vec::<StacklessBasicBlock>::new();

    let mut bidx: usize = 0;
    bidx = bidx.wrapping_sub(1);

    fn new_block(bbs: &mut Vec<StacklessBasicBlock>, bidx: &mut usize) {
        // if the last block flow is not terminated, we need to append a jump to this block
        // as the blocks may not be adjacent later
        let next_id = bidx.wrapping_add(1);
        let new_block = StacklessBasicBlock::new(next_id);
        bbs.push(new_block);
        *bidx = next_id;
    }

    // split the blocks
    let mut start_new_block = true;
    let mut is_block_start_labels = false;
    for (offset, code) in insts.iter().enumerate() {
        if start_new_block {
            new_block(&mut bbs, &mut bidx);
            is_block_start_labels = true;
            start_new_block = false;
            true
        } else {
            false
        };

        match code {
            Bytecode::Branch(..) | Bytecode::Jump(..) => {
                // start a new block after this instruction
                start_new_block = true;
            }
            Bytecode::Ret(_, _) => {
                bbs[bidx].next = Terminator::Ret;
            }
            Bytecode::Abort(_, _) => {
                bbs[bidx].next = Terminator::Abort;
            }
            _ => {}
        };

        if let Bytecode::Label(_, label) = code {
            // start a new block from this instruction
            if !is_block_start_labels {
                new_block(&mut bbs, &mut bidx);
                is_block_start_labels = true;
            }
            block_id_from_label.insert(label.clone(), bidx);
        } else {
            is_block_start_labels = false;
        }

        if !is_block_start_labels {
            bbs[bidx].content.code.push(
                AnnotatedBytecodeData {
                    removed: false,
                    jump_type: JumpType::Unknown,
                    original_offset: offset,
                    bytecode: code.clone(),
                }
                .with_metadata(),
            );
        }
    }

    Ok(bbs)
}

fn link_basic_blocks(
    instructions: &[Bytecode],
    bbs: &mut Vec<BasicBlock<usize, StacklessBlockContent>>,
    block_id_from_label: HashMap<Label, usize>,
) -> Result<(), anyhow::Error> {
    use anyhow::anyhow as e;

    let get = |label, err| block_id_from_label.get(label).ok_or(err).map(|x| x.clone());

    for block in bbs.iter_mut() {
        if matches!(block.next, Terminator::Ret | Terminator::Abort) {
            continue;
        }

        // there must be no empty block, right?
        let last_inst = block.content.code.last().unwrap();
        match &last_inst.bytecode {
            Bytecode::Branch(a, if_lbl, else_lbl, _cond) => {
                block.next = Terminator::IfElse {
                    if_block: BranchTarget::unknown(get(
                        if_lbl,
                        e!("Branch inst {} has invalid if label", a.as_usize()),
                    )?),
                    else_block: BranchTarget::unknown(get(
                        else_lbl,
                        e!("Branch inst {} has invalid else label", a.as_usize()),
                    )?),
                };
            }
            Bytecode::Jump(a, dest) => {
                block.next = Terminator::Branch {
                    target: get(
                        dest,
                        e!("Jump inst {} has invalid dest label", a.as_usize()),
                    )?,
                };
            }
            _ => {}
        };
    }

    rewrite_jumps(bbs, block_id_from_label)?;

    let mut next_inst_id = instructions.len();
    for instr in instructions.iter() {
        next_inst_id = std::cmp::max(next_inst_id, instr.get_attr_id().as_usize() + 1);
        if let Bytecode::Label(_, label) = instr {
            next_inst_id = std::cmp::max(next_inst_id, label.as_usize() + 1);
        }
    }

    generate_leading_labels(bbs, &mut next_inst_id);

    ensure_adjacent_blocks_jump(bbs, next_inst_id);

    if let Some(last_block) = bbs.last_mut() {
        if matches!(last_block.next, Terminator::Normal) {
            last_block.next = Terminator::Ret;
            // TODO: adding ret ocode?
        }
    }

    Ok(())
}

fn ensure_adjacent_blocks_jump(
    bbs: &mut Vec<BasicBlock<usize, StacklessBlockContent>>,
    mut next_inst_id: usize,
) {
    for idx in 1..bbs.len() {
        let prev_block = &mut bbs[idx - 1];

        if matches!(prev_block.next, Terminator::Normal) {
            prev_block.content.code.push(
                AnnotatedBytecodeData {
                    removed: false,
                    jump_type: JumpType::Unknown,
                    original_offset: usize::MAX,
                    bytecode: Bytecode::Jump(AttrId::new(next_inst_id), Label::new(idx)),
                }
                .with_metadata(),
            );
            next_inst_id = next_inst_id.wrapping_add(1);
            prev_block.next = Terminator::Branch { target: idx };
        }
    }
}

/// Rewrite jumps to use the block ids
fn rewrite_jumps(
    bbs: &mut [BasicBlock<usize, StacklessBlockContent>],
    block_id_from_label: HashMap<Label, usize>,
) -> Result<(), anyhow::Error> {
    use anyhow::anyhow as e;
    let get = |label, err| {
        block_id_from_label
            .get(&label)
            .ok_or(err)
            .map(|x| x.clone())
    };

    for block in bbs.iter_mut() {
        for inst in block.content.code.iter_mut() {
            match &mut inst.bytecode {
                Bytecode::Jump(_, dest) => {
                    *dest = Label::new(get(
                        *dest,
                        e!("Jump inst has invalid dest label {}", dest.as_usize()),
                    )?);
                }
                Bytecode::Branch(_, t, f, _) => {
                    *t = Label::new(get(
                        *t,
                        e!("Branch inst has invalid if label {}", t.as_usize()),
                    )?);
                    *f = Label::new(get(
                        *f,
                        e!("Branch inst has invalid else label {}", f.as_usize()),
                    )?);
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn generate_leading_labels(
    bbs: &mut [BasicBlock<usize, StacklessBlockContent>],
    next_inst_id: &mut usize,
) {
    for idx in 0..bbs.len() {
        let block = &mut bbs[idx];

        block.content.code.insert(
            0,
            AnnotatedBytecodeData {
                removed: false,
                jump_type: JumpType::Unknown,
                original_offset: *next_inst_id,
                bytecode: Bytecode::Label(AttrId::new(*next_inst_id), Label::new(idx)),
            }
            .with_metadata(),
        );

        *next_inst_id = next_inst_id.wrapping_add(1);
    }
}
