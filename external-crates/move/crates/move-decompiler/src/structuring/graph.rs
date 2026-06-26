// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    config::{Config, print_heading},
    structuring::{ast as D, dom_tree},
};

use petgraph::{
    graph::{DiGraph, NodeIndex},
    visit::EdgeRef,
};

use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct Graph {
    pub cfg: DiGraph<(), ()>,
    pub dom_tree: dom_tree::DominatorTree,
    pub loop_heads: HashSet<NodeIndex>,
    pub back_edges: HashMap<NodeIndex, HashSet<NodeIndex>>,
    /// For each non-loop-head node, the succ_nodes of every loop whose body contains it.
    /// `structure_acyclic_region`'s orphan hoist consults this so it doesn't eat an
    /// enclosing-loop successor that `structure_loop` will append after the `Loop` form;
    /// `structure_code_node`'s `next` fusion consults it for the same reason.
    pub loop_exits: HashMap<NodeIndex, HashSet<NodeIndex>>,
}

impl Graph {
    pub fn new(
        config: &Config,
        input: &BTreeMap<NodeIndex, D::Input>,
        start_node: NodeIndex,
    ) -> Self {
        // Create the control flow graph by first adding all nodes, then edges
        let mut cfg = DiGraph::new();

        // Add nodes for each label that exists in the input
        for label in input.keys() {
            // Ensure we have enough nodes in the graph
            while cfg.node_count() <= label.index() {
                cfg.add_node(());
            }
        }

        // Add all edges from the input
        for edge in input.values().flat_map(|value| value.edges()) {
            cfg.add_edge(edge.0, edge.1, ());
        }

        if config.debug_print.control_flow_graph {
            print_heading("control flow graph");
            println!("{cfg:#?}");
        }

        let (loop_heads, back_edges) = find_loop_heads_and_back_edges(&cfg, start_node);
        let dom_tree = dom_tree::DominatorTree::from_graph(&cfg, start_node);
        if config.debug_print.control_flow_graph {
            print_heading("dominators");
            println!("{dom_tree:#?}");
            print_heading("loop heads");
            println!("{loop_heads:#?}");
        }
        let mut graph = Self {
            cfg,
            dom_tree,
            loop_heads,
            back_edges,
            loop_exits: HashMap::new(),
        };
        // Populate `loop_exits` from the loops' bodies after the graph is otherwise built so
        // `find_loop_nodes` has the dom-tree and back-edges available.
        let mut loop_exits: HashMap<NodeIndex, HashSet<NodeIndex>> = HashMap::new();
        for &lh in &graph.loop_heads {
            let (body, succs) = graph.find_loop_nodes(lh);
            for body_node in &body {
                loop_exits
                    .entry(*body_node)
                    .or_default()
                    .extend(succs.iter().copied());
            }
        }
        graph.loop_exits = loop_exits;
        graph
    }

    pub fn update_loop_info(&mut self, loop_head: NodeIndex) {
        for (_, back_edges) in self.back_edges.iter_mut() {
            back_edges.remove(&loop_head);
        }
        for node in self.back_edges.keys().copied().collect::<Vec<_>>() {
            if node == loop_head || self.back_edges[&node].is_empty() {
                self.back_edges.remove(&node);
            }
        }
        self.loop_heads.remove(&loop_head);
    }

