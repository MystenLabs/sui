// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    config::{Config, print_heading},
    structuring::{ast as D, dom_tree},
};

use petgraph::{
    algo::dominators::Dominators,
    graph::{DiGraph, NodeIndex},
    visit::EdgeRef,
};

use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug)]
pub struct Graph {
    pub cfg: DiGraph<(), ()>,
    pub return_: NodeIndex,
    pub dom_tree: dom_tree::DominatorTree,
    pub loop_heads: HashSet<NodeIndex>,
    pub back_edges: HashMap<NodeIndex, HashSet<NodeIndex>>,
    pub post_dominators: Dominators<NodeIndex>,
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
        let (return_, post_dominators) = compute_post_dominators(config, &cfg, input);
        if config.debug_print.control_flow_graph {
            print_heading("dominators");
            println!("{dom_tree:#?}");
            print_heading("post-dominators");
            println!("{post_dominators:#?}");
            print_heading("loop heads");
            println!("{loop_heads:#?}");
        }
        Self {
            cfg,
            dom_tree,
            loop_heads,
            back_edges,
            post_dominators,
            return_,
        }
    }

    pub fn update_latch_nodes(&mut self, node: NodeIndex, latch: NodeIndex) {
        self.update_latch_branch_nodes(node, vec![latch]);
    }

    pub fn update_latch_branch_nodes(&mut self, node: NodeIndex, latches: Vec<NodeIndex>) {
        let latches = latches
            .iter()
            .filter_map(|latch| self.back_edges.remove(latch))
            .flatten()
            .collect::<HashSet<NodeIndex>>();
        if !latches.is_empty() {
            let result = self.back_edges.insert(node, latches);
            assert!(result.is_none());
        }
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
        // from the latches, treating the header as a frontier — O(V + E) per call.
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

        let mut succ_nodes = HashSet::new();
        for node in &loop_nodes {
            for successor in self
                .cfg
                .neighbors_directed(*node, petgraph::Direction::Outgoing)
            {
                if !loop_nodes.contains(&successor) {
                    succ_nodes.insert(successor);
                }
            }
        }

        let (loop_nodes, succ_nodes) = self.refine_loop_nodes(loop_nodes, succ_nodes, node_start);
        (loop_nodes, succ_nodes)
    }

    fn refine_loop_nodes(
        &self,
        mut loop_nodes: HashSet<NodeIndex>,
        mut succ_nodes: HashSet<NodeIndex>,
        loop_header: NodeIndex,
    ) -> (HashSet<NodeIndex>, HashSet<NodeIndex>) {
        let mut new_nodes = succ_nodes.clone();
        let dom_nodes = self
            .dom_tree
            .get(loop_header)
            .all_children()
            .collect::<HashSet<_>>();

        while succ_nodes.len() > 1 && !new_nodes.is_empty() {
            new_nodes.clear();
            for node in succ_nodes.clone() {
                if self
                    .cfg
                    .neighbors_directed(node, petgraph::Direction::Incoming)
                    .all(|node| loop_nodes.contains(&node))
                {
                    loop_nodes.insert(node);
                    succ_nodes.remove(&node);
                    let nodes = self
                        .cfg
                        .neighbors_directed(node, petgraph::Direction::Outgoing)
                        .filter(|node| !loop_nodes.contains(node) && dom_nodes.contains(node));
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

fn compute_post_dominators<N, E>(
    config: &Config,
    cfg: &petgraph::Graph<N, E>,
    input: &BTreeMap<D::Label, D::Input>,
) -> (NodeIndex, Dominators<NodeIndex>) {
    // Build a reversed copy of the CFG and add a synthetic `return_` node. Dominators rooted
    // at `return_` in this graph are post-dominators of the original.
    let mut rev = petgraph::graph::DiGraph::<(), ()>::from_edges(
        cfg.edge_references().map(|e| (e.target(), e.source())),
    );
    let return_: NodeIndex = rev.add_node(());
    if config.debug_print.control_flow_graph {
        println!("Return node: {return_:?}");
    }

    // Wire `return_` to each original sink (a node with no outgoing edges in the original CFG,
    // which is a node with no incoming edges in the reversed graph).
    for node in rev.node_indices() {
        if node == return_ || !input.contains_key(&node) {
            continue;
        }
        if rev
            .neighbors_directed(node, petgraph::Direction::Incoming)
            .count()
            == 0
        {
            if !matches!(input.get(&node), Some(D::Input::Code(_, _, None)))
                && config.debug_print.control_flow_graph
            {
                println!("Node {node:?} with no outs");
            }
            rev.add_edge(return_, node, ());
        }
    }

    add_infinite_loop_post_dominators(config, &mut rev, return_, cfg, input);

    (
        return_,
        petgraph::algo::dominators::simple_fast(&rev, return_),
    )
}

/// Add *impossible edges* (the folk-literature name for synthetic edges introduced to make
/// post-domination total) from `return_` to one representative of each terminal region of the
/// original CFG — an infinite loop or abort-cycle that never reaches a real exit. Without
/// them, `simple_fast` leaves `immediate_dominator` undefined for nodes in those regions and
/// structuring later crashes on `.unwrap()`.
///
/// We identify terminal regions as terminal SCCs of the original CFG: a strongly-connected
/// component is *terminal* when every outgoing edge from any of its nodes stays inside the
/// SCC, so execution that enters never leaves. (The singleton-sink case slips through this
/// check vacuously and gets a redundant impossible edge — already wired up by the sinks pass
/// — but `simple_fast` handles the duplicate fine.) For each terminal SCC we pick the
/// highest-indexed input node as the representative; any node in the SCC would suffice to
/// make the whole SCC post-dominator-defined.
fn add_infinite_loop_post_dominators<N, E>(
    config: &Config,
    rev: &mut DiGraph<(), ()>,
    return_: NodeIndex,
    cfg: &petgraph::Graph<N, E>,
    input: &BTreeMap<D::Label, D::Input>,
) {
    for scc in petgraph::algo::tarjan_scc(cfg) {
        let members: HashSet<NodeIndex> = scc.iter().copied().collect();
        let stays_in_scc = scc
            .iter()
            .flat_map(|&n| cfg.neighbors(n))
            .all(|m| members.contains(&m));
        if !stays_in_scc {
            continue;
        }
        let Some(rep) = scc.iter().filter(|n| input.contains_key(n)).max().copied() else {
            continue;
        };
        if config.debug_print.control_flow_graph {
            println!("Adding impossible edge return_ -> {rep:?}");
        }
        rev.add_edge(return_, rep, ());
    }
}
