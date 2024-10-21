// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfgir::{
        ast::{BasicBlock, BasicBlocks, BlockInfo, LoopEnd, LoopInfo},
        remove_no_ops,
    },
    diagnostics::Diagnostics,
    hlir::ast::{Command, Command_, Label},
    shared::ast_debug::*,
};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque},
    fmt::Debug,
    ops::Deref,
};

//**************************************************************************************************
// CFG
//**************************************************************************************************

pub trait CFG {
    fn successors(&self, label: Label) -> &BTreeSet<Label>;

    fn predecessors(&self, label: Label) -> &BTreeSet<Label>;
    fn commands<'a>(&'a self, label: Label) -> Box<dyn Iterator<Item = (usize, &'a Command)> + 'a>;
    fn num_blocks(&self) -> usize;
    fn start_block(&self) -> Label;

    fn next_block(&self, label: Label) -> Option<Label>;

    fn is_loop_head(&self, label: Label) -> bool;

    fn is_back_edge(&self, cur: Label, next: Label) -> bool;

    fn debug(&self);
}

//**************************************************************************************************
// Forward Traversal CFG
//**************************************************************************************************

#[derive(Debug)]
pub struct ForwardCFG<Blocks: Deref<Target = BasicBlocks>> {
    start: Label,
    blocks: Blocks,
    successor_map: BTreeMap<Label, BTreeSet<Label>>,
    predecessor_map: BTreeMap<Label, BTreeSet<Label>>,
    traversal_order: Vec<Label>,
    traversal_successors: BTreeMap<Label, Label>,
    loop_heads: BTreeMap<Label, BTreeSet<Label>>,
}

pub type MutForwardCFG<'a> = ForwardCFG<&'a mut BasicBlocks>;
pub type ImmForwardCFG<'a> = ForwardCFG<&'a BasicBlocks>;

impl<Blocks: Deref<Target = BasicBlocks>> ForwardCFG<Blocks> {
    /// Recomputes successor/predecessor maps. returns dead blocks
    fn recompute_impl(&mut self) -> Vec<Label> {
        let blocks = &self.blocks;
        let mut seen = BTreeSet::new();
        let mut work_list = VecDeque::new();
        seen.insert(self.start);
        work_list.push_back(self.start);

        // build successor map from reachable code
        let mut successor_map = BTreeMap::new();
        while let Some(label) = work_list.pop_front() {
            let last_cmd = blocks.get(&label).unwrap().back().unwrap();
            let successors = last_cmd.value.successors();
            for successor in &successors {
                if !seen.contains(successor) {
                    seen.insert(*successor);
                    work_list.push_back(*successor)
                }
            }
            let old = successor_map.insert(label, successors);
            assert!(old.is_none());
        }

        // build inverse map
        let mut predecessor_map = successor_map
            .keys()
            .cloned()
            .map(|lbl| (lbl, BTreeSet::new()))
            .collect::<BTreeMap<_, _>>();
        for (parent, children) in &successor_map {
            for child in children {
                predecessor_map.get_mut(child).unwrap().insert(*parent);
            }
        }
        self.successor_map = successor_map;
        self.predecessor_map = predecessor_map;

        let (mut post_order, back_edges) = post_order_traversal(
            self.start,
            blocks.keys().copied(),
            &self.successor_map,
            /* include_dead_code */ false,
        );

        self.traversal_order = {
            post_order.reverse();
            post_order
        };
        assert_eq!(self.traversal_order[0], self.start);
        // build a mapping from a block id to the next block id in the traversal order
        self.traversal_successors = self
            .traversal_order
            .windows(2)
            .map(|window| {
                debug_assert!(window.len() == 2);
                (window[0], window[1])
            })
            .collect();
        self.loop_heads = BTreeMap::new();
        for (id, loop_head) in back_edges {
            debug_assert!(id.0 >= loop_head.0);
            self.loop_heads.entry(loop_head).or_default().insert(id);
        }

        // determine dead blocks
        let mut dead_block_labels = vec![];
        for label in self.blocks.keys() {
            if !self.successor_map.contains_key(label) {
                assert!(!self.predecessor_map.contains_key(label));
                assert!(!self.traversal_successors.contains_key(label));
                dead_block_labels.push(*label);
            }
        }

        dead_block_labels
    }