    // Do loop node refinement, a la No More Gotos
    pub fn find_loop_nodes(
        &self,
        node_start: NodeIndex,
    ) -> (HashSet<NodeIndex>, HashSet<NodeIndex>) {
        // Loop-body discovery, following the No More Gotos definition: for each back-edge t -> h
        // (where the header h dominates the latch t), the loop body is {h} together with every
        // node that can reach t without going through h. We collect that with one reverse BFS
        // from the latches, treating the header as a frontier - O(V + E) per call.
        //
        // We recompute back-edges from the CFG and dom tree directly: u -> h is a back-edge iff h
        // dominates u. Both the CFG and the dom tree are immutable across structuring, so this is
        // stable. Self-loops (a CFG self-edge h -> h) fall out naturally: the latch list contains
        // h, and the BFS treats h as the frontier on the first pop without expanding the body.

        let dom_descendants: HashSet<NodeIndex> = self
            .dom_tree
            .get(node_start)
            .all_children()
            .chain(std::iter::once(node_start))
            .collect();

        let latches: Vec<NodeIndex> = self
            .cfg
            .neighbors_directed(node_start, petgraph::Direction::Incoming)
            .filter(|pred| dom_descendants.contains(pred))
            .collect();

        let mut loop_nodes: HashSet<NodeIndex> = HashSet::from([node_start]);
        let mut work: Vec<NodeIndex> = latches;
        while let Some(node) = work.pop() {
            if node == node_start || !loop_nodes.insert(node) {
                continue;
            }
            for pred in self
                .cfg
                .neighbors_directed(node, petgraph::Direction::Incoming)
            {
                if !loop_nodes.contains(&pred) {
                    work.push(pred);
                }
            }
        }

        // Iterate `loop_nodes` in sorted order - it's a HashSet so iteration order is
        // otherwise non-deterministic, and that order leaks into `refine_loop_nodes`'s
        // greedy fixpoint, which can produce different SCC-boundary refinements run-to-run.
        let mut loop_nodes_sorted: Vec<NodeIndex> = loop_nodes.iter().copied().collect();
        loop_nodes_sorted.sort_by_key(|n| n.index());
        let mut succ_nodes = HashSet::new();
        for node in &loop_nodes_sorted {
            for successor in self
                .cfg
                .neighbors_directed(*node, petgraph::Direction::Outgoing)
            {
                if !loop_nodes.contains(&successor) {
                    succ_nodes.insert(successor);
                }
            }
        }

        let (loop_nodes, succ_nodes) = self.refine_loop_nodes(loop_nodes, succ_nodes);
        (loop_nodes, succ_nodes)
    }

    fn refine_loop_nodes(
        &self,
        mut loop_nodes: HashSet<NodeIndex>,
        mut succ_nodes: HashSet<NodeIndex>,
    ) -> (HashSet<NodeIndex>, HashSet<NodeIndex>) {
        let mut new_nodes = succ_nodes.clone();

        while succ_nodes.len() > 1 && !new_nodes.is_empty() {
            new_nodes.clear();
            // Sort for determinism: HashSet iteration order leaks into the refinement's
            // greedy frontier expansion.
            let mut sorted_succs: Vec<NodeIndex> = succ_nodes.iter().copied().collect();
            sorted_succs.sort_by_key(|n| n.index());
            for node in sorted_succs {
                if self
                    .cfg
                    .neighbors_directed(node, petgraph::Direction::Incoming)
                    .all(|node| loop_nodes.contains(&node))
                {
                    loop_nodes.insert(node);
                    succ_nodes.remove(&node);
                    // Note: NMG also filters by `dom_nodes.contains(nodes)` here, but we do not:
                    // a legitimate loop break target (a label owned by an outer scope) is not
                    // dominated by the loop header but may be the loop's true successor;
                    // dropping it would leave `succ_nodes` empty, meaning `insert_breaks`
                    // cannot rewrite the break `Jump` and the goto leaks out as residue.
                    let nodes = self
                        .cfg
                        .neighbors_directed(node, petgraph::Direction::Outgoing)
                        .filter(|node| !loop_nodes.contains(node));
                    new_nodes.extend(nodes);
                }
            }
            succ_nodes.extend(new_nodes.iter().cloned());
        }
        (loop_nodes, succ_nodes)
    }
}

fn find_loop_heads_and_back_edges<N, E>(
    graph: &DiGraph<N, E>,
    start: NodeIndex,
) -> (HashSet<NodeIndex>, HashMap<NodeIndex, HashSet<NodeIndex>>) {
    pub fn find_recur<N, E>(
        graph: &DiGraph<N, E>,
        visited: &mut HashSet<NodeIndex>,
        path_to_root: &mut Vec<NodeIndex>,
        loop_heads: &mut HashSet<NodeIndex>,
        back_edges: &mut HashMap<NodeIndex, HashSet<NodeIndex>>,
        node: NodeIndex,
    ) {
        if !visited.insert(node) {
            return;
        };

        path_to_root.push(node);
        for edge in graph.edges_directed(node, petgraph::Direction::Outgoing) {
            let target = edge.target();
            if path_to_root
                .iter()
                .any(|ndx| *ndx != node && *ndx == target)
                || target == node
            {
                loop_heads.insert(target);
                back_edges.entry(node).or_default().insert(target);
            }
            find_recur(graph, visited, path_to_root, loop_heads, back_edges, target);
        }
        assert!(node == path_to_root.pop().expect("No seen node to pop"));
    }

    let mut loop_heads = HashSet::new();
    let mut back_edges = HashMap::new();

    find_recur(
        graph,
        &mut HashSet::new(),
        &mut vec![],
        &mut loop_heads,
        &mut back_edges,
        start,
    );

    (loop_heads, back_edges)
}
