// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Call graph construction and analysis for move-model-2.
//!
//! This module provides a call graph representation built from a [`Model`], supporting:
//! - Iteration over function call relationships
//! - Strongly connected component (SCC) detection for recursive functions
//! - Topological ordering for bottom-up analysis
//!
//! # Example
//! ```ignore
//! use move_model_2::{Model, call_graph::CallGraph};
//!
//! let model: Model<_> = /* ... */;
//! let call_graph = CallGraph::from_model(&model);
//!
//! // Iterate over functions in topological order
//! for item in call_graph.topological_order() {
//!     match item {
//!         TopologicalItem::Single(func_id) => { /* non-recursive */ }
//!         TopologicalItem::Recursive(scc) => { /* mutually recursive */ }
//!     }
//! }
//! ```

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use petgraph::{
    graph::{DiGraph, NodeIndex},
    visit::EdgeRef,
    Direction,
};

use crate::{model::Model, normalized::QualifiedMemberId, source_kind::SourceKind};

/// A function identifier in the call graph.
///
/// This is a tuple of (ModuleId, Symbol) identifying a specific function.
pub type FunctionId = QualifiedMemberId;

/// The call graph for a set of Move modules.
///
/// Provides efficient iteration over call relationships and supports
/// SCC detection and topological ordering.
pub struct CallGraph {
    graph: DiGraph<FunctionId, ()>,
    node_map: BTreeMap<FunctionId, NodeIndex>,
}

/// A strongly connected component in the call graph.
///
/// An SCC with more than one function, or a single function that calls itself,
/// represents mutual recursion.
#[derive(Debug, Clone)]
pub struct SCC {
    /// The functions in this SCC, in the order they were discovered.
    pub functions: Vec<FunctionId>,
    /// Whether this SCC contains actual recursion (cycles).
    /// False for trivial SCCs containing a single non-recursive function.
    pub is_cyclic: bool,
}

/// An item in topological order traversal.
#[derive(Debug, Clone)]
pub enum TopologicalItem {
    /// A non-recursive function.
    Single(FunctionId),
    /// A set of mutually recursive functions.
    Recursive(SCC),
}

impl CallGraph {
    /// Construct a call graph from a move-model-2 Model.
    ///
    /// This iterates over all modules and functions in the model,
    /// building a directed graph where edges represent function calls.
    pub fn from_model<K: SourceKind>(model: &Model<K>) -> Self {
        let mut graph = DiGraph::new();
        let mut node_map = BTreeMap::new();

        // First pass: add all function nodes
        for module in model.modules() {
            let module_id = module.id();
            for function in module.functions() {
                let func_id = (module_id, function.name());
                let node_idx = graph.add_node(func_id);
                node_map.insert(func_id, node_idx);
            }
        }

        // Second pass: add call edges
        for module in model.modules() {
            let module_id = module.id();
            for function in module.functions() {
                let caller_id = (module_id, function.name());
                let caller_idx = node_map[&caller_id];

                for callee_id in function.calls() {
                    // Only add edges for callees that exist in our graph
                    // (callees in external packages won't be present)
                    if let Some(&callee_idx) = node_map.get(callee_id) {
                        graph.add_edge(caller_idx, callee_idx, ());
                    }
                }
            }
        }

        Self { graph, node_map }
    }

    /// Returns the number of functions in the call graph.
    pub fn function_count(&self) -> usize {
        self.node_map.len()
    }

    /// Returns the number of call edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Returns true if the given function is in the call graph.
    pub fn contains(&self, func: &FunctionId) -> bool {
        self.node_map.contains_key(func)
    }

    /// Iterate over all functions in the call graph.
    pub fn functions(&self) -> impl Iterator<Item = &FunctionId> {
        self.node_map.keys()
    }

