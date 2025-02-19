// Copyright (c) Verichains, 2023

use std::collections::HashSet;


use super::{
    super::datastructs::*,
    topo::{TopoSortedBlockItem, TopoSortedBlocks},
};

#[derive(Default, Debug)]
pub struct DominationMeta {
    pub(crate) is_domination: bool,
}

pub fn loop_reconstruction<BlockContent: BlockContentTrait>(
    bbs: &mut TopoSortedBlocks<BlockContent>,
    _entry: usize,
) -> Result<(), anyhow::Error> {
    loop_reconstruction_recursive(bbs)
}

fn loop_reconstruction_recursive<BlockContent: BlockContentTrait>(
    bbs: &mut TopoSortedBlocks<BlockContent>,
) -> Result<(), anyhow::Error> {
    for item in bbs.blocks.iter_mut() {
        let TopoSortedBlockItem::SubBlock(sub_block) = item else {
            continue;
        };
        if sub_block.block_count() == 0 {
            continue;
        }
        // this is a loop, annotate it
        let scc_entry = sub_block.entry;
        let scc_exit = sub_block.exit.unwrap_or(usize::MAX);
        if sub_block.block_count() == 1 {
            let entry = sub_block.find_block_mut(scc_entry).unwrap();
            if !entry.next.next_blocks().iter().any(|&&x| x == scc_entry)
                || entry.unconditional_loop_entry.is_some()
                || matches!(entry.next, Terminator::While { .. })
            {
                continue;
            }
        }

        debug_assert!(scc_entry != scc_exit);

        let annotate_jump_target = |x: BranchTarget<usize>| {
            let mut x = x;
            if x.target == scc_entry {
                x.branch_type = BranchType::Continue;
            }
            if x.target == scc_exit {
                x.branch_type = BranchType::Break;
            }
            x
        };

        sub_block.for_each_block_mut(|b| match b.next.clone() {
            Terminator::Branch { target } => {
                if target == scc_entry {
                    b.next = Terminator::Continue { target };
                } else if target == scc_exit {
                    b.next = Terminator::Break { target };
                } else {
                    // inner loop block break/continue of outer loop
                    if let Terminator::Continue { target: t } | Terminator::Break { target: t } =
                        b.next
                    {
                        b.next = Terminator::Branch { target: t };
                    }
                }
            }
            Terminator::IfElse {
                if_block,
                else_block,
            } => {
                if b.idx != scc_entry {
                    b.next = Terminator::IfElse {
                        if_block: annotate_jump_target(if_block),
                        else_block: annotate_jump_target(else_block),
                    };
                }
            }
            _ => {}
        });

        let mut is_valid_conditioned_entry = true;

        let mut content = HashSet::new();

        sub_block.for_each_block(|b| {
            content.insert(b.idx);
        });

        let entry = sub_block.find_block_mut(scc_entry).unwrap();

        if let Terminator::IfElse {
            if_block,
            else_block,
        } = entry.next.clone()
        {
            if else_block.target != scc_exit {
                is_valid_conditioned_entry = false;
            }

            if !is_valid_conditioned_entry {
                // dummy block rewrite was skipped, we need to do it now
                entry.next = Terminator::IfElse {
                    if_block: annotate_jump_target(if_block),
                    else_block: annotate_jump_target(else_block),
                };
            }
        } else {
            is_valid_conditioned_entry = false;
        }

        if is_valid_conditioned_entry {
            if let Terminator::IfElse {
                if_block,
                else_block,
            } = entry.next.clone()
            {
                entry.next = Terminator::While {
                    inner_block: if_block.target,
                    outer_block: else_block.target,
                    content_blocks: content.iter().cloned().collect(),
                };
            } else {
                unreachable!();
            }
        } else {
            entry.unconditional_loop_entry = Some((scc_exit, content));
        }

        loop_reconstruction_recursive(sub_block)?;
    }

    Ok(())
}
