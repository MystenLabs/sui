// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use petgraph::{
    algo::dominators::simple_fast,
    graph::{DiGraph, NodeIndex},
};

use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug)]
pub struct DominatorTree(Node);

#[derive(Debug)]
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
        let all_nodes: HashSet<NodeIndex> = graph.node_indices().collect();

        for &node in &all_nodes {
            if let Some(idom) = dominators.immediate_dominator(node) {
                // Skip the root
                if idom != node {
                    child_map.entry(idom).or_default().push(node);
                }
            }
        }

        // Build tree recursively from root
        let tree = build_node(root, &mut child_map);
        DominatorTree(tree)
    }

    pub fn get(&self, target: NodeIndex) -> &'_ Node {
        let mut queue = VecDeque::from([&self.0]);

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