    pub fn blocks(&self) -> &BasicBlocks {
        self.blocks.deref()
    }

    pub fn block(&self, label: Label) -> &BasicBlock {
        self.blocks.deref().get(&label).unwrap()
    }

    pub fn display_blocks(&self) {
        for (lbl, block) in self.blocks() {
            println!("--BLOCK {}--", lbl);
            for cmd in block {
                println!("{:#?}", cmd.value);
            }
            println!();
        }
    }
}

impl<'a> MutForwardCFG<'a> {
    // Returns
    // - A CFG
    // - A set of infinite loop heads
    // - and any errors resulting from building the CFG
    pub fn new<'info>(
        start: Label,
        blocks: &'a mut BasicBlocks,
        block_info: impl IntoIterator<Item = (&'info Label, &'info BlockInfo)>,
    ) -> (Self, BTreeSet<Label>, Diagnostics) {
        let mut cfg = ForwardCFG {
            start,
            blocks,
            successor_map: BTreeMap::new(),
            predecessor_map: BTreeMap::new(),
            traversal_order: vec![],
            traversal_successors: BTreeMap::new(),
            loop_heads: BTreeMap::new(),
        };
        remove_no_ops::optimize(&mut cfg);

        let diags = Diagnostics::new();
        // remove dead code because we already warned about it
        let _ = cfg.recompute();

        let infinite_loop_starts = determine_infinite_loop_starts(&cfg, block_info);
        (cfg, infinite_loop_starts, diags)
    }

    /// Recomputes successor/predecessor maps. returns removed dead blocks
    pub fn recompute(&mut self) -> BasicBlocks {
        let dead_code_labels = self.recompute_impl();
        dead_code_labels
            .into_iter()
            .map(|lbl| (lbl, self.blocks.remove(&lbl).unwrap()))
            .collect()
    }

    pub fn block_mut(&mut self, label: Label) -> &mut BasicBlock {
        self.blocks.get_mut(&label).unwrap()
    }

    pub fn blocks_mut(&mut self) -> &mut BasicBlocks {
        self.blocks
    }
}

impl<'a> ImmForwardCFG<'a> {
    /// Returns
    /// - A CFG
    /// - A set of infinite loop heads
    ///
    /// This _must_ be called after `BlockMutCFG::new`, as the mutable version optimizes the code
    /// This will be done for external usage,
    /// since the Mut CFG is used during the building of the cfgir::ast::Program
    pub fn new<'info>(
        start: Label,
        blocks: &'a BasicBlocks,
        block_info: impl IntoIterator<Item = (&'info Label, &'info BlockInfo)>,
    ) -> (Self, BTreeSet<Label>) {
        let mut cfg = ForwardCFG {
            start,
            blocks,
            successor_map: BTreeMap::new(),
            predecessor_map: BTreeMap::new(),
            traversal_order: vec![],
            traversal_successors: BTreeMap::new(),
            loop_heads: BTreeMap::new(),
        };
        cfg.recompute_impl();

        let infinite_loop_starts = determine_infinite_loop_starts(&cfg, block_info);
        (cfg, infinite_loop_starts)
    }
}

impl<T: Deref<Target = BasicBlocks>> CFG for ForwardCFG<T> {
    fn successors(&self, label: Label) -> &BTreeSet<Label> {
        self.successor_map.get(&label).unwrap()
    }

    fn predecessors(&self, label: Label) -> &BTreeSet<Label> {
        self.predecessor_map.get(&label).unwrap()
    }

    fn commands<'s>(&'s self, label: Label) -> Box<dyn Iterator<Item = (usize, &'s Command)> + 's> {
        Box::new(self.block(label).iter().enumerate())
    }

    fn num_blocks(&self) -> usize {
        self.blocks.len()
    }

    fn start_block(&self) -> Label {
        self.start
    }

    fn next_block(&self, label: Label) -> Option<Label> {
        self.traversal_successors.get(&label).copied()
    }

    fn is_loop_head(&self, label: Label) -> bool {
        self.loop_heads.contains_key(&label)
    }

    fn is_back_edge(&self, cur: Label, next: Label) -> bool {
        self.loop_heads
            .get(&next)
            .map_or(false, |back_edge_predecessors| {
                back_edge_predecessors.contains(&cur)
            })
    }

    fn debug(&self) {
        self.print();
    }
}

