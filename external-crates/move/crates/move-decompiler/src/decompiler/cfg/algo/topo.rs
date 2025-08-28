// Copyright (c) Verichains, 2023

use crate::decompiler::cfg::metadata::WithMetadata;

use super::{super::datastructs::*, graph::*};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone)]
pub enum TopoSortedBlockItem<BlockContent: BlockContentTrait> {
    SubBlock(Box<TopoSortedBlocks<BlockContent>>),
    Blocks(Box<Vec<WithMetadata<BasicBlock<usize, BlockContent>>>>),
}
impl<BlockContent: BlockContentTrait> TopoSortedBlockItem<BlockContent> {
    pub(crate) fn block_count(&self) -> usize {
        match self {
            TopoSortedBlockItem::SubBlock(sub) => sub.block_count(),
            TopoSortedBlockItem::Blocks(blocks) => blocks.len(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TopoSortedBlocks<BlockContent: BlockContentTrait> {
    pub entry: usize,
    pub exit: Option<usize>,
    pub blocks: Vec<TopoSortedBlockItem<BlockContent>>,
}

impl<BlockContent: BlockContentTrait> TopoSortedBlocks<BlockContent> {
    #[allow(dead_code)]
    pub fn from_blocks(
        blocks: Vec<WithMetadata<BasicBlock<usize, BlockContent>>>,
        entry: usize,
    ) -> TopoSortedBlocks<BlockContent> {
        TopoSortedBlocks {
            entry,
            exit: None,
            blocks: vec![TopoSortedBlockItem::Blocks(Box::new(blocks))],
        }
    }

    pub fn block_count(&self) -> usize {
        self.blocks.iter().map(|x| x.block_count()).sum()
    }

    pub fn clone_flatten(&self) -> Vec<WithMetadata<BasicBlock<usize, BlockContent>>> {
        let mut result = Vec::new();
        for item in self.blocks.iter() {
            match item {
                TopoSortedBlockItem::Blocks(blocks) => {
                    result.extend(blocks.iter().cloned());
                }
                TopoSortedBlockItem::SubBlock(sub) => {
                    result.extend(sub.clone_flatten());
                }
            }
        }
        result
    }

    fn for_each_block_dispatch<F: FnMut(&WithMetadata<BasicBlock<usize, BlockContent>>)>(
        &self,
        f: &mut F,
    ) {
        for item in self.blocks.iter() {
            match item {
                TopoSortedBlockItem::Blocks(blocks) => {
                    for block in blocks.iter() {
                        f(block);
                    }
                }
                TopoSortedBlockItem::SubBlock(sub) => {
                    sub.for_each_block_dispatch(f);
                }
            }
        }
    }

    pub fn for_each_block<F: FnMut(&WithMetadata<BasicBlock<usize, BlockContent>>)>(
        &self,
        mut f: F,
    ) {
        self.for_each_block_dispatch(&mut f)
    }

    fn for_each_block_mut_dispatch<F: FnMut(&mut WithMetadata<BasicBlock<usize, BlockContent>>)>(
        &mut self,
        f: &mut F,
    ) {
        for item in self.blocks.iter_mut() {
            match item {
                TopoSortedBlockItem::Blocks(blocks) => {
                    for block in blocks.iter_mut() {
                        f(block);
                    }
                }
                TopoSortedBlockItem::SubBlock(sub) => {
                    sub.for_each_block_mut_dispatch(f);
                }
            }
        }
    }

    pub fn for_each_block_mut<F: FnMut(&mut WithMetadata<BasicBlock<usize, BlockContent>>)>(
        &mut self,
        mut f: F,
    ) {
        self.for_each_block_mut_dispatch(&mut f)
    }

    fn for_each_block_mut_check_error_dispatch<
        F: FnMut(&mut WithMetadata<BasicBlock<usize, BlockContent>>) -> Result<(), anyhow::Error>,
    >(
        &mut self,
        f: &mut F,
    ) -> Result<(), anyhow::Error> {
        for item in self.blocks.iter_mut() {
            match item {
                TopoSortedBlockItem::Blocks(blocks) => {
                    for block in blocks.iter_mut() {
                        if let Err(e) = f(block) {
                            return Err(e);
                        }
                    }
                }
                TopoSortedBlockItem::SubBlock(sub) => {
                    sub.for_each_block_mut_check_error_dispatch(f)?;
                }
            }
        }
        Ok(())
    }

    pub fn for_each_block_mut_check_error<
        F: FnMut(&mut WithMetadata<BasicBlock<usize, BlockContent>>) -> Result<(), anyhow::Error>,
    >(
        &mut self,
        mut f: F,
    ) -> Result<(), anyhow::Error> {
        self.for_each_block_mut_check_error_dispatch(&mut f)
    }

    pub fn find_block_mut(
        &mut self,
        idx: usize,
    ) -> Option<&mut WithMetadata<BasicBlock<usize, BlockContent>>> {
        for item in self.blocks.iter_mut() {
            match item {
                TopoSortedBlockItem::Blocks(blocks) => {
                    for block in blocks.iter_mut() {
                        if block.idx == idx {
                            return Some(block);
                        }
                    }
                }
                TopoSortedBlockItem::SubBlock(sub) => {
                    if let Some(block) = sub.find_block_mut(idx) {
                        return Some(block);
                    }
                }
            }
        }
        None
    }
}

#[derive(Debug, Clone)]
enum TopoSortedBlockOrderItem {
    SubBlock(Box<TopoSortedBlockOrder>),
    Blocks(Box<Vec<usize>>),
}

#[derive(Debug, Clone)]
struct TopoSortedBlockOrder {
    entry: usize,
    exit: Option<usize>,
    order: Vec<TopoSortedBlockOrderItem>,
}

impl TopoSortedBlockOrder {
    fn new(entry: usize) -> Self {
        Self {
            order: Vec::new(),
            entry,
            exit: None,
        }
    }

    fn push_item(&mut self, item: usize) {
        self.order
            .push(TopoSortedBlockOrderItem::Blocks(Box::new(vec![item])));
    }

    fn push_sub(&mut self, mut other: TopoSortedBlockOrder, exit: Option<usize>) {
        other.exit = exit;
        self.order
            .push(TopoSortedBlockOrderItem::SubBlock(Box::new(other)));
    }

    fn flatten(&self) -> Vec<usize> {
        let mut result = Vec::new();
        for item in self.order.iter() {
            match item {
                TopoSortedBlockOrderItem::Blocks(blocks) => {
                    result.extend(blocks.iter().cloned());
                }
                TopoSortedBlockOrderItem::SubBlock(sub) => {
                    result.extend(sub.flatten());
                }
            }
        }
        result
    }

    #[cfg(debug_assertions)]
    #[allow(dead_code)]
    fn debug_dump(&self, level: usize) {
        for item in self.order.iter() {
            match item {
                TopoSortedBlockOrderItem::Blocks(blocks) => {
                    println!("{}blocks: {:?}", "  ".repeat(level), blocks);
                }
                TopoSortedBlockOrderItem::SubBlock(sub) => {
                    sub.debug_dump(level + 1);
                }
            }
        }
    }
}

#[derive(Debug)]
struct SuperGraph {
    tarjan: TarjanScc,
    g: Graph,
    g_entry: usize,
    g_in_degree: Vec<usize>,
    g_out_degree: Vec<usize>,
    g_node_2_original_node_entries: HashMap<usize, HashSet<usize>>,
    g_node_2_original_node_exits: HashMap<usize, HashSet<usize>>,
}

impl SuperGraph {
    // the `view` must be corresponding to the `original_graph`, not to define a new subset
    fn new(
        original_graph: &Graph,
        subgraph: &Graph,
        dummy_dispatch_blocks: &HashMap<usize, usize>,
        view_stack: &Option<&Vec<&HashSet<usize>>>,
        view: &Option<&HashSet<usize>>,
        entry: usize,
    ) -> Self {
        debug_assert!(
            view.map_or(true, |view| view.contains(&entry)),
            "entry not in view"
        );

        let mut super_graph = Graph::new();
        let mut node_entries = HashMap::new();
        let mut node_external_exits = HashMap::new();

        // join dummy dispatch blocks into its parent
        // only add one layer of dummy dispatch blocks
        let mut subgraph = subgraph.clone();
        let mut add_edges = Vec::new();

        for &u in subgraph.nodes().iter() {
            if let Some(v) = dummy_dispatch_blocks.get(&u) {
                let mut visible_height = usize::MAX;
                view_stack.as_ref().map(|view_stack| {
                    for (idx, view) in view_stack.iter().rev().enumerate() {
                        if view.contains(&v) {
                            visible_height = idx;
                            break;
                        }
                    }
                });
                if visible_height <= 1 && subgraph.reverse_edges(u).count() == 1 {
                    let prev = *subgraph.reverse_edges(u).next().unwrap();
                    add_edges.push((u, prev));
                }
            }
        }

        for (u, v) in add_edges {
            subgraph.add_edge(u, v);
        }

        let tarjan = TarjanScc::new(&subgraph);

        for &u in subgraph
            .nodes()
            .iter()
            .filter(|&u| view.map_or(true, |view| view.contains(u)))
        {
            let u_scc = tarjan.scc_for_node(u).unwrap().0;
            super_graph.ensure_node(u_scc);
            for &v in subgraph
                .edges(u)
                .filter(|&v| view.map_or(true, |view| view.contains(v)))
            {
                let v_scc = tarjan.scc_for_node(v).unwrap().0;
                if u_scc != v_scc {
                    super_graph.add_edge(u_scc, v_scc);
                    node_entries
                        .entry(v_scc)
                        .or_insert_with(HashSet::new)
                        .insert(v);
                    node_external_exits
                        .entry(u_scc)
                        .or_insert_with(HashSet::new)
                        .insert(v);
                }
            }
        }

        for &u in original_graph
            .nodes()
            .iter()
            .filter(|&u| view.map_or(true, |view| view.contains(u)))
        {
            let u_scc = tarjan.scc_for_node(u).unwrap().0;
            for &v in original_graph.edges(u) {
                if !view.map_or(true, |view| view.contains(&v)) {
                    node_external_exits
                        .entry(u_scc)
                        .or_insert_with(HashSet::new)
                        .insert(v);
                }
            }
        }

        let super_graph_entry = tarjan.scc_for_node(entry).unwrap().0;
        node_entries
            .entry(super_graph_entry)
            .or_insert_with(HashSet::new)
            .insert(entry);

        let mut g_in_degree = vec![0; super_graph.nodes().len()];
        for &idx in super_graph.nodes() {
            g_in_degree[idx] = super_graph.in_degree(idx);
        }

        let mut g_out_degree = vec![0; super_graph.nodes().len()];
        for &idx in super_graph.nodes() {
            g_out_degree[idx] = super_graph.out_degree(idx);
        }

        Self {
            tarjan,
            g: super_graph,
            g_entry: super_graph_entry,
            g_node_2_original_node_entries: node_entries,
            g_node_2_original_node_exits: node_external_exits,
            g_in_degree,
            g_out_degree,
        }
    }

    fn scc_idx_for_node_unchecked(&self, node: usize) -> usize {
        self.tarjan.scc_for_node(node).unwrap().0
    }
}

#[derive(Debug, Clone, Default)]
struct DominatorInfo {
    descendant_ret_cnt: usize,
    descendant_abort_cnt: usize,
}

#[derive(Debug, Clone, Default)]
struct NodeInfo {
    idx: usize,
    dominator: Option<DominatorInfo>,
}

#[derive(Debug)]
struct CFGTopo<'a, BlockContent: BlockContentTrait> {
    blocks: &'a Vec<WithMetadata<BasicBlock<usize, BlockContent>>>,
    dummy_dispatch_blocks: HashMap<usize, usize>,
    graph: Graph,
    super_graph: Option<SuperGraph>,
    node_info: Vec<NodeInfo>,
}

impl<'a, BlockContent: BlockContentTrait + 'a> CFGTopo<'a, BlockContent> {
    fn new(blocks: &'a Vec<WithMetadata<BasicBlock<usize, BlockContent>>>) -> Self {
        Self {
            blocks,
            dummy_dispatch_blocks: Default::default(),
            graph: Graph::new(),
            super_graph: None,
            node_info: Vec::new(),
        }
    }

    fn apply_order(&self, order: &TopoSortedBlockOrder) -> TopoSortedBlocks<BlockContent> {
        let (blocks, rev_order) = self.rewrite_blocks_to_new_order(order);

        Self::recursive_apply_order(&blocks, &rev_order, order)
    }

    fn recursive_apply_order(
        linear_blocks: &[WithMetadata<BasicBlock<usize, BlockContent>>],
        rev_order: &[usize],
        order: &TopoSortedBlockOrder,
    ) -> TopoSortedBlocks<BlockContent> {
        let mut result = TopoSortedBlocks {
            blocks: Vec::new(),
            entry: rev_order[order.entry],
            exit: order.exit.map(|x| rev_order[x]).clone(),
        };
        for item in order.order.iter() {
            match item {
                TopoSortedBlockOrderItem::Blocks(blocks) => {
                    result.blocks.push(TopoSortedBlockItem::Blocks(Box::new(
                        blocks
                            .iter()
                            .map(|&idx| linear_blocks[idx].clone())
                            .collect::<Vec<_>>(),
                    )));
                }
                TopoSortedBlockOrderItem::SubBlock(sub) => {
                    result.blocks.push(TopoSortedBlockItem::SubBlock(Box::new(
                        Self::recursive_apply_order(linear_blocks, rev_order, sub),
                    )));
                }
            }
        }
        result
    }

    fn rewrite_blocks_to_new_order(
        &self,
        order: &TopoSortedBlockOrder,
    ) -> (
        Vec<WithMetadata<BasicBlock<usize, BlockContent>>>,
        Vec<usize>,
    ) {
        let mut result = Vec::new();
        let mut rev_order = vec![usize::MAX; self.blocks.len()];
        let flattened_order = order.flatten();
        for (idx, &order_idx) in flattened_order.iter().enumerate() {
            rev_order[order_idx] = idx;
        }
        let rev_order = rev_order;

        for block in self.blocks.iter() {
            let mut block = block.clone();
            block.idx = rev_order[block.idx];
            block.next = match &block.inner().next {
                Terminator::IfElse {
                    if_block,
                    else_block,
                } => Terminator::IfElse {
                    if_block: if_block.with_target(rev_order[if_block.target]),
                    else_block: else_block.with_target(rev_order[else_block.target]),
                },
                Terminator::Ret => Terminator::Ret,
                Terminator::Abort => Terminator::Abort,
                Terminator::Normal => Terminator::Normal,
                Terminator::While {
                    inner_block,
                    outer_block,
                    content_blocks,
                } => Terminator::While {
                    inner_block: rev_order[*inner_block],
                    outer_block: rev_order[*outer_block],
                    content_blocks: content_blocks.iter().map(|&x| rev_order[x]).collect(),
                },
                Terminator::Branch { target } => Terminator::Branch {
                    target: rev_order[*target],
                },
                Terminator::Break { target } => Terminator::Break {
                    target: rev_order[*target],
                },
                Terminator::Continue { target } => Terminator::Continue {
                    target: rev_order[*target],
                },
            };
            if let Some((idx, contents)) = &block.unconditional_loop_entry {
                let new_idx = if *idx != usize::MAX {
                    rev_order[*idx]
                } else {
                    *idx
                };
                block.unconditional_loop_entry = Some((
                    new_idx,
                    contents
                        .iter()
                        .map(|&x| rev_order[x])
                        .collect::<HashSet<usize>>(),
                ));
            }
            result.push(block);
        }
        (result, rev_order)
    }

    fn create_graph(&mut self, entry: usize) {
        self.graph = Graph::new();
        for (idx, block) in self.blocks.iter().enumerate() {
            self.graph.ensure_node(idx);
            for &&next_idx in block.next.next_blocks().iter() {
                self.graph.add_edge(idx, next_idx);
            }
            if block.is_dummy_dispatch_block {
                self.dummy_dispatch_blocks
                    .insert(idx, **block.next.next_blocks().iter().next().unwrap());
            }
        }
        self.super_graph = Some(SuperGraph::new(
            &self.graph,
            &self.graph,
            &self.dummy_dispatch_blocks,
            &None,
            &None,
            entry,
        ));
    }

    fn annotate_dominator(&mut self, entry: usize) {
        let mut descendant_ret_set = vec![HashSet::new(); self.blocks.len()];
        let mut descendant_abort_set = vec![HashSet::new(); self.blocks.len()];
        for (idx, block) in self.blocks.iter().enumerate() {
            self.node_info[idx].idx = idx;
            match block.next {
                Terminator::Ret => {
                    self.node_info[idx].dominator = Some(DominatorInfo::default());
                    self.node_info[idx]
                        .dominator
                        .as_mut()
                        .unwrap()
                        .descendant_ret_cnt = 1;
                    descendant_ret_set[idx].insert(idx);
                }
                Terminator::Abort => {
                    self.node_info[idx].dominator = Some(DominatorInfo::default());
                    self.node_info[idx]
                        .dominator
                        .as_mut()
                        .unwrap()
                        .descendant_abort_cnt = 1;
                    descendant_abort_set[idx].insert(idx);
                }
                Terminator::IfElse { .. }
                | Terminator::Normal
                | Terminator::While { .. }
                | Terminator::Branch { .. }
                | Terminator::Break { .. }
                | Terminator::Continue { .. } => {}
            };
        }
        let super_graph = SuperGraph::new(
            &self.graph,
            &self.graph,
            &self.dummy_dispatch_blocks,
            &None,
            &None,
            entry,
        );
        let mut out_degree = super_graph.g_out_degree.clone();

        let mut queue = VecDeque::new();

        let mut dominator_infos = vec![DominatorInfo::default(); super_graph.g.nodes().len()];

        for node_info in self.node_info.iter() {
            if node_info.dominator.is_none() {
                continue;
            }

            let idx = node_info.idx;
            let idx_in_super_graph = super_graph.scc_idx_for_node_unchecked(idx);
            queue.push_back(idx_in_super_graph);

            debug_assert!(out_degree[idx_in_super_graph] == 0);

            dominator_infos[idx_in_super_graph] = node_info.dominator.as_ref().unwrap().clone();
        }

        let dominator_nodes: HashSet<usize> = DominatorNodes::for_graph(&super_graph.g, entry);

        while let Some(idx) = queue.pop_front() {
            if dominator_nodes.contains(&idx) {
                let entries = &super_graph.g_node_2_original_node_entries[&idx];
                if entries.len() == 1 {
                    for &entry in entries.iter() {
                        self.node_info[entry].dominator = Some(dominator_infos[idx].clone());
                    }
                }
            }
            let curr_ret_set = descendant_ret_set[idx].clone();
            let curr_abort_set = descendant_abort_set[idx].clone();

            for &prev_idx in super_graph.g.reverse_edges(idx) {
                out_degree[prev_idx] -= 1;

                if prev_idx != idx {
                    descendant_ret_set[prev_idx].extend(&curr_ret_set);
                    descendant_abort_set[prev_idx].extend(&curr_abort_set);

                    dominator_infos[prev_idx].descendant_ret_cnt =
                        descendant_ret_set[prev_idx].len();
                    dominator_infos[prev_idx].descendant_abort_cnt =
                        descendant_abort_set[prev_idx].len();
                }
                if out_degree[prev_idx] == 0 {
                    queue.push_back(prev_idx);
                }
            }
        }
    }

    fn create_and_annotate_nodes_info(&mut self, entry: usize) {
        self.node_info = vec![NodeInfo::default(); self.blocks.len()];
        self.annotate_dominator(entry);
    }

    fn view_without_dead_entries(&self, view: &HashSet<usize>, entry: usize) -> HashSet<usize> {
        let mut queue = VecDeque::new();
        let mut in_degree = vec![0; self.blocks.len()];
        for &idx in view.iter() {
            in_degree[idx] = self.graph.in_degree(idx);
            if in_degree[idx] == 0 {
                queue.push_back(idx);
            }
        }
        let mut new_view = view.clone();
        while let Some(idx) = queue.pop_front() {
            if idx == entry {
                continue;
            }
            new_view.remove(&idx);
            for &next_idx in self.graph.edges(idx) {
                if !new_view.contains(&next_idx) {
                    continue;
                }
                in_degree[next_idx] -= 1;
                if in_degree[next_idx] == 0 {
                    queue.push_back(next_idx);
                }
            }
        }
        new_view
    }

    fn topo_sort_super_graph(
        &self,
        super_graph: &SuperGraph,
        original_entry_scc: &HashSet<usize>,
    ) -> Result<Vec<usize>, anyhow::Error> {
        fn dfs<'a, BlockContent: BlockContentTrait>(
            topo: &CFGTopo<'a, BlockContent>,
            original_entry_scc: &HashSet<usize>,
            result: &mut Vec<usize>,
            super_graph: &SuperGraph,
            in_degree: &mut Vec<usize>,
            u: usize,
        ) {
            result.push(u);
            let mut will_visit = Vec::new();
            for &v in super_graph.g.edges(u) {
                in_degree[v] -= 1;
                if in_degree[v] == 0 {
                    will_visit.push(v);
                }
            }
            will_visit.sort_by_key(|&super_node| {
                let entries: &HashSet<usize> =
                    &super_graph.g_node_2_original_node_entries[&super_node];
                let is_dominator = entries.len() == 1
                    && topo.node_info[*entries.iter().next().unwrap()]
                        .dominator
                        .is_some();
                let in_degree: usize = entries.iter().map(|&x| topo.graph.in_degree(x)).sum();
                let is_in_entry_original_scc =
                    entries.intersection(&original_entry_scc).next().is_some();
                (
                    if is_dominator { 0 } else { 1 },
                    if is_in_entry_original_scc { 1 } else { 0 },
                    in_degree,
                )
            });
            for v in will_visit {
                dfs(topo, original_entry_scc, result, super_graph, in_degree, v);
            }
        }

        let mut result = Vec::new();
        let mut in_degree = super_graph.g_in_degree.clone();

        dfs(
            self,
            original_entry_scc,
            &mut result,
            super_graph,
            &mut in_degree,
            super_graph.g_entry,
        );

        Ok(result)
    }

    fn topo_sort_dag(&self, graph: &Graph, entry: &usize) -> Result<Vec<usize>, anyhow::Error> {
        fn dfs<'a, BlockContent: BlockContentTrait>(
            topo: &CFGTopo<'a, BlockContent>,
            graph: &Graph,
            entry: &usize,
            result: &mut Vec<usize>,
            in_degree: &mut Vec<usize>,
            u: usize,
        ) {
            result.push(u);
            let mut will_visit = Vec::new();
            for &v in graph.edges(u) {
                in_degree[v] -= 1;
                if in_degree[v] == 0 {
                    will_visit.push(v);
                }
            }
            will_visit.sort_by_key(|&node| {
                let is_dominator = topo.node_info[node].dominator.is_some();
                let in_degree: usize = graph.in_degree(node);
                let is_in_entry_original_scc = node == *entry;
                (
                    if is_dominator { 0 } else { 1 },
                    if is_in_entry_original_scc { 0 } else { 1 },
                    in_degree,
                )
            });
            for v in will_visit {
                dfs(topo, graph, entry, result, in_degree, v);
            }
        }

        let mut result = Vec::new();
        let mut in_degree = vec![0; graph.nodes().iter().max().unwrap() + 1];
        for &u in graph.nodes() {
            in_degree[u] = graph.in_degree(u);
        }

        dfs(self, graph, entry, &mut result, &mut in_degree, *entry);

        Ok(result)
    }

