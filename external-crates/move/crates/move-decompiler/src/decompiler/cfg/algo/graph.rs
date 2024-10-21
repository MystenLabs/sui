// Copyright (c) Verichains, 2023

use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone)]
pub struct Graph {
    _empty: Vec<usize>,
    nodes: HashSet<usize>,
    graph: HashMap<usize, Vec<usize>>,
    rev_graph: HashMap<usize, Vec<usize>>,
}

fn add_if_not_present(graph: &mut HashMap<usize, Vec<usize>>, fr: usize, to: usize) {
    let v = graph.entry(fr).or_insert(Vec::new());
    if !v.contains(&to) {
        v.push(to);
    }
}

impl Graph {
    pub fn new() -> Self {
        Self {
            _empty: Default::default(),
            nodes: Default::default(),
            graph: Default::default(),
            rev_graph: Default::default(),
        }
    }

    pub fn add_edge(&mut self, from: usize, to: usize) {
        self.nodes.insert(from);
        self.nodes.insert(to);
        add_if_not_present(&mut self.graph, from, to);
        add_if_not_present(&mut self.rev_graph, to, from);
    }

    #[allow(dead_code)]
    pub fn undirected(&self) -> Graph {
        let mut result = self.clone();
        for u in self.nodes() {
            for v in self.edges(*u) {
                result.add_edge(*v, *u);
            }
        }
        result
    }

    pub fn nodes(&self) -> &HashSet<usize> {
        &self.nodes
    }

    pub fn out_degree(&self, node: usize) -> usize {
        self.graph.get(&node).unwrap_or(&self._empty).len()
    }

    pub fn in_degree(&self, node: usize) -> usize {
        self.rev_graph.get(&node).unwrap_or(&self._empty).len()
    }

    pub fn edges(&self, node: usize) -> impl Iterator<Item = &usize> {
        self.graph.get(&node).unwrap_or(&self._empty).iter()
    }

    pub fn reverse_edges(&self, node: usize) -> impl Iterator<Item = &usize> {
        self.rev_graph.get(&node).unwrap_or(&self._empty).iter()
    }

    pub fn ensure_node(&mut self, node: usize) {
        self.nodes.insert(node);
    }

    pub fn remove_edges_to(&mut self, entry: usize) {
        for edges in self.graph.values_mut() {
            edges.retain(|&e| e != entry);
        }
        self.rev_graph.remove(&entry);
    }

    pub(crate) fn subgraph(&self, view: &HashSet<usize>) -> Self {
        let mut result = Self::new();
        for u in self.nodes().iter().filter(|n| view.contains(n)) {
            result.ensure_node(*u);
            for v in self.edges(*u).filter(|n| view.contains(n)) {
                result.add_edge(*u, *v);
            }
        }
        result
    }
}

#[derive(Debug, Clone)]
pub struct TarjanScc {
    index: usize,
    stack: Vec<usize>,
    scc: HashMap<usize, usize>,
    sccs: Vec<Vec<usize>>,
    indices: HashMap<usize, usize>,
    lowlinks: HashMap<usize, usize>,
    in_stack: HashSet<usize>,
}

impl TarjanScc {
    pub fn new(graph: &Graph) -> Self {
        let mut tarjan = Self {
            index: 0,
            stack: Vec::new(),
            scc: HashMap::new(),
            sccs: Vec::new(),
            indices: HashMap::new(),
            lowlinks: HashMap::new(),
            in_stack: HashSet::new(),
        };

        for u in graph.nodes() {
            if !tarjan.indices.contains_key(u) {
                tarjan.strong_connect(&graph, *u);
            }
        }

        tarjan.sccs.sort_by(|a, b| a[0].cmp(&b[0]));
        for (idx, scc) in tarjan.sccs.iter().enumerate() {
            for node in scc {
                tarjan.scc.insert(*node, idx);
            }
        }

        tarjan
    }

    pub fn sccs(&self) -> impl Iterator<Item = (usize, &Vec<usize>)> {
        self.sccs.iter().enumerate()
    }

    pub fn scc_for_node(&self, node: usize) -> Option<(usize, impl Iterator<Item = &usize>)> {
        if let Some(&scc_idx) = self.scc.get(&node) {
            Some((scc_idx, self.sccs[scc_idx].iter()))
        } else {
            None
        }
    }

