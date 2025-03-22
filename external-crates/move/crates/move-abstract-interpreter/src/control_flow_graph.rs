// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module defines the control-flow graph uses for bytecode verification.
use move_binary_format::file_format::{Bytecode, CodeOffset, VariantJumpTable};
use std::collections::{btree_map::Entry, BTreeMap, BTreeSet};

/// A trait that specifies the basic requirements for a CFG
pub trait ControlFlowGraph {
    type BlockId: Copy + Ord;
    type InstructionIndex: Copy + Ord;
    type Instruction;

    /// Start index of the block ID in the bytecode vector
    fn block_start(&self, block_id: Self::BlockId) -> Self::InstructionIndex;

    /// End index of the block ID in the bytecode vector
    fn block_end(&self, block_id: Self::BlockId) -> Self::InstructionIndex;

    /// Successors of the block ID in the bytecode vector
    fn successors(&self, block_id: Self::BlockId) -> &[Self::BlockId];

    /// Return the next block in traversal order
    fn next_block(&self, block_id: Self::BlockId) -> Option<Self::BlockId>;

    /// Iterator over the indexes of instructions in this block
    fn instructions(
        &self,
        block_id: Self::BlockId,
    ) -> impl IntoIterator<Item = (Self::InstructionIndex, &Self::Instruction)>;

    /// Return an iterator over the blocks of the CFG
    fn blocks(&self) -> Vec<Self::BlockId>;

    /// Return the number of blocks (vertices) in the control flow graph
    fn num_blocks(&self) -> usize;

    /// Return the id of the entry block for this control-flow graph
    /// Note: even a CFG with no instructions has an (empty) entry block.
    fn entry_block_id(&self) -> Self::BlockId;

    /// Checks if the block ID is a loop head
    fn is_loop_head(&self, block_id: Self::BlockId) -> bool;

    /// Checks if the edge from cur->next is a back edge
    /// returns false if the edge is not in the cfg
    fn is_back_edge(&self, cur: Self::BlockId, next: Self::BlockId) -> bool;

    /// Return the number of back edges in the cfg
    fn num_back_edges(&self) -> usize;
}

type VMBlockId = file_format::CodeOffset;

struct BasicBlock {
    exit: file_format::CodeOffset,
    successors: Vec<VMBlockId>,
}

/// The control flow graph that we build from the bytecode.
pub struct VMControlFlowGraph<'a> {
    code: &'a [file_format::Bytecode],
    /// The basic blocks
    blocks: BTreeMap<VMBlockId, BasicBlock>,
    /// Basic block ordering for traversal
    traversal_successors: BTreeMap<VMBlockId, VMBlockId>,
    /// Map of loop heads with all of their back edges
    loop_heads: BTreeMap<VMBlockId, /* back edges */ BTreeSet<VMBlockId>>,
}

impl BasicBlock {
    pub fn display(&self, entry: VMBlockId) {
        println!("+=======================+");
        println!("| Enter:  {}            |", entry);
        println!("+-----------------------+");
        println!("==> Children: {:?}", self.successors);
        println!("+-----------------------+");
        println!("| Exit:   {}            |", self.exit);
        println!("+=======================+");
    }
}

const ENTRY_BLOCK_ID: VMBlockId = 0;