    fn reachable_any(
        &self,
        u: &usize,
        target: &HashSet<usize>,
        exclusion: &HashSet<usize>,
    ) -> bool {
        if exclusion.contains(u) {
            return false;
        }
        let graph = &self.graph;
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        queue.push_back(*u);
        visited.insert(*u);
        while let Some(u) = queue.pop_front() {
            for &v in graph.edges(u) {
                if exclusion.contains(&v) {
                    continue;
                }
                if visited.insert(v) {
                    if target.contains(&v) {
                        return true;
                    }
                    queue.push_back(v);
                }
            }
        }
        false
    }

    fn pick_exit(
        &self,
        view_stack: &Vec<&HashSet<usize>>,
        entries_stack: &Vec<usize>,
        possible_exits: &HashSet<usize>,
        parent_reachable_exits: &HashSet<usize>,
    ) -> Result<Option<usize>, anyhow::Error> {
        let mut possible_exits = possible_exits.clone();
        if !parent_reachable_exits.is_empty() {
            let parent_reachable_exits =
                self.prune_unnecessary_exit_nodes(parent_reachable_exits, entries_stack);
            if !parent_reachable_exits.is_empty() {
                possible_exits = parent_reachable_exits;
            }
        }
        if possible_exits.is_empty() {
            return Ok(None);
        }
        for entry in entries_stack.iter().rev() {
            if possible_exits.contains(entry) {
                return Ok(Some(*entry));
            }
        }
        for view in view_stack.iter().rev() {
            let reachable_from_outside = possible_exits
                .iter()
                .filter(|&v| self.graph.reverse_edges(*v).any(|&u| !view.contains(&u)));
            let reachable_from_outside = reachable_from_outside.cloned().collect::<Vec<_>>();
            let max = reachable_from_outside.iter().max();
            if max.is_some() {
                return Ok(max.cloned());
            }
        }
        // prefer the exit that is accessible from entries in reverse order
        for entry in entries_stack.iter().rev() {
            let possible_from_v = self
                .graph
                .edges(*entry)
                .filter(|&v| possible_exits.contains(v));
            let possible_from_v = possible_from_v.cloned().collect::<Vec<_>>();
            let max = possible_from_v.iter().max();
            if max.is_some() {
                return Ok(max.cloned());
            }
        }
        let possible_exits = self.prune_unnecessary_exit_nodes(&possible_exits, entries_stack);
        return Ok(possible_exits.iter().max().cloned());
    }