// Relying on the ordered block info (ordered in the linear ordering of the source code)
// Determines the infinite loop starts
// This cannot be determined in earlier passes due to dead code
fn determine_infinite_loop_starts<'a, T: Deref<Target = BasicBlocks>>(
    cfg: &ForwardCFG<T>,
    block_info: impl IntoIterator<Item = (&'a Label, &'a BlockInfo)>,
) -> BTreeSet<Label> {
    // Filter dead code
    let block_info = block_info
        .into_iter()
        .filter(|(lbl, _info)| cfg.blocks().contains_key(lbl))
        .collect::<Vec<_>>();

    // Fully populate infinite loop starts to be pruned later
    // And for any block, determine the current loop
    let mut infinite_loop_starts = BTreeSet::new();

    let mut loop_stack: Vec<(Label, LoopEnd)> = vec![];
    let mut current_loop_info = Vec::with_capacity(block_info.len());
    for (lbl, info) in &block_info {
        match loop_stack.last() {
            Some((_, cur_loop_end)) if cur_loop_end.equals(**lbl) => {
                loop_stack.pop();
            }
            _ => (),
        }

        match info {
            BlockInfo::Other => (),
            BlockInfo::LoopHead(LoopInfo { is_loop_stmt, .. }) if !*is_loop_stmt => (),
            BlockInfo::LoopHead(LoopInfo { loop_end, .. }) => {
                infinite_loop_starts.insert(**lbl);
                loop_stack.push((**lbl, *loop_end))
            }
        }

        current_loop_info.push(loop_stack.last().cloned());
    }

    // Given the loop info for any block, determine which loops are infinite
    // Each 'loop' based loop starts in the set, and is removed if it's break is used, or if a
    // return or abort is used
    let mut prev_opt: Option<Label> = None;
    let zipped =
        block_info
            .into_iter()
            .zip(current_loop_info)
            .filter_map(|(block_info, cur_loop_opt)| {
                cur_loop_opt.map(|cur_loop| (block_info, cur_loop))
            });
    for ((lbl, _info), (cur_loop_start, cur_loop_end)) in zipped {
        debug_assert!(prev_opt.map(|prev| prev.0 < lbl.0).unwrap_or(true));
        maybe_unmark_infinite_loop_starts(
            &mut infinite_loop_starts,
            cur_loop_start,
            cur_loop_end,
            &cfg.blocks()[lbl],
        );
        prev_opt = Some(*lbl);
    }

    infinite_loop_starts
}

fn maybe_unmark_infinite_loop_starts(
    infinite_loop_starts: &mut BTreeSet<Label>,
    cur_loop_start: Label,
    cur_loop_end: LoopEnd,
    block: &BasicBlock,
) {
    use Command_ as C;
    // jumps/return/abort are only found at the end of the block
    match &block.back().unwrap().value {
        C::Jump { target, .. } if cur_loop_end.equals(*target) => {
            infinite_loop_starts.remove(&cur_loop_start);
        }
        C::JumpIf {
            if_true, if_false, ..
        } if cur_loop_end.equals(*if_true) || cur_loop_end.equals(*if_false) => {
            infinite_loop_starts.remove(&cur_loop_start);
        }
        C::VariantSwitch { arms, .. }
            if arms.iter().any(|(_, target)| cur_loop_end.equals(*target)) =>
        {
            infinite_loop_starts.remove(&cur_loop_start);
        }
        C::Return { .. } | C::Abort(_, _) => {
            infinite_loop_starts.remove(&cur_loop_start);
        }

        C::Jump { .. }
        | C::JumpIf { .. }
        | C::VariantSwitch { .. }
        | C::Assign(_, _, _)
        | C::Mutate(_, _)
        | C::IgnoreAndPop { .. } => (),
        C::Break(_) | C::Continue(_) => panic!("ICE break/continue not translated to jumps"),
    }
}