impl<'a> VMControlFlowGraph<'a> {
    pub fn new(
        code: &'a [file_format::Bytecode],
        jump_tables: &[file_format::VariantJumpTable],
    ) -> Self {
        use file_format::{Bytecode, CodeOffset};
        use std::collections::{BTreeMap as Map, BTreeSet as Set};

        let code_len = code.len() as CodeOffset;
        // First go through and collect block ids, i.e., offsets that begin basic blocks.
        // Need to do this first in order to handle backwards edges.
        let mut block_ids = BTreeSet::new();
        block_ids.insert(ENTRY_BLOCK_ID);
        for pc in 0..code.len() {
            VMControlFlowGraph::record_block_ids(
                pc as CodeOffset,
                code,
                jump_tables,
                &mut block_ids,
            );
        }

        // Create basic blocks
        let mut blocks = Map::new();
        let mut entry = 0;
        let mut exit_to_entry = Map::new();
        for pc in 0..code.len() {
            let co_pc = pc as CodeOffset;

            // Create a basic block
            if Self::is_end_of_block(co_pc, code, &block_ids) {
                let exit = co_pc;
                exit_to_entry.insert(exit, entry);
                let successors = Bytecode::get_successors(co_pc, code, jump_tables);
                let bb = BasicBlock { exit, successors };
                blocks.insert(entry, bb);
                entry = co_pc + 1;
            }
        }
        let blocks = blocks;
        assert_eq!(entry, code_len);

        // # Loop analysis
        //
        // This section identifies loops in the control-flow graph, picks a back edge and loop head
        // (the basic block the back edge returns to), and decides the order that blocks are
        // traversed during abstract interpretation (reverse post-order).
        //
        // The implementation is based on the algorithm for finding widening points in Section 4.1,
        // "Depth-first numbering" of Bourdoncle [1993], "Efficient chaotic iteration strategies
        // with widenings."
        //
        // NB. The comments below refer to a block's sub-graph -- the reflexive transitive closure
        // of its successor edges, modulo cycles.

        #[derive(Copy, Clone)]
        enum Exploration {
            InProgress,
            Done,
        }

        let mut exploration: Map<VMBlockId, Exploration> = Map::new();
        let mut stack = vec![ENTRY_BLOCK_ID];

        // For every loop in the CFG that is reachable from the entry block, there is an entry in
        // `loop_heads` mapping to all the back edges pointing to it, and vice versa.
        //
        // Entry in `loop_heads` implies loop in the CFG is justified by the comments in the loop
        // below.  Loop in the CFG implies entry in `loop_heads` is justified by considering the
        // point at which the first node in that loop, `F` is added to the `exploration` map:
        //
        // - By definition `F` is part of a loop, meaning there is a block `L` such that:
        //
        //     F - ... -> L -> F
        //
        // - `F` will not transition to `Done` until all the nodes reachable from it (including `L`)
        //   have been visited.
        // - Because `F` is the first node seen in the loop, all the other nodes in the loop
        //   (including `L`) will be visited while `F` is `InProgress`.
        // - Therefore, we will process the `L -> F` edge while `F` is `InProgress`.
        // - Therefore, we will record a back edge to it.
        let mut loop_heads: Map<VMBlockId, Set<VMBlockId>> = Map::new();

        // Blocks appear in `post_order` after all the blocks in their (non-reflexive) sub-graph.
        let mut post_order = Vec::with_capacity(blocks.len());

        while let Some(block) = stack.pop() {
            match exploration.entry(block) {
                Entry::Vacant(entry) => {
                    // Record the fact that exploration of this block and its sub-graph has started.
                    entry.insert(Exploration::InProgress);

                    // Push the block back on the stack to finish processing it, and mark it as done
                    // once its sub-graph has been traversed.
                    stack.push(block);

                    for succ in &blocks[&block].successors {
                        match exploration.get(succ) {
                            // This successor has never been visited before, add it to the stack to
                            // be explored before `block` gets marked `Done`.
                            None => stack.push(*succ),

                            // This block's sub-graph was being explored, meaning it is a (reflexive
                            // transitive) predecessor of `block` as well as being a successor,
                            // implying a loop has been detected -- greedily choose the successor
                            // block as the loop head.
                            Some(Exploration::InProgress) => {
                                loop_heads.entry(*succ).or_default().insert(block);
                            }

                            // Cross-edge detected, this block and its entire sub-graph (modulo
                            // cycles) has already been explored via a different path, and is
                            // already present in `post_order`.
                            Some(Exploration::Done) => { /* skip */ }
                        };
                    }
                }

                Entry::Occupied(mut entry) => match entry.get() {
                    // Already traversed the sub-graph reachable from this block, so skip it.
                    Exploration::Done => continue,

                    // Finish up the traversal by adding this block to the post-order traversal
                    // after its sub-graph (modulo cycles).
                    Exploration::InProgress => {
                        post_order.push(block);
                        entry.insert(Exploration::Done);
                    }
                },
            }
        }

        let traversal_order = {
            // This reverse post order is akin to a topological sort (ignoring cycles) and is
            // different from a pre-order in the presence of diamond patterns in the graph.
            post_order.reverse();
            post_order
        };

        // build a mapping from a block id to the next block id in the traversal order
        let traversal_successors = traversal_order
            .windows(2)
            .map(|window| {
                debug_assert!(window.len() == 2);
                (window[0], window[1])
            })
            .collect();

        VMControlFlowGraph {
            code,
            blocks,
            traversal_successors,
            loop_heads,
        }
    }