    fn prune_unnecessary_exit_nodes(
        &self,
        nodes: &HashSet<usize>,
        entries_stack: &Vec<usize>,
    ) -> HashSet<usize> {
        let mut result: Vec<_> = nodes.iter().cloned().collect();
        let entries = entries_stack.iter().cloned().collect::<HashSet<_>>();
        let mut checked = HashSet::new();
        loop {
            let mut to_remove = Vec::new();
            for u in result.iter() {
                if checked.contains(u) {
                    continue;
                }
                let check_set = result
                    .iter()
                    .filter(|&x| x != u)
                    .cloned()
                    .collect::<HashSet<_>>();
                if self.reachable_any(u, &check_set, &entries) {
                    to_remove.push(*u);
                    break;
                } else {
                    checked.insert(*u);
                }
            }
            if to_remove.is_empty() {
                break;
            }
            result.retain(|&x| !to_remove.contains(&x));
        }
        result.into_iter().collect()
    }

    fn recursive_sort(
        &self,
        views_stack: &Vec<&HashSet<usize>>,
        entries_stack: &Vec<usize>,
        remove_entry_back_edges: bool,
    ) -> Result<TopoSortedBlockOrder, anyhow::Error> {
        let view = *views_stack.last().unwrap();

        let current_entry = *entries_stack.last().unwrap();

        let mut result = TopoSortedBlockOrder::new(current_entry);
        if view.len() == 1 {
            if self.graph.edges(current_entry).any(|&x| x == current_entry) {
                // self-loop
                let mut inner = TopoSortedBlockOrder::new(current_entry);
                inner.push_item(current_entry);
                result.push_sub(inner, None);
            } else {
                result.push_item(current_entry);
            }
            return Ok(result);
        }
        let mut subgraph = self.graph.subgraph(view);
        let mut super_graph = SuperGraph::new(
            &self.graph,
            &subgraph,
            &self.dummy_dispatch_blocks,
            &Some(views_stack),
            &Some(view),
            current_entry,
        );
        let original_entry_scc = if super_graph.g.nodes().len() == 1
            || (remove_entry_back_edges
                && super_graph
                    .tarjan
                    .scc_for_node(current_entry)
                    .unwrap()
                    .1
                    .count()
                    > 1)
        {
            let entry_scc: HashSet<_> = super_graph
                .tarjan
                .scc_for_node(current_entry)
                .unwrap()
                .1
                .cloned()
                .collect();
            subgraph.remove_edges_to(current_entry);
            super_graph = SuperGraph::new(
                &self.graph,
                &subgraph,
                &self.dummy_dispatch_blocks,
                &Some(views_stack),
                &Some(&view),
                current_entry,
            );
            entry_scc
        } else {
            HashSet::new()
        };

        let sccs: HashMap<_, _> = super_graph.tarjan.sccs().collect();

        let scc_order = self.topo_sort_super_graph(&super_graph, &original_entry_scc)?;

        let mut included: HashSet<usize> = HashSet::new();

        let parent_reachable_super_nodes =
            self.calculate_parent_reachable_super_nodes(&super_graph, view, current_entry);

        for &scc_idx in scc_order.iter() {
            if included.contains(&scc_idx) {
                continue;
            }
            included.insert(scc_idx);
            let scc = sccs.get(&scc_idx).unwrap();
            if scc.len() == 1 {
                let item = *scc.iter().next().unwrap();
                if !self.graph.edges(item).any(|&x| x == item) {
                    result.push_item(item);
                    continue;
                }
                // it maybe a loop, continue processing
            }
            let entries = super_graph.g_node_2_original_node_entries[&scc_idx].clone();
            if entries.is_empty() {
                continue;
            }
            if entries.len() != 1 {
                return Err(anyhow::anyhow!("SCC has more than one entry"));
            }
            let entry = *entries.iter().next().unwrap();

            {
                // check if this component is not a loop
                let subgraph = subgraph.subgraph(&scc.iter().cloned().collect::<HashSet<_>>());
                if has_no_loop(&subgraph, entry) {
                    let order = self.topo_sort_dag(&subgraph, &entry)?;
                    for &item in order.iter() {
                        result.push_item(item);
                    }
                    continue;
                }
            }

            let mut entries_stack = entries_stack.clone();
            entries_stack.push(entry);

            let mut subview = HashSet::from_iter(scc.iter().cloned());

            let mut exit_nodes = super_graph
                .g_node_2_original_node_exits
                .get(&scc_idx)
                .cloned()
                .unwrap_or_default();

            let mut added_back_exit_nodes = HashSet::new();
            for u in scc.iter() {
                for &v in self.graph.edges(*u) {
                    if !scc.contains(&v) {
                        exit_nodes.insert(v);
                        added_back_exit_nodes.insert(v);
                    }
                }
            }
            if exit_nodes.len() <= 1 {
                let mut views_stack = views_stack.clone();
                views_stack.push(&subview);
                let need_remove_entry_back_edges = subview.is_superset(&view);
                result.push_sub(
                    self.recursive_sort(
                        &views_stack,
                        &entries_stack,
                        need_remove_entry_back_edges,
                    )?,
                    if exit_nodes.is_empty() {
                        None
                    } else {
                        Some(*exit_nodes.iter().next().unwrap())
                    },
                );
                continue;
            }

            let all_exits = exit_nodes;

            let mut subview_graph = subgraph.subgraph(&subview);
            subview_graph.remove_edges_to(entry);
            let subview_super_graph = SuperGraph::new(
                &self.graph,
                &subview_graph,
                &Default::default(),
                &None,
                &Some(&subview),
                entry,
            );
            let mut subview_possible_exits = HashSet::new();
            for subview_scc in subview_super_graph.tarjan.sccs() {
                if subview_scc.1.len() == 1 {
                    let u = *subview_scc.1.iter().next().unwrap();
                    for v in self.graph.edges(u) {
                        if !subview.contains(&v)
                            || v == &entry
                            || added_back_exit_nodes.contains(&v)
                        {
                            subview_possible_exits.insert(*v);
                        }
                    }
                }
            }
            let possible_exits: HashSet<_> = all_exits
                .intersection(&subview_possible_exits)
                .cloned()
                .collect();

            let parent_reachable_exits: HashSet<_> = possible_exits
                .iter()
                .filter(|&&exit| {
                    !view.contains(&exit)
                        || added_back_exit_nodes.contains(&exit)
                        || exit == current_entry
                        || parent_reachable_super_nodes
                            .contains(&super_graph.scc_idx_for_node_unchecked(exit))
                })
                .cloned()
                .collect();

            let mut possible_new_view_stack = views_stack.clone();
            possible_new_view_stack.push(&subview);
            let exit = self.pick_exit(
                &possible_new_view_stack,
                &entries_stack,
                &possible_exits,
                &parent_reachable_exits,
            )?;

            let mut will_merge = HashSet::new();
            let mut ignore_super_nodes = included.clone();
            if let Some(exit) = exit {
                let exit_scc = super_graph.tarjan.scc_for_node(exit);
                if let Some((exit_scc, _)) = exit_scc {
                    ignore_super_nodes.insert(exit_scc);
                }
            }
            let exit_is_dummy_dispatch_block = exit
                .map(|exit| self.dummy_dispatch_blocks.get(&exit))
                .flatten();
            for &non_exit in all_exits.iter().filter(|&&e| Some(e) != exit) {
                if !view.contains(&non_exit) {
                    continue;
                }
                if exit_is_dummy_dispatch_block.is_some()
                    && self.dummy_dispatch_blocks.get(&non_exit) == exit_is_dummy_dispatch_block
                {
                    continue;
                }
                let non_exit_scc = super_graph.scc_idx_for_node_unchecked(non_exit);
                if will_merge.contains(&non_exit_scc) {
                    continue;
                }
                for super_idx in
                    traversal_and_collect(&super_graph.g, non_exit_scc, &ignore_super_nodes)
                {
                    will_merge.insert(super_idx);
                }
            }
            for &super_idx in will_merge.iter() {
                subview.extend(sccs[&super_idx].iter().cloned());
            }

            included.extend(will_merge);

            let still_same_as_parent = subview.is_superset(&view);
            if still_same_as_parent {
                if remove_entry_back_edges {
                    panic!("infinity loop detected");
                }
                return self.recursive_sort(&views_stack, &entries_stack, true);
            } else {
                let mut view_stack = views_stack.clone();
                view_stack.push(&subview);
                result.push_sub(
                    self.recursive_sort(&view_stack, &entries_stack, false)?,
                    exit,
                );
            }
        }

        Ok(result)
    }

