// Copyright (c) Verichains, 2023

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct Graph {
    _empty: HashSet<usize>,
    nodes: HashSet<usize>,
    graph: HashMap<usize, HashSet<usize>>,
}

impl Graph {
    pub fn new() -> Self {
        Self {
            _empty: HashSet::new(),
            nodes: HashSet::new(),
            graph: HashMap::new(),
        }
    }

    pub fn add_edge(&mut self, from: usize, to: usize) {
        self.nodes.insert(from);
        self.nodes.insert(to);
        self.graph.entry(from).or_insert(HashSet::new()).insert(to);
    }

    pub fn nodes(&self) -> &HashSet<usize> {
        &self.nodes
    }

    pub fn edges(&self, node: usize) -> impl Iterator<Item = &usize> {
        self.graph.get(&node).unwrap_or(&self._empty).iter()
    }

    pub fn ensure_node(&mut self, node: usize) {
        self.nodes.insert(node);
    }

    pub fn remove_edges_to(&mut self, entry: usize) {
        for edges in self.graph.values_mut() {
            edges.remove(&entry);
        }
    }
}

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
