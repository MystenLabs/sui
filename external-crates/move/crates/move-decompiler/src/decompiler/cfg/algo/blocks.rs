// Copyright (c) Verichains, 2023

use crate::decompiler::cfg::metadata::WithMetadata;

use super::super::datastructs::*;

#[allow(dead_code)]
pub fn remove_block<BlockContent: BlockContentTrait>(
    blocks: &mut Vec<WithMetadata<BasicBlock<usize, BlockContent>>>,
    block_id: usize,
) {
    let update = |dest: &mut usize| {
        if *dest == block_id {
            panic!("block {} still referenced", block_id);
        } else if *dest > block_id {
            *dest -= 1;
        }
    };
    fn update_set<'s, T: Iterator<Item = &'s mut usize>>(iter_mut: T, block_id: usize) {
        for dest in iter_mut {
            if *dest == block_id {
                panic!("block {} still referenced", block_id);
            } else if *dest > block_id {
                *dest -= 1;
            }
        }
    }
    let update_terminator = |terminator: &mut Terminator<usize>| match terminator {
        Terminator::Branch { target } => {
            update(target);
        }
        Terminator::IfElse {
            if_block,
            else_block,
        } => {
            update(&mut if_block.target);
            update(&mut else_block.target);
        }
        Terminator::While {
            inner_block,
            outer_block,
            content_blocks,
        } => {
            update(inner_block);
            update(outer_block);
            update_set(content_blocks.iter_mut(), block_id);
        }
        Terminator::Break { target } => {
            update(target);
        }
        Terminator::Continue { target } => {
            update(target);
        }
        Terminator::Ret | Terminator::Abort | Terminator::Normal => {}
    };
    for block in blocks.iter_mut() {
        if block.idx != block_id {
            update(&mut block.idx);
        }
        update_terminator(&mut block.next);
        if let Some((idx, contents)) = &mut block.unconditional_loop_entry {
            update(idx);
            let mut contents_vec: Vec<_> = contents.iter().cloned().collect();
            update_set(contents_vec.iter_mut(), block_id);
            *contents = contents_vec.into_iter().collect();
        }
        if let Some((_, terminator)) = &mut block.short_circuit_terminator {
            update_terminator(terminator);
        }
    }
    blocks.remove(block_id);
}

// pub fn insert_block<BlockContent: BlockContentTrait>(
//     blocks: &mut Vec<BasicBlock<usize, BlockContent>>,
//     block_id: usize,
//     block: BasicBlock<usize, BlockContent>,
// ) {
//     let update = |dest: &mut usize| {
//         if *dest >= block_id {
//             *dest += 1;
//         }
//     };
//     let update_set = |set: &mut HashSet<usize>| {
//         let mut new_set = HashSet::new();
//         for &id in set.iter() {
//             if id >= block_id {
//                 new_set.insert(id + 1);
//             } else {
//                 new_set.insert(id);
//             }
//         }
//         *set = new_set;
//     };
//     for block in blocks.iter_mut() {
//         match &mut block.next {
//             Terminator::Branch(dest) => {
//                 update(dest);
//             }
//             Terminator::IfElse(if_dest, else_dest) => {
//                 update(if_dest);
//                 update(else_dest);
//             }
//             Terminator::While(in_dest, out_dest) => {
//                 update(in_dest);
//                 update(out_dest);
//             }
//             Terminator::Break(dest) => {
//                 update(dest);
//             }
//             Terminator::Continue(dest) => {
//                 update(dest);
//             }
//             Terminator::Ret | Terminator::Abort | Terminator::Normal => {}
//         }
//         if let Some(idx) = &mut block.unconditional_loop_entry {
//             update(idx);
//         }
//         update_set(&mut block.topo_after);
//         update_set(&mut block.topo_before);
//     }
//     blocks.insert(block_id + 1, block);
// }
