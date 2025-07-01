// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module defines the control-flow graph uses for bytecode verification.
use move_binary_format::{
    file_format::{CodeOffset, JumpTableInner},
    normalized::{Bytecode, VariantJumpTable},
};
use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    fmt::{Debug, Display},
    hash::Hash,
    rc::Rc,
};
// BTree/Hash agnostic type wrappers
type Map<K, V> = BTreeMap<K, V>;
type Set<V> = BTreeSet<V>;

pub type BlockId = CodeOffset;

/// A trait that specifies the basic requirements for a CFG
pub trait ControlFlowGraph {
    /// Start index of the block ID in the bytecode vector
    fn block_start(&self, block_id: BlockId) -> CodeOffset;

    /// End index of the block ID in the bytecode vector
    fn block_end(&self, block_id: BlockId) -> CodeOffset;

    /// Successors of the block ID in the bytecode vector
    fn successors(&self, block_id: BlockId) -> &Vec<BlockId>;

    /// Return the next block in traversal order
    fn next_block(&self, block_id: BlockId) -> Option<BlockId>;

    /// Iterator over the indexes of instructions in this block
    fn instr_indexes(&self, block_id: BlockId) -> Box<dyn Iterator<Item = CodeOffset>>;

    /// Return an iterator over the blocks of the CFG
    fn blocks(&self) -> Vec<BlockId>;

    /// Return the number of blocks (vertices) in the control flow graph
    fn num_blocks(&self) -> u16;

    /// Return the id of the entry block for this control-flow graph
    /// Note: even a CFG with no instructions has an (empty) entry block.
    fn entry_block_id(&self) -> BlockId;

    /// Checks if the block ID is a loop head
    fn is_loop_head(&self, block_id: BlockId) -> bool;

    /// Checks if the edge from cur->next is a back edge
    /// returns false if the edge is not in the cfg
    fn is_back_edge(&self, cur: BlockId, next: BlockId) -> bool;

    /// Return the number of back edges in the cfg
    fn num_back_edges(&self) -> usize;
}

struct BasicBlock {
    exit: CodeOffset,
    successors: Vec<BlockId>,
}

/// The control flow graph that we build from the bytecode.
pub struct StacklessControlFlowGraph {
    /// The basic blocks
    blocks: Map<BlockId, BasicBlock>,
    /// Basic block ordering for traversal
    traversal_successors: Map<BlockId, BlockId>,
    /// Map of loop heads with all of their back edges
    loop_heads: Map<BlockId, /* back edges */ Set<BlockId>>,
}

impl BasicBlock {
    pub fn display(&self, entry: BlockId) {
        println!("+=======================+");
        println!("| Enter:  {}            |", entry);
        println!("+-----------------------+");
        println!("==> Children: {:?}", self.successors);
        println!("+-----------------------+");
        println!("| Exit:   {}            |", self.exit);
        println!("+=======================+");
    }
}

const ENTRY_BLOCK_ID: BlockId = 0;

