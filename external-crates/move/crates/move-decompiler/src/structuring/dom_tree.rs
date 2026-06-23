// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use petgraph::{
    algo::dominators::simple_fast,
    graph::{DiGraph, NodeIndex},
};

use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone)]
pub struct DominatorTree {
    tree: Node,
    /// `idoms[child] = parent` for every non-root reachable node. Lets us walk *upward* in
    /// the dom tree (e.g. for NCD); `Node` carries only the downward direction.
    idoms: HashMap<NodeIndex, NodeIndex>,
}

#[derive(Debug, Clone)]
pub struct Node {
    value: NodeIndex,
    children: Vec<Node>,
}

impl DominatorTree {
    pub fn from_graph<N, E>(graph: &DiGraph<N, E>, root: NodeIndex) -> DominatorTree {
        fn build_node(
            value: NodeIndex,
            child_map: &mut HashMap<NodeIndex, Vec<NodeIndex>>,
        ) -> Node {
            let children = child_map.remove(&value).unwrap_or_default();
            let children = children
                .into_iter()
                .map(|child| build_node(child, child_map))
                .collect::<Vec<_>>();
            Node { value, children }
        }

        let dominators = simple_fast(&graph, root);
        let mut child_map: HashMap<NodeIndex, Vec<NodeIndex>> = HashMap::new();
        let mut idoms: HashMap<NodeIndex, NodeIndex> = HashMap::new();
        let all_nodes: HashSet<NodeIndex> = graph.node_indices().collect();

        for &node in &all_nodes {
            if let Some(parent) = dominators.immediate_dominator(node) {
                // Skip the root
                if parent != node {
                    child_map.entry(parent).or_default().push(node);
                    idoms.insert(node, parent);
                }
            }
        }

        let tree = build_node(root, &mut child_map);
        DominatorTree { tree, idoms }
    }

    pub fn get(&self, target: NodeIndex) -> &'_ Node {
        let mut queue = VecDeque::from([&self.tree]);

        while let Some(node) = queue.pop_front() {
            if node.value == target {
                return node;
            };

            node.children
                .iter()
                .for_each(|child| queue.push_back(child));
        }

        panic!("Not found")
    }

    /// Immediate dominator of `n`, or `None` if `n` is the root or unreachable.
    pub fn idom(&self, n: NodeIndex) -> Option<NodeIndex> {
        self.idoms.get(&n).copied()
    }

    /// Nearest common dominator of `nodes`: the lowest node in the dom tree that dominates
    /// every input. Returns `None` for an empty input; a single-element input returns that
    /// node.
    pub fn nearest_common_dominator(
        &self,
        nodes: impl IntoIterator<Item = NodeIndex>,
    ) -> Option<NodeIndex> {
        let mut iter = nodes.into_iter();
        let mut acc = iter.next()?;
        for n in iter {
            acc = self.ncd_pair(acc, n);
        }
        Some(acc)
    }

    fn ncd_pair(&self, a: NodeIndex, b: NodeIndex) -> NodeIndex {
        // Collect `a`'s ancestor chain (including `a`); walk `b`'s chain and return the first
        // ancestor that appears in `a`'s set.
        let mut a_chain: HashSet<NodeIndex> = HashSet::new();
        let mut cur = Some(a);
        while let Some(n) = cur {
            a_chain.insert(n);
            cur = self.idoms.get(&n).copied();
        }
        let mut cur = Some(b);
        while let Some(n) = cur {
            if a_chain.contains(&n) {
                return n;
            }
            cur = self.idoms.get(&n).copied();
        }
        self.tree.value
    }
}

impl Node {
    pub fn immediate_children(&self) -> impl Iterator<Item = NodeIndex> {
        self.children.iter().map(|child| child.value)
    }

    pub fn all_children(&self) -> Box<dyn Iterator<Item = NodeIndex> + '_> {
        let iter = self.children.iter().flat_map(|child| {
            let self_iter = std::iter::once(child.value);
            let child_iter = child.all_children();
            self_iter.chain(child_iter)
        });

        Box::new(iter)
    }
}