    fn solve(&mut self, entry: usize) -> Result<TopoSortedBlockOrder, anyhow::Error> {
        self.create_graph(entry);
        let full_view: HashSet<usize> = (0..self.blocks.len()).collect();
        let full_view_without_dead_entries = self.view_without_dead_entries(&full_view, entry);
        self.create_and_annotate_nodes_info(entry);
        let order =
            self.recursive_sort(&vec![&full_view_without_dead_entries], &vec![entry], false)?;
        Ok(order)
    }

    fn calculate_parent_reachable_super_nodes(
        &self,
        super_graph: &SuperGraph,
        subview: &HashSet<usize>,
        entry: usize,
    ) -> HashSet<usize> {
        let mut parent_reachable = HashSet::new();
        let mut queue = VecDeque::new();

        let sccs = super_graph.tarjan.sccs().collect::<HashMap<_, _>>();

        let mut out_degree = super_graph.g_out_degree.clone();

        for &super_idx in super_graph.g.nodes() {
            if out_degree[super_idx] == 0 {
                let mut can_reach_parent = false;
                for &u in sccs[&super_idx].iter() {
                    for &v in self.graph.edges(u) {
                        if !subview.contains(&v) || v == entry {
                            can_reach_parent = true;
                            break;
                        }
                    }
                    if can_reach_parent {
                        break;
                    }
                }
                if can_reach_parent {
                    parent_reachable.insert(super_idx);
                }
                queue.push_back(super_idx);
            }
        }

        while let Some(super_idx) = queue.pop_front() {
            let can_reach_parent = parent_reachable.contains(&super_idx);
            for &prev_super_idx in super_graph.g.reverse_edges(super_idx) {
                out_degree[prev_super_idx] -= 1;
                if can_reach_parent {
                    parent_reachable.insert(prev_super_idx);
                }
                if out_degree[prev_super_idx] == 0 {
                    queue.push_back(prev_super_idx);
                }
            }
        }

        parent_reachable
    }
}

fn traversal_and_collect(
    g: &Graph,
    starting_node: usize,
    ignore_nodes: &HashSet<usize>,
) -> HashSet<usize> {
    let mut result = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(starting_node);
    while let Some(node) = queue.pop_front() {
        if ignore_nodes.contains(&node) {
            continue;
        }
        result.insert(node);
        for &next_node in g.edges(node) {
            queue.push_back(next_node);
        }
    }
    result
}

pub fn topo_sort<BlockContent: BlockContentTrait>(
    blocks: Vec<WithMetadata<BasicBlock<usize, BlockContent>>>,
) -> Result<TopoSortedBlocks<BlockContent>, anyhow::Error> {
    let mut topo = CFGTopo::new(&blocks);

    let order = topo.solve(0)?;

    Ok(topo.apply_order(&order))
}