impl StacklessControlFlowGraph {
    pub fn new<S: Hash + Eq + Display + Debug>(
        code: &[Bytecode<S>],
        jump_tables: &[Rc<VariantJumpTable<S>>],
    ) -> Self {
        let code_len = code.len() as CodeOffset;
        // First go through and collect block ids, i.e., offsets that begin basic blocks.
        // Need to do this first in order to handle backwards edges.
        let mut block_ids = Set::new();
        block_ids.insert(ENTRY_BLOCK_ID);
        for pc in 0..code.len() {
            StacklessControlFlowGraph::record_block_ids(
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
                let successors = get_successors(co_pc, code, jump_tables);
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

        let mut exploration: Map<BlockId, Exploration> = Map::new();
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
        let mut loop_heads: Map<BlockId, Set<BlockId>> = Map::new();

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

        StacklessControlFlowGraph {
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

    fn is_end_of_block<S: Hash + Eq + Display + Debug>(
        pc: CodeOffset,
        code: &[Bytecode<S>],
        block_ids: &Set<BlockId>,
    ) -> bool {
        pc + 1 == (code.len() as CodeOffset) || block_ids.contains(&(pc + 1))
    }

    fn record_block_ids<S: Hash + Eq + Display + Debug>(
        pc: CodeOffset,
        code: &[Bytecode<S>],
        jump_tables: &[Rc<VariantJumpTable<S>>],
        block_ids: &mut Set<BlockId>,
    ) {
        let bytecode = &code[pc as usize];

        block_ids.extend(offsets(bytecode, jump_tables));

        if is_branch(bytecode) && pc + 1 < (code.len() as CodeOffset) {
            block_ids.insert(pc + 1);
        }
    }

    /// A utility function that implements BFS-reachability from block_id with
    /// respect to get_targets function
    fn traverse_by(&self, block_id: BlockId) -> Vec<BlockId> {
        let mut ret = Vec::new();
        // We use this index to keep track of our frontier.
        let mut index = 0;
        // Guard against cycles
        let mut seen = Set::new();

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

    pub fn reachable_from(&self, block_id: BlockId) -> Vec<BlockId> {
        self.traverse_by(block_id)
    }
}

fn is_branch<S: Hash + Eq + Display + Debug>(bytecode: &Bytecode<S>) -> bool {
    is_conditional_branch(bytecode) || is_unconditional_branch(bytecode)
}

pub fn is_unconditional_branch<S: Hash + Eq + Display + Debug>(bytecode: &Bytecode<S>) -> bool {
    match bytecode {
            Bytecode::Ret | Bytecode::Abort | Bytecode::Branch(_) => true,
            // NB: Since `VariantSwitch` is guaranteed to be exhaustive by the bytecode verifier,
            // it is an unconditional branch.
            Bytecode::VariantSwitch(_) => true,
            Bytecode::Pop
            | Bytecode::BrTrue(_)
            | Bytecode::BrFalse(_)
            | Bytecode::LdU8(_)
            | Bytecode::LdU64(_)
            | Bytecode::LdU128(_)
            | Bytecode::CastU8
            | Bytecode::CastU64
            | Bytecode::CastU128
            | Bytecode::LdConst(_)
            | Bytecode::LdTrue
            | Bytecode::LdFalse
            | Bytecode::CopyLoc(_)
            | Bytecode::MoveLoc(_)
            | Bytecode::StLoc(_)
            | Bytecode::Call(_)
            // | Bytecode::CallGeneric(_)
            | Bytecode::Pack(_)
            // | Bytecode::PackGeneric(_)
            | Bytecode::Unpack(_)
            // | Bytecode::UnpackGeneric(_)
            | Bytecode::ReadRef
            | Bytecode::WriteRef
            | Bytecode::FreezeRef
            | Bytecode::MutBorrowLoc(_)
            | Bytecode::ImmBorrowLoc(_)
            | Bytecode::MutBorrowField(_)
            // | Bytecode::MutBorrowFieldGeneric(_)
            | Bytecode::ImmBorrowField(_)
            // | Bytecode::ImmBorrowFieldGeneric(_)
            | Bytecode::Add
            | Bytecode::Sub
            | Bytecode::Mul
            | Bytecode::Mod
            | Bytecode::Div
            | Bytecode::BitOr
            | Bytecode::BitAnd
            | Bytecode::Xor
            | Bytecode::Or
            | Bytecode::And
            | Bytecode::Not
            | Bytecode::Eq
            | Bytecode::Neq
            | Bytecode::Lt
            | Bytecode::Gt
            | Bytecode::Le
            | Bytecode::Ge
            | Bytecode::Nop
            | Bytecode::Shl
            | Bytecode::Shr
            | Bytecode::VecPack(_)
            | Bytecode::VecLen(_)
            | Bytecode::VecImmBorrow(_)
            | Bytecode::VecMutBorrow(_)
            | Bytecode::VecPushBack(_)
            | Bytecode::VecPopBack(_)
            | Bytecode::VecUnpack(_)
            | Bytecode::VecSwap(_)
            | Bytecode::LdU16(_)
            | Bytecode::LdU32(_)
            | Bytecode::LdU256(_)
            | Bytecode::CastU16
            | Bytecode::CastU32
            | Bytecode::CastU256
            | Bytecode::PackVariant(_)
            // | Bytecode::PackVariantGeneric(_)
            | Bytecode::UnpackVariant(_)
            | Bytecode::UnpackVariantImmRef(_)
            | Bytecode::UnpackVariantMutRef(_)
            // | Bytecode::UnpackVariantGeneric(_)
            // | Bytecode::UnpackVariantGenericImmRef(_)
            // | Bytecode::UnpackVariantGenericMutRef(_)
            | Bytecode::ExistsDeprecated(_)
            // | Bytecode::ExistsGenericDeprecated(_)
            | Bytecode::MoveFromDeprecated(_)
            // | Bytecode::MoveFromGenericDeprecated(_)
            | Bytecode::MoveToDeprecated(_)
            // | Bytecode::MoveToGenericDeprecated(_)
            | Bytecode::MutBorrowGlobalDeprecated(_)
            // | Bytecode::MutBorrowGlobalGenericDeprecated(_)
            | Bytecode::ImmBorrowGlobalDeprecated(_)
            // | Bytecode::ImmBorrowGlobalGenericDeprecated(_) 
            => false,
        }
}

fn is_conditional_branch<S: Hash + Eq + Display + Debug>(bytecode: &Bytecode<S>) -> bool {
    match bytecode {
            Bytecode::BrFalse(_) | Bytecode::BrTrue(_) => true,
            // NB: since `VariantSwitch` is guaranteed to branch (since it is exhaustive), it is
            // not conditional.
            Bytecode::VariantSwitch(_) => false,
            Bytecode::Pop
            | Bytecode::Ret
            | Bytecode::Branch(_)
            | Bytecode::LdU8(_)
            | Bytecode::LdU64(_)
            | Bytecode::LdU128(_)
            | Bytecode::CastU8
            | Bytecode::CastU64
            | Bytecode::CastU128
            | Bytecode::LdConst(_)
            | Bytecode::LdTrue
            | Bytecode::LdFalse
            | Bytecode::CopyLoc(_)
            | Bytecode::MoveLoc(_)
            | Bytecode::StLoc(_)
            | Bytecode::Call(_)
            // | Bytecode::CallGeneric(_)
            | Bytecode::Pack(_)
            // | Bytecode::PackGeneric(_)
            | Bytecode::Unpack(_)
            // | Bytecode::UnpackGeneric(_)
            | Bytecode::ReadRef
            | Bytecode::WriteRef
            | Bytecode::FreezeRef
            | Bytecode::MutBorrowLoc(_)
            | Bytecode::ImmBorrowLoc(_)
            | Bytecode::MutBorrowField(_)
            // | Bytecode::MutBorrowFieldGeneric(_)
            | Bytecode::ImmBorrowField(_)
            // | Bytecode::ImmBorrowFieldGeneric(_)
            | Bytecode::Add
            | Bytecode::Sub
            | Bytecode::Mul
            | Bytecode::Mod
            | Bytecode::Div
            | Bytecode::BitOr
            | Bytecode::BitAnd
            | Bytecode::Xor
            | Bytecode::Or
            | Bytecode::And
            | Bytecode::Not
            | Bytecode::Eq
            | Bytecode::Neq
            | Bytecode::Lt
            | Bytecode::Gt
            | Bytecode::Le
            | Bytecode::Ge
            | Bytecode::Abort
            | Bytecode::Nop
            | Bytecode::Shl
            | Bytecode::Shr
            | Bytecode::VecPack(_)
            | Bytecode::VecLen(_)
            | Bytecode::VecImmBorrow(_)
            | Bytecode::VecMutBorrow(_)
            | Bytecode::VecPushBack(_)
            | Bytecode::VecPopBack(_)
            | Bytecode::VecUnpack(_)
            | Bytecode::VecSwap(_)
            | Bytecode::LdU16(_)
            | Bytecode::LdU32(_)
            | Bytecode::LdU256(_)
            | Bytecode::CastU16
            | Bytecode::CastU32
            | Bytecode::CastU256
            | Bytecode::PackVariant(_)
            // | Bytecode::PackVariantGeneric(_)
            | Bytecode::UnpackVariant(_)
            | Bytecode::UnpackVariantImmRef(_)
            | Bytecode::UnpackVariantMutRef(_)
            // | Bytecode::UnpackVariantGeneric(_)
            // | Bytecode::UnpackVariantGenericImmRef(_)
            // | Bytecode::UnpackVariantGenericMutRef(_)
            | Bytecode::ExistsDeprecated(_)
            // | Bytecode::ExistsGenericDeprecated(_)
            | Bytecode::MoveFromDeprecated(_)
            // | Bytecode::MoveFromGenericDeprecated(_)
            | Bytecode::MoveToDeprecated(_)
            // | Bytecode::MoveToGenericDeprecated(_)
            | Bytecode::MutBorrowGlobalDeprecated(_)
            // | Bytecode::MutBorrowGlobalGenericDeprecated(_)
            | Bytecode::ImmBorrowGlobalDeprecated(_)
            // | Bytecode::ImmBorrowGlobalGenericDeprecated(_)
             => false,
        }
}

fn get_successors<S: Hash + Eq + Display + Debug>(
    pc: CodeOffset,
    code: &[Bytecode<S>],
    jump_tables: &[Rc<VariantJumpTable<S>>],
) -> Vec<BlockId> {
    assert!(
        // The program counter must remain within the bounds of the code
        pc < u16::MAX && (pc as usize) < code.len(),
        "Program counter out of bounds"
    );

    let bytecode = &code[pc as usize];
    let mut v = vec![];

    v.extend(offsets(bytecode, jump_tables));

    let next_pc = pc + 1;
    if next_pc >= code.len() as CodeOffset {
        return v;
    }

    if !is_unconditional_branch(bytecode) && !v.contains(&next_pc) {
        // avoid duplicates
        v.push(pc + 1);
    }

    // always give successors in ascending order
    // NB: the size of `v` is generally quite small (bounded by maximum # of variants allowed
    // in a variant jump table), so a sort here is not a performance concern.
    v.sort();

    v
}

pub fn offsets<S: Hash + Eq + Display + Debug>(
    bytecode: &Bytecode<S>,
    jump_tables: &[Rc<VariantJumpTable<S>>],
) -> Vec<CodeOffset> {
    match bytecode {
            Bytecode::BrFalse(offset) | Bytecode::BrTrue(offset) | Bytecode::Branch(offset) => {
                vec![*offset]
            }
            // NB: bounds checking has already been performed at this point.
            Bytecode::VariantSwitch(jt) => {
                let JumpTableInner::Full(offsets) = &jt.jump_table;

                assert!(
                    // The jump table index must be within the bounds of the jump tables. This is
                    // checked in the bounds checker.
                    // TODO is this really necessary?
                    jump_tables.iter().any(|jt_| jt_.equivalent(jt)),
                    "Jump table index out of bounds"
                );

                offsets.clone()
            }
            // Separated out for clarity -- these are branch instructions, but have no offset so we
            // don't return any offsets for them.
            Bytecode::Ret | Bytecode::Abort => vec![],

            Bytecode::Pop
            | Bytecode::LdU8(_)
            | Bytecode::LdU64(_)
            | Bytecode::LdU128(_)
            | Bytecode::CastU8
            | Bytecode::CastU64
            | Bytecode::CastU128
            | Bytecode::LdConst(_)
            | Bytecode::LdTrue
            | Bytecode::LdFalse
            | Bytecode::CopyLoc(_)
            | Bytecode::MoveLoc(_)
            | Bytecode::StLoc(_)
            | Bytecode::Call(_)
            // | Bytecode::CallGeneric(_)
            | Bytecode::Pack(_)
            // | Bytecode::PackGeneric(_)
            | Bytecode::Unpack(_)
            // | Bytecode::UnpackGeneric(_)
            | Bytecode::ReadRef
            | Bytecode::WriteRef
            | Bytecode::FreezeRef
            | Bytecode::MutBorrowLoc(_)
            | Bytecode::ImmBorrowLoc(_)
            | Bytecode::MutBorrowField(_)
            // | Bytecode::MutBorrowFieldGeneric(_)
            | Bytecode::ImmBorrowField(_)
            // | Bytecode::ImmBorrowFieldGeneric(_)
            | Bytecode::Add
            | Bytecode::Sub
            | Bytecode::Mul
            | Bytecode::Mod
            | Bytecode::Div
            | Bytecode::BitOr
            | Bytecode::BitAnd
            | Bytecode::Xor
            | Bytecode::Or
            | Bytecode::And
            | Bytecode::Not
            | Bytecode::Eq
            | Bytecode::Neq
            | Bytecode::Lt
            | Bytecode::Gt
            | Bytecode::Le
            | Bytecode::Ge
            | Bytecode::Nop
            | Bytecode::Shl
            | Bytecode::Shr
            | Bytecode::VecPack(_)
            | Bytecode::VecLen(_)
            | Bytecode::VecImmBorrow(_)
            | Bytecode::VecMutBorrow(_)
            | Bytecode::VecPushBack(_)
            | Bytecode::VecPopBack(_)
            | Bytecode::VecUnpack(_)
            | Bytecode::VecSwap(_)
            | Bytecode::LdU16(_)
            | Bytecode::LdU32(_)
            | Bytecode::LdU256(_)
            | Bytecode::CastU16
            | Bytecode::CastU32
            | Bytecode::CastU256
            | Bytecode::PackVariant(_)
            // | Bytecode::PackVariantGeneric(_)
            | Bytecode::UnpackVariant(_)
            | Bytecode::UnpackVariantImmRef(_)
            | Bytecode::UnpackVariantMutRef(_)
            // | Bytecode::UnpackVariantGeneric(_)
            // | Bytecode::UnpackVariantGenericImmRef(_)
            // | Bytecode::UnpackVariantGenericMutRef(_)
            | Bytecode::ExistsDeprecated(_)
            // | Bytecode::ExistsGenericDeprecated(_)
            | Bytecode::MoveFromDeprecated(_)
            //  Bytecode::MoveFromGenericDeprecated(_)
            | Bytecode::MoveToDeprecated(_)
            // | Bytecode::MoveToGenericDeprecated(_)
            | Bytecode::MutBorrowGlobalDeprecated(_)
            // | Bytecode::MutBorrowGlobalGenericDeprecated(_)
            | Bytecode::ImmBorrowGlobalDeprecated(_)
            // | Bytecode::ImmBorrowGlobalGenericDeprecated(_)
             => vec![],
        }
}

impl ControlFlowGraph for StacklessControlFlowGraph {
    // Note: in the following procedures, it's safe not to check bounds because:
    // - Every CFG (even one with no instructions) has a block at ENTRY_BLOCK_ID
    // - The only way to acquire new BlockId's is via block_successors()
    // - block_successors only() returns valid BlockId's
    // Note: it is still possible to get a BlockId from one CFG and use it in another CFG where it
    // is not valid. The design does not attempt to prevent this abuse of the API.

    fn block_start(&self, block_id: BlockId) -> CodeOffset {
        block_id
    }

    fn block_end(&self, block_id: BlockId) -> CodeOffset {
        self.blocks[&block_id].exit
    }

    fn successors(&self, block_id: BlockId) -> &Vec<BlockId> {
        &self.blocks[&block_id].successors
    }

    fn next_block(&self, block_id: BlockId) -> Option<CodeOffset> {
        debug_assert!(self.blocks.contains_key(&block_id));
        self.traversal_successors.get(&block_id).copied()
    }

    fn instr_indexes(&self, block_id: BlockId) -> Box<dyn Iterator<Item = CodeOffset>> {
        Box::new(self.block_start(block_id)..=self.block_end(block_id))
    }

    fn blocks(&self) -> Vec<BlockId> {
        self.blocks.keys().cloned().collect()
    }

    fn num_blocks(&self) -> u16 {
        self.blocks.len() as u16
    }

    fn entry_block_id(&self) -> BlockId {
        ENTRY_BLOCK_ID
    }

    fn is_loop_head(&self, block_id: BlockId) -> bool {
        self.loop_heads.contains_key(&block_id)
    }

    fn is_back_edge(&self, cur: BlockId, next: BlockId) -> bool {
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