    fn strong_connect(&mut self, graph: &Graph, u: usize) {
        self.indices.insert(u, self.index);
        self.lowlinks.insert(u, self.index);
        self.index += 1;
        self.stack.push(u);
        self.in_stack.insert(u);

        for v in graph.edges(u) {
            if !self.indices.contains_key(v) {
                self.strong_connect(graph, *v);
                let lowlink = std::cmp::min(self.lowlinks[&u], self.lowlinks[v]);
                self.lowlinks.insert(u, lowlink);
            } else if self.in_stack.contains(v) {
                let lowlink = std::cmp::min(self.lowlinks[&u], self.indices[v]);
                self.lowlinks.insert(u, lowlink);
            }
        }

        if self.lowlinks[&u] == self.indices[&u] {
            let mut scc = Vec::new();
            loop {
                let n = self.stack.pop().unwrap();
                self.in_stack.remove(&n);
                scc.push(n);
                if n == u {
                    break;
                }
            }
            scc.sort();
            self.sccs.push(scc);
        }
    }
}

pub struct StrongBridges {}

impl StrongBridges {
    pub fn for_undirected_graph(graph: &Graph, root: usize) -> HashSet<(usize, usize)> {
        let mut bridges = HashSet::new();
        let mut visited = HashSet::new();
        let mut lowlinks = HashMap::new();
        let mut indices = HashMap::new();
        let mut index = 0;

        Self::strong_bridges(
            graph,
            root,
            None,
            &mut index,
            &mut indices,
            &mut lowlinks,
            &mut visited,
            &mut bridges,
        );

        bridges
    }

    fn strong_bridges(
        graph: &Graph,
        u: usize,
        parent: Option<usize>,
        index: &mut usize,
        indices: &mut HashMap<usize, usize>,
        lowlinks: &mut HashMap<usize, usize>,
        visited: &mut HashSet<usize>,
        bridges: &mut HashSet<(usize, usize)>,
    ) {
        visited.insert(u);
        indices.insert(u, *index);
        lowlinks.insert(u, *index);
        *index += 1;
        for v in graph.edges(u) {
            if Some(*v) == parent {
                continue;
            }
            if !visited.contains(v) {
                Self::strong_bridges(
                    graph,
                    *v,
                    Some(u),
                    index,
                    indices,
                    lowlinks,
                    visited,
                    bridges,
                );
                let lowlink = std::cmp::min(lowlinks[&u], lowlinks[v]);
                lowlinks.insert(u, lowlink);
                if lowlinks[v] > indices[&u] {
                    bridges.insert((u, *v));
                }
            } else {
                let lowlink = std::cmp::min(lowlinks[&u], indices[v]);
                lowlinks.insert(u, lowlink);
            }
        }
    }
}

pub struct DominatorNodes {}

/// nodes which all paths to its children must pass through itself
impl DominatorNodes {
    pub(crate) fn for_graph(graph: &Graph, root: usize) -> HashSet<usize> {
        let mut to_remove = HashSet::new();
        let mut in_degree = vec![0; graph.nodes().len()];
        let mut queue = VecDeque::new();

        for &u in graph.nodes() {
            in_degree[u] = graph.in_degree(u);
            if u == root {
                continue;
            }
            if in_degree[u] == 0 {
                to_remove.insert(u);
                queue.push_back(u);
            }
        }

        while let Some(u) = queue.pop_front() {
            for v in graph.edges(u) {
                in_degree[*v] -= 1;
                if in_degree[*v] == 0 {
                    to_remove.insert(*v);
                    queue.push_back(*v);
                }
            }
        }

        let mut split_graph = Graph::new();
        fn in_node(node: usize) -> usize {
            node * 2
        }
        fn out_node(node: usize) -> usize {
            node * 2 + 1
        }
        for &u in graph.nodes() {
            if to_remove.contains(&u) {
                continue;
            }
            split_graph.add_edge(in_node(u), out_node(u));
            for &v in graph.edges(u) {
                split_graph.add_edge(out_node(u), in_node(v));
            }
        }
        let bridges = StrongBridges::for_undirected_graph(&split_graph.undirected(), root);
        let mut result = HashSet::new();

        for (u, v) in bridges {
            let ou = u / 2;
            let ov = v / 2;
            if ou == ov {
                result.insert(ou);
            }
        }

        result.remove(&root);

        result
    }
}

pub fn has_no_loop(graph: &Graph, root: usize) -> bool {
    let mut visited = HashSet::new();
    let mut stack = Vec::new();
    stack.push(root);
    while let Some(u) = stack.pop() {
        if visited.contains(&u) {
            return false;
        }
        visited.insert(u);
        for v in graph.edges(u) {
            stack.push(*v);
        }
    }
    true
}