    pub fn display(&self) {
        for (entry, block) in &self.blocks {
            block.display(*entry);
        }
        println!("Traversal: {:#?}", self.traversal_successors);
    }

    fn is_end_of_block(
        pc: file_format::CodeOffset,
        code: &[file_format::Bytecode],
        block_ids: &BTreeSet<VMBlockId>,
    ) -> bool {
        pc + 1 == (code.len() as file_format::CodeOffset) || block_ids.contains(&(pc + 1))
    }

    fn record_block_ids(
        pc: file_format::CodeOffset,
        code: &[file_format::Bytecode],
        jump_tables: &[file_format::VariantJumpTable],
        block_ids: &mut BTreeSet<VMBlockId>,
    ) {
        let bytecode = &code[pc as usize];

        block_ids.extend(bytecode.offsets(jump_tables));

        if bytecode.is_branch() && pc + 1 < (code.len() as file_format::CodeOffset) {
            block_ids.insert(pc + 1);
        }
    }

    /// A utility function that implements BFS-reachability from block_id with
    /// respect to get_targets function
    fn traverse_by(&self, block_id: VMBlockId) -> Vec<VMBlockId> {
        let mut ret = Vec::new();
        // We use this index to keep track of our frontier.
        let mut index = 0;
        // Guard against cycles
        let mut seen = BTreeSet::new();

        ret.push(block_id);
        seen.insert(&block_id);

        while index < ret.len() {
            let block_id = ret[index];
            index += 1;
            let successors = self.successors(block_id);
            for block_id in successors.iter() {
                if !seen.contains(&block_id) {
                    ret.push(*block_id);
                    seen.insert(block_id);
                }
            }
        }

        ret
    }

    pub fn reachable_from(&self, block_id: VMBlockId) -> Vec<VMBlockId> {
        self.traverse_by(block_id)
    }
}

impl ControlFlowGraph for VMControlFlowGraph<'_> {
    type BlockId = VMBlockId;
    type InstructionIndex = file_format::CodeOffset;
    type Instruction = file_format::Bytecode;

    // Note: in the following procedures, it's safe not to check bounds because:
    // - Every CFG (even one with no instructions) has a block at ENTRY_BLOCK_ID
    // - The only way to acquire new BlockId's is via block_successors()
    // - block_successors only() returns valid BlockId's
    // Note: it is still possible to get a BlockId from one CFG and use it in another CFG where it
    // is not valid. The design does not attempt to prevent this abuse of the API.

    fn block_start(&self, block_id: VMBlockId) -> file_format::CodeOffset {
        block_id
    }

    fn block_end(&self, block_id: VMBlockId) -> file_format::CodeOffset {
        self.blocks[&block_id].exit
    }

    fn successors(&self, block_id: VMBlockId) -> &[VMBlockId] {
        &self.blocks[&block_id].successors
    }

    fn next_block(&self, block_id: VMBlockId) -> Option<file_format::CodeOffset> {
        debug_assert!(self.blocks.contains_key(&block_id));
        self.traversal_successors.get(&block_id).copied()
    }

    fn instructions(
        &self,
        block_id: VMBlockId,
    ) -> impl IntoIterator<Item = (VMBlockId, &file_format::Bytecode)> {
        let start = self.block_start(block_id);
        let end = self.block_end(block_id);
        (start..end).map(|pc| (pc, &self.code[pc as usize]))
    }

    fn blocks(&self) -> Vec<VMBlockId> {
        self.blocks.keys().cloned().collect()
    }

    fn num_blocks(&self) -> usize {
        self.blocks.len()
    }

    fn entry_block_id(&self) -> VMBlockId {
        ENTRY_BLOCK_ID
    }

    fn is_loop_head(&self, block_id: VMBlockId) -> bool {
        self.loop_heads.contains_key(&block_id)
    }

    fn is_back_edge(&self, cur: VMBlockId, next: VMBlockId) -> bool {
        self.loop_heads
            .get(&next)
            .is_some_and(|back_edges| back_edges.contains(&cur))
    }

    fn num_back_edges(&self) -> usize {
        self.loop_heads
            .iter()
            .fold(0, |acc, (_, edges)| acc + edges.len())
    }
}