fn post_order_traversal(
    start: Label,
    all_labels: impl IntoIterator<Item = Label>,
    successor_map: &BTreeMap<Label, BTreeSet<Label>>,
    include_dead_code: bool,
) -> (
    /* order */ Vec<Label>,
    /* back edges */ Vec<(Label, Label)>,
) {
    fn is_back_edge(cur: Label, target: Label) -> bool {
        target.0 <= cur.0
    }
    // Determine traversal order
    // build a DAG subgraph (remove the loop back edges)
    let dag: BTreeMap<Label, BTreeSet<Label>> = successor_map
        .iter()
        .map(|(node, successors)| {
            let node = *node;
            let non_loop_continue_successors = successors
                .iter()
                // remove the loop back edges
                .filter(|successor| !is_back_edge(node, **successor))
                .copied()
                .collect();
            (node, non_loop_continue_successors)
        })
        .collect();

    // build the post-order traversal
    let mut post_order = Vec::with_capacity(dag.len());
    let mut finished = BTreeSet::new();
    let mut stack = vec![(start, /* is_first_visit */ true)];
    let mut remaining = all_labels
        .into_iter()
        .map(Reverse)
        .collect::<BinaryHeap<_>>();
    while let Some((cur, is_first_visit)) = stack.pop() {
        if is_first_visit {
            stack.push((cur, false));
            stack.extend(
                dag[&cur]
                    .iter()
                    .filter(|successor| !finished.contains(*successor))
                    .map(|successor| (*successor, /* is_first_visit */ true)),
            );
        } else {
            debug_assert!(dag[&cur]
                .iter()
                .all(|successor| finished.contains(successor)));
            if finished.insert(cur) {
                post_order.push(cur)
            }
        }
        if include_dead_code {
            // if dead code needs to be visited...
            if stack.is_empty() {
                // find the minimum label that has not been visited
                let next_opt = loop {
                    match remaining.pop() {
                        Some(next) if finished.contains(&next.0) => continue,
                        next_opt => break next_opt.map(|rev| rev.0),
                    }
                };
                // add that min label to the stack and continue
                if let Some(next) = next_opt {
                    debug_assert!(!finished.contains(&next));
                    stack.push((next, true))
                }
            }
        }
    }

    // Determine loop back edges
    let mut back_edges: Vec<(Label, Label)> = vec![];
    for (node, successors) in successor_map {
        let node = *node;
        let loop_continues = successors
            .iter()
            .filter(|successor| is_back_edge(node, **successor))
            .copied();
        for successor in loop_continues {
            back_edges.push((node, successor));
        }
    }

    (post_order, back_edges)
}

//**************************************************************************************************
// Reverse Traversal CFG
//**************************************************************************************************

#[derive(Debug)]
pub struct ReverseCFG<'forward, Blocks: Deref<Target = BasicBlocks>> {
    terminal: Label,
    terminal_block: BasicBlock,
    blocks: &'forward mut Blocks,
    successor_map: &'forward mut BTreeMap<Label, BTreeSet<Label>>,
    predecessor_map: &'forward mut BTreeMap<Label, BTreeSet<Label>>,
    traversal_order: Vec<Label>,
    traversal_successors: BTreeMap<Label, Label>,
    loop_heads: BTreeMap<Label, BTreeSet<Label>>,
}

pub type MutReverseCFG<'forward, 'blocks> = ReverseCFG<'forward, &'blocks mut BasicBlocks>;
pub type ImmReverseCFG<'forward, 'blocks> = ReverseCFG<'forward, &'blocks BasicBlocks>;