    /// Iterate over the callees of a function (functions it calls).
    ///
    /// Returns `None` if the function is not in the graph.
    pub fn callees(&self, func: &FunctionId) -> Option<impl Iterator<Item = FunctionId> + '_> {
        let idx = self.node_map.get(func)?;
        Some(
            self.graph
                .neighbors_directed(*idx, Direction::Outgoing)
                .map(|idx| self.graph[idx]),
        )
    }

    /// Iterate over the callers of a function (functions that call it).
    ///
    /// Returns `None` if the function is not in the graph.
    pub fn callers(&self, func: &FunctionId) -> Option<impl Iterator<Item = FunctionId> + '_> {
        let idx = self.node_map.get(func)?;
        Some(
            self.graph
                .neighbors_directed(*idx, Direction::Incoming)
                .map(|idx| self.graph[idx]),
        )
    }

    /// Iterate over strongly connected components in reverse topological order.
    ///
    /// SCCs are returned from leaves (functions that don't call others) to roots
    /// (entry points). This ordering is useful for bottom-up analysis where
    /// you need to process callees before callers.
    pub fn sccs(&self) -> impl Iterator<Item = SCC> + '_ {
        petgraph::algo::tarjan_scc(&self.graph)
            .into_iter()
            .map(|nodes| {
                let functions: Vec<_> = nodes.iter().map(|&idx| self.graph[idx]).collect();

                // An SCC is cyclic if it has more than one node, or if the single
                // node has a self-edge
                let is_cyclic = if nodes.len() > 1 {
                    true
                } else {
                    let idx = nodes[0];
                    self.graph
                        .edges_directed(idx, Direction::Outgoing)
                        .any(|e| e.target() == idx)
                };

                SCC {
                    functions,
                    is_cyclic,
                }
            })
    }

    /// Iterate over functions in topological order (bottom-up from leaves).
    ///
    /// Returns [`TopologicalItem::Single`] for non-recursive functions,
    /// and [`TopologicalItem::Recursive`] for groups of mutually recursive functions.
    ///
    /// This ordering processes callees before callers, which is useful for
    /// analyses that need to propagate information upward through the call graph.
    pub fn topological_order(&self) -> impl Iterator<Item = TopologicalItem> + '_ {
        self.sccs().map(|scc| {
            if scc.is_cyclic {
                TopologicalItem::Recursive(scc)
            } else {
                debug_assert_eq!(scc.functions.len(), 1);
                TopologicalItem::Single(scc.functions.into_iter().next().unwrap())
            }
        })
    }

    /// Check if a function is recursive (calls itself directly or indirectly).
    ///
    /// Returns `false` if the function is not in the graph.
    pub fn is_recursive(&self, func: &FunctionId) -> bool {
        let Some(&idx) = self.node_map.get(func) else {
            return false;
        };

        // Check for direct self-recursion first (fast path)
        if self
            .graph
            .edges_directed(idx, Direction::Outgoing)
            .any(|e| e.target() == idx)
        {
            return true;
        }

        // Check for indirect recursion via reachability
        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::new();

        // Start from direct callees
        for neighbor in self.graph.neighbors_directed(idx, Direction::Outgoing) {
            queue.push_back(neighbor);
        }

        while let Some(current) = queue.pop_front() {
            if current == idx {
                return true;
            }
            if visited.insert(current) {
                for neighbor in self.graph.neighbors_directed(current, Direction::Outgoing) {
                    queue.push_back(neighbor);
                }
            }
        }

        false
    }

    /// Iterate over transitive callees (all functions reachable from func).
    ///
    /// Uses breadth-first traversal. Does not include the starting function
    /// unless it is recursive.
    pub fn transitive_callees(
        &self,
        func: &FunctionId,
    ) -> impl Iterator<Item = FunctionId> + use<'_> {
        TransitiveIter::new(&self.graph, self.node_map.get(func).copied(), Direction::Outgoing)
    }

    /// Iterate over transitive callers (all functions that can reach func).
    ///
    /// Uses breadth-first traversal. Does not include the starting function
    /// unless it is recursive.
    pub fn transitive_callers(
        &self,
        func: &FunctionId,
    ) -> impl Iterator<Item = FunctionId> + use<'_> {
        TransitiveIter::new(&self.graph, self.node_map.get(func).copied(), Direction::Incoming)
    }
}

/// Iterator for transitive traversal (BFS).
struct TransitiveIter<'a> {
    graph: &'a DiGraph<FunctionId, ()>,
    visited: BTreeSet<NodeIndex>,
    queue: VecDeque<NodeIndex>,
    direction: Direction,
}

impl<'a> TransitiveIter<'a> {
    fn new(
        graph: &'a DiGraph<FunctionId, ()>,
        start: Option<NodeIndex>,
        direction: Direction,
    ) -> Self {
        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::new();

        // Initialize with immediate neighbors
        if let Some(idx) = start {
            visited.insert(idx);
            for neighbor in graph.neighbors_directed(idx, direction) {
                if visited.insert(neighbor) {
                    queue.push_back(neighbor);
                }
            }
        }

        Self {
            graph,
            visited,
            queue,
            direction,
        }
    }
}