impl<'forward, Blocks: Deref<Target = BasicBlocks>> ReverseCFG<'forward, Blocks> {
    pub fn new(
        forward_cfg: &'forward mut ForwardCFG<Blocks>,
        infinite_loop_starts: &BTreeSet<Label>,
    ) -> Self
    where
        Blocks: Debug,
    {
        let blocks = &mut forward_cfg.blocks;
        let forward_successors = &mut forward_cfg.successor_map;
        let forward_predecessor = &mut forward_cfg.predecessor_map;
        let end_blocks = {
            let mut end_blocks = BTreeSet::new();
            for (lbl, successors) in forward_successors.iter() {
                let loop_start_successors = successors
                    .iter()
                    .filter(|l| infinite_loop_starts.contains(l));
                for loop_start_successor in loop_start_successors {
                    if lbl >= loop_start_successor {
                        end_blocks.insert(*lbl);
                    }
                }
            }
            for (lbl, block) in blocks.iter() {
                let last_cmd = block.back().unwrap();
                if last_cmd.value.is_exit() {
                    end_blocks.insert(*lbl);
                }
            }
            end_blocks
        };

        // setup fake terminal block that will act as the start node in reverse traversal
        let terminal = Label(blocks.keys().map(|lbl| lbl.0).max().unwrap_or(0) + 1);
        assert!(!blocks.contains_key(&terminal), "{:#?}", blocks);
        for terminal_predecessor in &end_blocks {
            forward_successors
                .entry(*terminal_predecessor)
                .or_default()
                .insert(terminal);
        }
        forward_predecessor.insert(terminal, end_blocks);
        // ensure map is not partial
        forward_successors.insert(terminal, BTreeSet::new());

        let (post_order, back_edges) = post_order_traversal(
            forward_cfg.start,
            blocks.keys().copied().chain(std::iter::once(terminal)),
            forward_successors,
            /* include_dead_code */ false,
        );
        let successor_map = forward_predecessor;
        let predecessor_map = forward_successors;
        let traversal_order = post_order;
        let traversal_successors = traversal_order
            .windows(2)
            .map(|window| {
                debug_assert!(window.len() == 2);
                (window[0], window[1])
            })
            .collect();
        let mut loop_heads: BTreeMap<Label, BTreeSet<Label>> = BTreeMap::new();
        for (id, forward_loop_head) in back_edges {
            debug_assert!(id.0 >= forward_loop_head.0);
            loop_heads.entry(id).or_default().insert(forward_loop_head);
        }
        let res = Self {
            terminal,
            terminal_block: BasicBlock::new(),
            blocks,
            successor_map,
            predecessor_map,
            traversal_order,
            traversal_successors,
            loop_heads,
        };
        for l in res.blocks.keys() {
            if l != &forward_cfg.start && !res.traversal_successors.contains_key(l) {
                res.debug();
                panic!("ICE {} not in traversal", l);
            }
        }
        res
    }

    pub fn blocks(&self) -> impl Iterator<Item = (&Label, &BasicBlock)> {
        self.blocks
            .iter()
            .chain(std::iter::once((&self.terminal, &self.terminal_block)))
    }

    pub fn block(&self, label: Label) -> &BasicBlock {
        if label == self.terminal {
            &self.terminal_block
        } else {
            self.blocks.get(&label).unwrap()
        }
    }
}

impl<'forward, 'blocks> MutReverseCFG<'forward, 'blocks> {
    pub fn block_mut(&mut self, label: Label) -> &mut BasicBlock {
        if label == self.terminal {
            &mut self.terminal_block
        } else {
            self.blocks.get_mut(&label).unwrap()
        }
    }

    pub fn blocks_mut(&mut self) -> impl Iterator<Item = (&Label, &mut BasicBlock)> {
        self.blocks
            .iter_mut()
            .chain(std::iter::once((&self.terminal, &mut self.terminal_block)))
    }
}

impl<'forward, Blocks: Deref<Target = BasicBlocks>> Drop for ReverseCFG<'forward, Blocks> {
    fn drop(&mut self) {
        assert!(self.terminal_block.is_empty());
        let start_predecessors = self.predecessor_map.remove(&self.terminal);
        assert!(
            start_predecessors.is_some(),
            "ICE missing start node from predecessors"
        );
        let start_successors = self.successor_map.remove(&self.terminal).unwrap();
        for start_successor in start_successors {
            self.predecessor_map
                .get_mut(&start_successor)
                .unwrap()
                .remove(&self.terminal);
        }
    }
}

impl<'forward, Blocks: Deref<Target = BasicBlocks>> CFG for ReverseCFG<'forward, Blocks> {
    fn successors(&self, label: Label) -> &BTreeSet<Label> {
        self.successor_map.get(&label).unwrap()
    }