impl Iterator for TransitiveIter<'_> {
    type Item = FunctionId;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.queue.pop_front()?;
        let func_id = self.graph[current];

        // Add unvisited neighbors to queue
        for neighbor in self.graph.neighbors_directed(current, self.direction) {
            if self.visited.insert(neighbor) {
                self.queue.push_back(neighbor);
            }
        }

        Some(func_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a simple test graph manually
    fn create_test_graph(
        nodes: &[FunctionId],
        edges: &[(FunctionId, FunctionId)],
    ) -> CallGraph {
        let mut graph = DiGraph::new();
        let mut node_map = BTreeMap::new();

        for &node in nodes {
            let idx = graph.add_node(node);
            node_map.insert(node, idx);
        }

        for (from, to) in edges {
            if let (Some(&from_idx), Some(&to_idx)) = (node_map.get(from), node_map.get(to)) {
                graph.add_edge(from_idx, to_idx, ());
            }
        }

        CallGraph { graph, node_map }
    }

    fn make_func_id(module: &str, func: &str) -> FunctionId {
        use crate::normalized::ModuleId;
        use move_core_types::account_address::AccountAddress;
        use move_symbol_pool::Symbol;

        let module_id = ModuleId {
            address: AccountAddress::ZERO,
            name: Symbol::from(module),
        };
        (module_id, Symbol::from(func))
    }

    #[test]
    fn test_empty_graph() {
        let graph = create_test_graph(&[], &[]);
        assert_eq!(graph.function_count(), 0);
        assert_eq!(graph.edge_count(), 0);
        assert_eq!(graph.functions().count(), 0);
    }

    #[test]
    fn test_single_function() {
        let f = make_func_id("test", "foo");
        let graph = create_test_graph(&[f], &[]);

        assert_eq!(graph.function_count(), 1);
        assert!(graph.contains(&f));
        assert!(!graph.is_recursive(&f));
        assert_eq!(graph.callees(&f).unwrap().count(), 0);
        assert_eq!(graph.callers(&f).unwrap().count(), 0);
    }

    #[test]
    fn test_direct_recursion() {
        let f = make_func_id("test", "recursive");
        let graph = create_test_graph(&[f], &[(f, f)]);

        assert!(graph.is_recursive(&f));

        let sccs: Vec<_> = graph.sccs().collect();
        assert_eq!(sccs.len(), 1);
        assert!(sccs[0].is_cyclic);
    }

    #[test]
    fn test_mutual_recursion() {
        let a = make_func_id("test", "a");
        let b = make_func_id("test", "b");
        let graph = create_test_graph(&[a, b], &[(a, b), (b, a)]);

        assert!(graph.is_recursive(&a));
        assert!(graph.is_recursive(&b));

        let sccs: Vec<_> = graph.sccs().collect();
        assert_eq!(sccs.len(), 1);
        assert!(sccs[0].is_cyclic);
        assert_eq!(sccs[0].functions.len(), 2);
    }

    #[test]
    fn test_linear_chain() {
        // a -> b -> c
        let a = make_func_id("test", "a");
        let b = make_func_id("test", "b");
        let c = make_func_id("test", "c");
        let graph = create_test_graph(&[a, b, c], &[(a, b), (b, c)]);

        assert!(!graph.is_recursive(&a));
        assert!(!graph.is_recursive(&b));
        assert!(!graph.is_recursive(&c));

        // Topological order should be: c, b, a (leaves first)
        let topo: Vec<_> = graph.topological_order().collect();
        assert_eq!(topo.len(), 3);

        // All should be single (non-recursive)
        for item in &topo {
            assert!(matches!(item, TopologicalItem::Single(_)));
        }

        // Verify order: c before b before a
        let order: Vec<_> = topo
            .iter()
            .map(|item| match item {
                TopologicalItem::Single(f) => *f,
                TopologicalItem::Recursive(_) => panic!("unexpected recursive"),
            })
            .collect();

        let c_pos = order.iter().position(|&f| f == c).unwrap();
        let b_pos = order.iter().position(|&f| f == b).unwrap();
        let a_pos = order.iter().position(|&f| f == a).unwrap();

        assert!(c_pos < b_pos, "c should come before b");
        assert!(b_pos < a_pos, "b should come before a");
    }

    #[test]
    fn test_transitive_callees() {
        // a -> b -> c
        //      |-> d
        let a = make_func_id("test", "a");
        let b = make_func_id("test", "b");
        let c = make_func_id("test", "c");
        let d = make_func_id("test", "d");
        let graph = create_test_graph(&[a, b, c, d], &[(a, b), (b, c), (b, d)]);

        let callees: BTreeSet<_> = graph.transitive_callees(&a).collect();
        assert_eq!(callees.len(), 3);
        assert!(callees.contains(&b));
        assert!(callees.contains(&c));
        assert!(callees.contains(&d));
    }

    #[test]
    fn test_transitive_callers() {
        // a -> c
        // b -> c
        let a = make_func_id("test", "a");
        let b = make_func_id("test", "b");
        let c = make_func_id("test", "c");
        let graph = create_test_graph(&[a, b, c], &[(a, c), (b, c)]);

        let callers: BTreeSet<_> = graph.transitive_callers(&c).collect();
        assert_eq!(callers.len(), 2);
        assert!(callers.contains(&a));
        assert!(callers.contains(&b));
    }

    #[test]
    fn test_callees_callers() {
        // a -> b
        // a -> c
        // d -> b
        let a = make_func_id("test", "a");
        let b = make_func_id("test", "b");
        let c = make_func_id("test", "c");
        let d = make_func_id("test", "d");
        let graph = create_test_graph(&[a, b, c, d], &[(a, b), (a, c), (d, b)]);

        let a_callees: BTreeSet<_> = graph.callees(&a).unwrap().collect();
        assert_eq!(a_callees.len(), 2);
        assert!(a_callees.contains(&b));
        assert!(a_callees.contains(&c));

        let b_callers: BTreeSet<_> = graph.callers(&b).unwrap().collect();
        assert_eq!(b_callers.len(), 2);
        assert!(b_callers.contains(&a));
        assert!(b_callers.contains(&d));
    }
}