    fn predecessors(&self, label: Label) -> &BTreeSet<Label> {
        self.predecessor_map.get(&label).unwrap()
    }

    fn commands<'s>(&'s self, label: Label) -> Box<dyn Iterator<Item = (usize, &'s Command)> + 's> {
        Box::new(self.block(label).iter().enumerate().rev())
    }

    fn num_blocks(&self) -> usize {
        // + 1 for the terminal
        self.blocks.len() + 1
    }

    fn start_block(&self) -> Label {
        self.traversal_order[0]
    }

    fn next_block(&self, label: Label) -> Option<Label> {
        self.traversal_successors.get(&label).copied()
    }

    fn is_loop_head(&self, label: Label) -> bool {
        self.loop_heads.contains_key(&label)
    }

    fn is_back_edge(&self, cur: Label, next: Label) -> bool {
        self.loop_heads
            .get(&next)
            .map_or(false, |back_edge_predecessors| {
                back_edge_predecessors.contains(&cur)
            })
    }

    fn debug(&self) {
        self.print();
    }
}

//**************************************************************************************************
// Debug
//**************************************************************************************************

impl<T: Deref<Target = BasicBlocks>> AstDebug for ForwardCFG<T> {
    fn ast_debug(&self, w: &mut AstWriter) {
        let ForwardCFG {
            start,
            blocks,
            successor_map,
            predecessor_map,
            traversal_order,
            traversal_successors: _,
            loop_heads,
        } = self;
        w.writeln("--BlockCFG--");
        ast_debug_cfg(
            w,
            *start,
            blocks,
            successor_map.iter(),
            predecessor_map.iter(),
            traversal_order.windows(2).map(|w| (&w[0], &w[1])),
            loop_heads.iter(),
        );
    }
}

impl<'a, T: Deref<Target = BasicBlocks>> AstDebug for ReverseCFG<'a, T> {
    fn ast_debug(&self, w: &mut AstWriter) {
        let ReverseCFG {
            terminal,
            terminal_block: _,
            blocks,
            successor_map,
            predecessor_map,
            traversal_order,
            traversal_successors: _,
            loop_heads,
        } = self;
        w.writeln("--ReverseBlockCFG--");
        w.writeln(format!("terminal: {}", terminal));
        ast_debug_cfg(
            w,
            traversal_order[0],
            blocks,
            successor_map.iter(),
            predecessor_map.iter(),
            traversal_order.windows(2).map(|w| (&w[0], &w[1])),
            loop_heads.iter(),
        );
    }
}

fn ast_debug_cfg<'a>(
    w: &mut AstWriter,
    start: Label,
    blocks: &BasicBlocks,
    successor_map: impl Iterator<Item = (&'a Label, &'a BTreeSet<Label>)>,
    predecessor_map: impl Iterator<Item = (&'a Label, &'a BTreeSet<Label>)>,
    traversal: impl Iterator<Item = (&'a Label, &'a Label)>,
    loop_heads: impl Iterator<Item = (&'a Label, &'a BTreeSet<Label>)>,
) {
    w.write("successor_map:");
    w.indent(4, |w| {
        for (lbl, nexts) in successor_map {
            w.write(format!("{} => [", lbl));
            w.comma(nexts, |w, next| w.write(format!("{}", next)));
            w.writeln("]")
        }
    });

    w.write("predecessor_map:");
    w.indent(4, |w| {
        for (lbl, nexts) in predecessor_map {
            w.write(format!("{} <= [", lbl));
            w.comma(nexts, |w, next| w.write(format!("{}", next)));
            w.writeln("]")
        }
    });

    w.write("traversal:");
    w.indent(4, |w| {
        for (cur, next) in traversal {
            w.writeln(format!("{} => {}", cur, next))
        }
    });

    w.write("loop heads:");
    w.indent(4, |w| {
        for (loop_head, back_edge_predecessors) in loop_heads {
            for pred in back_edge_predecessors {
                w.writeln(format!(
                    "loop head: {}. back edge predecessor: {}",
                    loop_head, pred
                ))
            }
        }
    });

    w.writeln(format!("start: {}", start));
    w.writeln("blocks:");
    w.indent(4, |w| blocks.ast_debug(w));
}
