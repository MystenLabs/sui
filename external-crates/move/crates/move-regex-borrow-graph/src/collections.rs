// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::graph_map::{GraphMap, NodeIndex};
use crate::{
    MeterResult, Result, bail, ensure, error,
    meter::Meter,
    references::{Edge, Node, Ref},
    regex::{Extension, Regex},
};
use core::fmt;
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

//**************************************************************************************************
// Definitions
//**************************************************************************************************

#[derive(Clone, Debug)]
/// Returned from the public query APIs of `borrowed_by` and `borrows_from`.
// Note this is not to be used internally and is not
pub struct Path<Loc, Lbl> {
    pub loc: Loc,
    pub labels: Vec<Lbl>,
    pub ends_in_dot_star: bool,
}

pub type Paths<Loc, Lbl> = Vec<Path<Loc, Lbl>>;

#[derive(Clone, Debug)]
pub struct Graph<Loc, Lbl: Ord> {
    canonical_reference_capacity: usize,
    fresh_id: u32,
    nodes: BTreeMap<Ref, Node>,
    pub(crate) graph: GraphMap<Ref, Edge<Loc, Lbl>>,
}

//**************************************************************************************************
// impls
//**************************************************************************************************

impl<Loc, Lbl> Path<Loc, Lbl> {
    /// An empty path
    pub fn is_epsilon(&self) -> bool {
        self.labels.is_empty() && !self.is_dot_star()
    }

    /// A path with a single label (and not dot-star)
    pub fn is_label(&self, lbl: &Lbl) -> bool
    where
        Lbl: Eq,
    {
        !self.is_dot_star() && self.labels.len() == 1 && &self.labels[0] == lbl
    }

    /// A path that starts with the specified label
    pub fn starts_with(&self, lbl: &Lbl) -> bool
    where
        Lbl: Eq,
    {
        self.is_dot_star() || self.labels.first().is_some_and(|l| l == lbl)
    }

    /// A path with no labels and ends with dot-star
    pub fn is_dot_star(&self) -> bool {
        self.labels.is_empty() && self.ends_in_dot_star
    }

    pub fn abstract_size(&self) -> usize {
        1 + self.labels.len() + (self.ends_in_dot_star as usize)
    }
}

impl<Loc: Copy, Lbl: Ord + Clone + fmt::Display> Graph<Loc, Lbl> {
    pub fn new<K: fmt::Debug + Ord>(
        canonical_reference_capacity: usize,
        initial_refs: impl IntoIterator<Item = (K, Loc, /* is_mut */ bool)>,
    ) -> Result<(Self, BTreeMap<K, Ref>)> {
        let mut map = BTreeMap::new();
        let mut graph = Self {
            canonical_reference_capacity,
            fresh_id: 0,
            nodes: BTreeMap::new(),
            graph: GraphMap::new(canonical_reference_capacity),
        };
        for (k, loc, is_mut) in initial_refs {
            let (r, _r_idx) = graph.add_ref(loc, is_mut)?;
            ensure!(!map.contains_key(&k), "key {:?} already exists", k);
            map.insert(k, r);
        }
        debug_assert_eq!(graph.nodes.len(), graph.graph.node_count());
        graph.check_invariants();
        Ok((graph, map))
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn is_mutable(&self, r: Ref) -> Result<bool> {
        self.node(&r).map(|n| n.is_mutable())
    }

    pub(crate) fn node(&self, r: &Ref) -> Result<&Node> {
        self.nodes
            .get(r)
            .ok_or_else(|| error!("missing ref {:?}", r))
    }

    /// Returns the direct successors of the specified reference
    pub(crate) fn successors(
        &self,
        r: Ref,
    ) -> Result<impl Iterator<Item = (&Edge<Loc, Lbl>, Ref)> + '_> {
        let r_idx = self.node(&r)?.node_index();
        Ok(self.successors_idx(r_idx)?.map(move |(e, s_idx)| {
            let s = *self.graph.node_weight(s_idx).unwrap();
            (e, s)
        }))
    }

    /// Returns the direct successors of the specified reference by NodeIndex
    fn successors_idx(
        &self,
        r: NodeIndex,
    ) -> Result<impl Iterator<Item = (&Edge<Loc, Lbl>, NodeIndex)> + '_> {
        ensure!(self.graph.contains_node(r), "missing ref {:?} in graph", r);
        Ok(self.graph.outgoing_edges(r))
    }

    /// Returns the direct predecessors of the specified reference
    fn predecessors(&self, r: Ref) -> Result<impl Iterator<Item = (Ref, &Edge<Loc, Lbl>)> + '_> {
        let r_idx = self.node(&r)?.node_index();
        Ok(self.predecessors_idx(r_idx)?.map(move |(p_idx, e)| {
            let p = *self.graph.node_weight(p_idx).unwrap();
            (p, e)
        }))
    }

    /// Returns the direct predecessors of the specified reference by NodeIndex
    fn predecessors_idx(
        &self,
        r: NodeIndex,
    ) -> Result<impl Iterator<Item = (NodeIndex, &Edge<Loc, Lbl>)> + '_> {
        ensure!(self.graph.contains_node(r), "missing ref {:?} in graph", r);
        Ok(self.graph.incoming_edges(r))
    }

    fn add_ref(&mut self, loc: Loc, is_mut: bool) -> Result<(Ref, NodeIndex)> {
        self.check_invariants();
        let id = self.fresh_id;
        self.fresh_id += 1;
        let r = Ref::fresh(id);

        let r_idx = self.graph.add_node(r);
        let mut edge = Edge::<Loc, Lbl>::new();
        edge.insert(loc, Cow::Owned(Regex::epsilon()));
        self.graph.add_edge(r_idx, edge, r_idx);

        let node = Node::new(is_mut, r_idx);
        let prev = self.nodes.insert(r, node);
        ensure!(prev.is_none(), "ref {:?} already exists", r);
        self.check_invariants();
        Ok((r, r_idx))
    }

    pub fn extend_by_epsilon<M: Meter>(
        &mut self,
        loc: Loc,
        sources: impl IntoIterator<Item = Ref>,
        is_mut: bool,
        meter: &mut M,
    ) -> MeterResult<Ref, M::Error> {
        let (new_ref, new_ref_idx) = self.add_ref(loc, is_mut)?;
        let source_idxs = sources
            .into_iter()
            .map(|r| self.node(&r).map(|n| n.node_index()))
            .collect::<Result<BTreeSet<_>>>()?;
        let ext = Extension::Epsilon;
        let edges_to_add = {
            let mut acc = BTreeMap::new();
            self.determine_all_new_edges(&mut acc, &source_idxs, ext, &[new_ref_idx], meter)?;
            acc
        };
        self.add_new_edges(loc, &BTreeSet::from([new_ref_idx]), edges_to_add)?;
        Ok(new_ref)
    }

    /// Creates a new reference whose paths are an extension of all specified sources.
    /// If sources is empty, the reference will have a single path rooted at the specified label
    pub fn extend_by_label<M: Meter>(
        &mut self,
        loc: Loc,
        sources: impl IntoIterator<Item = Ref>,
        is_mut: bool,
        extension: Lbl,
        meter: &mut M,
    ) -> MeterResult<Ref, M::Error> {
        let (new_ref, new_ref_idx) = self.add_ref(loc, is_mut)?;
        let source_idxs = sources
            .into_iter()
            .map(|r| self.node(&r).map(|n| n.node_index()))
            .collect::<Result<BTreeSet<_>>>()?;
        let ext = Extension::Label(extension);
        let edges_to_add = {
            let mut acc = BTreeMap::new();
            self.determine_all_new_edges(&mut acc, &source_idxs, ext, &[new_ref_idx], meter)?;
            acc
        };
        self.add_new_edges(loc, &BTreeSet::from([new_ref_idx]), edges_to_add)?;
        Ok(new_ref)
    }

    /// Creates new references based on the mutability specified. Immutable references will extend
    /// from all sources and mutable references will extends only from mutable sources.
    /// Additionally, all mutable references will be disjoint from all other references created
    pub fn extend_by_dot_star_for_call<M: Meter>(
        &mut self,
        loc: Loc,
        all_sources: &BTreeSet<Ref>,
        mutabilities: Vec<bool>,
        meter: &mut M,
    ) -> MeterResult<Vec<Ref>, M::Error> {
        let all_source_idxs = all_sources
            .iter()
            .map(|r| self.node(r).map(|n| n.node_index()))
            .collect::<Result<BTreeSet<_>>>()?;
        let mut_source_idxs = {
            let mut s = BTreeSet::new();
            for r in all_sources.iter().copied() {
                if self.is_mutable(r)? {
                    let node = self.node(&r)?;
                    let r_idx = node.node_index();
                    s.insert(r_idx);
                }
            }
            s
        };

        let mut mut_new_refs = vec![];
        let mut imm_new_refs = vec![];
        let new_refs = mutabilities
            .into_iter()
            .map(|is_mut| {
                let (new_ref, new_ref_idx) = self.add_ref(loc, is_mut)?;
                if is_mut {
                    mut_new_refs.push(new_ref_idx);
                } else {
                    imm_new_refs.push(new_ref_idx);
                }
                Ok(new_ref)
            })
            .collect::<Result<Vec<_>>>()?;
        let edges_to_add = {
            let mut acc = BTreeMap::new();
            // determine mut edges
            self.determine_all_new_edges(
                &mut acc,
                &mut_source_idxs,
                Extension::DotStar,
                &mut_new_refs,
                meter,
            )?;
            // determine imm edges
            self.determine_all_new_edges(
                &mut acc,
                &all_source_idxs,
                Extension::DotStar,
                &imm_new_refs,
                meter,
            )?;
            // edges between imm refs
            for &x in &imm_new_refs {
                for &y in &imm_new_refs {
                    if x == y {
                        continue;
                    }
                    let prev = acc.insert((x, y), vec![Regex::dot_star()]);
                    ensure!(
                        prev.is_none(),
                        "new imm refs should not yet have edges between them"
                    );
                }
            }
            acc
        };
        let all_new_refs = mut_new_refs.iter().chain(&imm_new_refs).copied().collect();
        self.add_new_edges(loc, &all_new_refs, edges_to_add)?;

        #[cfg(debug_assertions)]
        {
            let mut all_new_refs = BTreeSet::new();
            for new_ref in mut_new_refs.iter().chain(&imm_new_refs).copied() {
                let was_new = all_new_refs.insert(new_ref);
                debug_assert!(was_new, "duplicate new ref in extend_by_dot_star_for_call");
            }
            for &mut_new_ref in &mut_new_refs {
                for (_, s) in self
                    .successors_idx(mut_new_ref)
                    .unwrap()
                    .filter(|&(_, s)| s != mut_new_ref)
                {
                    debug_assert!(!imm_new_refs.contains(&s));
                    debug_assert!(!mut_new_refs.contains(&s));
                }
            }
            for &imm_new_ref in &imm_new_refs {
                for (_, s) in self
                    .successors_idx(imm_new_ref)
                    .unwrap()
                    .filter(|&(_, s)| s != imm_new_ref)
                {
                    // s is new ==> s is imm
                    debug_assert!(!all_new_refs.contains(&s) || imm_new_refs.contains(&s));
                    debug_assert!(!mut_new_refs.contains(&s));
                }
            }
        }
        Ok(new_refs)
    }

    /// For each source, and for each other node x (including the case where x == source),
    /// consider all edges source --> x and x --> source,
    /// for each new reference, determine the edge new --> x and x --> new based on the extension
    /// provided
    fn determine_all_new_edges<M: Meter>(
        &mut self,
        acc: &mut BTreeMap<(NodeIndex, NodeIndex), Vec<Regex<Lbl>>>,
        sources: &BTreeSet<NodeIndex>,
        ext: Extension<Lbl>,
        new_refs: &[NodeIndex],
        meter: &mut M,
    ) -> MeterResult<(), M::Error> {
        self.check_invariants();
        let mut nodes_visited = 0usize;
        let mut total_edge_size = 0usize;
        let mut edge_to_add = |p: NodeIndex, r: Regex<Lbl>, s: NodeIndex| {
            total_edge_size = total_edge_size.saturating_add(r.abstract_size());
            acc.entry((p, s)).or_default().push(r);
        };
        // look for all edges of the form source --> x or x --> source
        for source in sources {
            nodes_visited = nodes_visited.saturating_add(1);
            // x --> source
            for (x, edge) in self.graph.incoming_edges(*source) {
                nodes_visited = nodes_visited.saturating_add(1);
                for x_to_source in edge.regexes() {
                    let extended = x_to_source.clone().extend(&ext);
                    for &new_ref in new_refs {
                        edge_to_add(x, extended.clone(), new_ref)
                    }
                }
            }
            // source --> x
            for (edge, x) in self.graph.outgoing_edges(*source) {
                nodes_visited = nodes_visited.saturating_add(1);
                for source_to_x in edge.regexes() {
                    // For the edge source --> x, we adding a new edge source --> new_ref
                    // In cases of a label extension, we might need to add an edge new_ref --> x
                    // if the extension is a prefix of source_to_x.
                    // However! In cases where an epsilon or dot-star is involved,
                    // we might also have the case that we can remove source --> x as a prefix
                    // of source --> new_ref
                    // In the case where we have `e.remove_prefix(p)` and `e` is a list of labels
                    // `fgh` and `p` is `.*`, we will consider all possible suffixes of `e`,
                    // `[fgh, gh, h, epsilon]`. This could grow rather quickly, so we might
                    // want to optimize this representation
                    for source_to_x_suffix in source_to_x.remove_prefix(&ext) {
                        for &new_ref in new_refs {
                            edge_to_add(new_ref, source_to_x_suffix.clone(), x)
                        }
                    }
                    for regex_suffix in ext.remove_prefix(source_to_x) {
                        for &new_ref in new_refs {
                            edge_to_add(x, regex_suffix.clone(), new_ref);
                        }
                    }
                }
            }
        }
        meter.visit_nodes(nodes_visited)?;
        meter.visit_edges(total_edge_size)?;
        Ok(())
    }

    // Insert new edges p --> s
    // Returns an error if p or s are not in new_refs
    // this should not be called directly, and the various extend_by_* functions
    // should be used instead
    fn add_new_edges(
        &mut self,
        loc: Loc,
        new_refs: &BTreeSet<NodeIndex>,
        edges_to_add: BTreeMap<(NodeIndex, NodeIndex), Vec<Regex<Lbl>>>,
    ) -> Result<()> {
        for ((p, s), rs) in edges_to_add {
            ensure!(
                new_refs.contains(&p) || new_refs.contains(&s),
                "should only add edges to or from the new ref"
            );
            self.add_edge(p, loc, rs, s)?;
        }
        self.check_invariants();
        Ok(())
    }

    // adds a single edge to the graph
    // this should not be called directly, and the various extend_by_* functions
    // should be used instead
    fn add_edge(
        &mut self,
        source: NodeIndex,
        loc: Loc,
        regexes: Vec<Regex<Lbl>>,
        target: NodeIndex,
    ) -> Result<()> {
        ensure!(!regexes.is_empty(), "no regexes to add edge");
        if source == target {
            let non_epsilon = regexes.iter().find(|regex| !regex.is_epsilon());
            ensure!(
                non_epsilon.is_none(),
                "self edge must be epsilon {:?} --{}--> {:?}",
                source,
                non_epsilon.unwrap(),
                target
            );
            self.check_self_epsilon_invariant(source);
            return Ok(());
        }

        debug_assert!(!self.graph.contains_edge(source, target));
        let mut edge = Edge::<Loc, Lbl>::new();
        for r in regexes {
            edge.insert(loc, Cow::Owned(r));
        }
        self.graph.add_edge(source, edge, target);
        Ok(())
    }

    //**********************************************************************************************
    // Ref API
    //**********************************************************************************************

    pub fn release<M: Meter>(&mut self, r: Ref, meter: &mut M) -> MeterResult<(), M::Error> {
        self.check_invariants();
        meter.visit_nodes(self.nodes.len())?;
        let Some(node) = self.nodes.remove(&r) else {
            bail!("missing ref {:?}", r);
        };
        ensure!(
            self.graph.contains_node(node.node_index()),
            "missing ref {:?} in graph",
            r
        );
        self.graph.remove_node(node.node_index());
        self.check_invariants();
        Ok(())
    }

    pub fn release_all(&mut self) {
        self.nodes.clear();
        self.graph.clear();
        self.fresh_id = 0
    }

    //**********************************************************************************************
    // Query API
    //**********************************************************************************************

    /// Returns the references that extend the specified reference and the path(s) for the extension
    pub fn borrowed_by<M: Meter>(
        &self,
        r: Ref,
        meter: &mut M,
    ) -> MeterResult<BTreeMap<Ref, Paths<Loc, Lbl>>, M::Error> {
        let mut paths = BTreeMap::new();
        let mut nodes_visited = 0usize;
        let mut total_edge_size = 0usize;
        for (edge, s) in self.successors(r)? {
            nodes_visited = nodes_visited.saturating_add(1);
            if r == s {
                // skip self epsilon
                continue;
            }
            total_edge_size = total_edge_size.saturating_add(edge.abstract_size());
            let _prev = paths.insert(s, edge.paths());
            debug_assert!(_prev.is_none());
        }
        meter.visit_nodes(nodes_visited)?;
        meter.visit_edges(total_edge_size)?;
        Ok(paths)
    }

    /// Returns the references that are extended by the specified reference and the path(s) for the
    /// extension
    pub fn borrows_from<M: Meter>(
        &self,
        r: Ref,
        meter: &mut M,
    ) -> MeterResult<BTreeMap<Ref, Paths<Loc, Lbl>>, M::Error> {
        let mut paths = BTreeMap::new();
        let mut nodes_visited = 0usize;
        let mut total_edge_size = 0usize;
        for (p, edge) in self.predecessors(r)? {
            nodes_visited = nodes_visited.saturating_add(1);
            if r == p {
                // skip self epsilon
                continue;
            }
            total_edge_size = total_edge_size.saturating_add(edge.abstract_size());
            let _prev = paths.insert(p, edge.paths());
            debug_assert!(_prev.is_none());
        }
        meter.visit_nodes(nodes_visited)?;
        meter.visit_edges(total_edge_size)?;
        Ok(paths)
    }

    //**********************************************************************************************
    // Joining
    //**********************************************************************************************

    /// Returns true if self changed
    pub fn join<M: Meter>(&mut self, other: &Self, meter: &mut M) -> MeterResult<bool, M::Error> {
        self.check_join_invariants(other);
        let mut total_edge_size_increase = 0usize;
        let self_keys = self.keys().collect::<BTreeSet<_>>();
        let other_all_edges = other
            .graph
            .all_edges()
            .map(|(p_other_idx, e, s_other_idx)| {
                let p = *other.graph.node_weight(p_other_idx).unwrap();
                let s = *other.graph.node_weight(s_other_idx).unwrap();
                (p, e, s)
            });
        for (p, other_edge, s) in
            other_all_edges.filter(|(p, _, s)| self_keys.contains(p) && self_keys.contains(s))
        {
            let p_self_idx = self.node(&p)?.node_index();
            let s_self_idx = self.node(&s)?.node_index();
            let self_edge_size_increase =
                if let Some(self_edge) = self.graph.edge_weight_mut(p_self_idx, s_self_idx) {
                    self_edge.join(other_edge)
                } else {
                    let edge = other_edge.clone();
                    let size = edge.abstract_size();
                    debug_assert!(size > 0);
                    self.graph.add_edge(p_self_idx, edge, s_self_idx);
                    size
                };
            total_edge_size_increase =
                total_edge_size_increase.saturating_add(self_edge_size_increase);
        }
        meter.visit_nodes(self.nodes.len())?;
        meter.visit_edges(total_edge_size_increase)?;
        self.check_invariants();
        Ok(total_edge_size_increase > 0)
    }

    /// Refresh all references (making them no longer canonical)
    pub fn refresh_refs(&mut self) -> Result<()> {
        let nodes = std::mem::take(&mut self.nodes);
        self.fresh_id = 0;
        self.nodes = nodes
            .into_iter()
            .map(|(r, node)| {
                let Some(node_weight_mut) = self.graph.node_weight_mut(node.node_index()) else {
                    bail!("missing ref {:?} in graph", r);
                };
                debug_assert_eq!(r, *node_weight_mut);
                let r_fresh = r.refresh()?;
                *node_weight_mut = r_fresh;
                let Some(r_fresh_succ) = r_fresh.fresh_id()?.checked_add(1) else {
                    bail!("fresh id overflow");
                };
                self.fresh_id = std::cmp::max(self.fresh_id, r_fresh_succ);
                Ok((r_fresh, node))
            })
            .collect::<Result<_>>()?;
        debug_assert!(self.is_fresh());
        self.check_invariants();
        Ok(())
    }

    /// Canonicalize all references according to the remapping. This allows graphs to have the same
    /// set of references before being joined.
    pub fn canonicalize(&mut self, remapping: &BTreeMap<Ref, u32>) -> Result<()> {
        let nodes = std::mem::take(&mut self.nodes);
        self.nodes = nodes
            .into_iter()
            .map(|(r, node)| {
                let Some(node_weight_mut) = self.graph.node_weight_mut(node.node_index()) else {
                    bail!("missing ref {:?} in graph", r);
                };
                debug_assert_eq!(r, *node_weight_mut);
                let r_canon = r.canonicalize(remapping)?;
                *node_weight_mut = r_canon;
                Ok((r_canon, node))
            })
            .collect::<Result<_>>()?;
        self.fresh_id = 0;
        self.graph.minimize();
        debug_assert!(self.is_canonical());
        debug_assert!(
            self.graph.node_count() <= self.canonical_reference_capacity,
            "exceeded canonical reference capacity"
        );
        debug_assert_eq!(self.nodes.len(), self.graph.node_count());
        self.check_invariants();
        Ok(())
    }

    /// Are all references canonical?
    pub fn is_canonical(&self) -> bool {
        self.nodes.keys().all(|r| r.is_canonical())
    }

    /// Are all references fresh?
    pub fn is_fresh(&self) -> bool {
        self.nodes.keys().all(|r| r.is_fresh())
    }

    //**********************************************************************************************
    // Invariants
    //**********************************************************************************************

    // checks:
    // - both graphs satisfy their invariants
    // - all nodes are canonical
    // - all nodes in self are also in other
    fn check_join_invariants(&self, other: &Self) {
        #[cfg(debug_assertions)]
        {
            self.check_invariants();
            other.check_invariants();
            for self_r in self.keys() {
                debug_assert!(self_r.is_canonical());
                debug_assert!(other.nodes.contains_key(&self_r));
            }
            for other_r in other.keys() {
                debug_assert!(other_r.is_canonical());
                // there can be nodes in other that are not in self
            }
            for (self_r, self_node) in &self.nodes {
                let other_node = other.node(self_r).unwrap();
                debug_assert_eq!(self_node.is_mutable(), other_node.is_mutable());
            }
        }
    }

    // checks:
    // - all nodes are canonical or all nodes are fresh
    // - all nodes are present in map and graph
    // - all nodes have a self epsilon
    // - the abstract size is correct
    pub fn check_invariants(&self) {
        #[cfg(debug_assertions)]
        {
            self.graph.check_invariants();
            let mut is_canonical_opt = None;
            let mut node_indices = BTreeSet::new();
            for (&r, node) in &self.nodes {
                match is_canonical_opt {
                    None => is_canonical_opt = Some(r.is_canonical()),
                    Some(is_canonical) => debug_assert_eq!(is_canonical, r.is_canonical()),
                }
                let is_new = node_indices.insert(node.node_index());
                debug_assert!(is_new, "duplicate node index");
            }
            for (p_idx, e, s_idx) in self.graph.all_edges() {
                let p = *self.graph.node_weight(p_idx).unwrap();
                let s = *self.graph.node_weight(s_idx).unwrap();
                debug_assert!(self.nodes.contains_key(&p));
                debug_assert!(self.nodes.contains_key(&s));
                e.check_invariants();
            }
            for node in self.nodes.values() {
                self.check_self_epsilon_invariant(node.node_index());
            }
        }
    }

    pub fn check_self_epsilon_invariant(&self, r: NodeIndex) {
        #[cfg(debug_assertions)]
        {
            let edge_opt = self.graph.edge_weight(r, r);
            debug_assert!(edge_opt.is_some());
            let rs = edge_opt.unwrap().regexes().collect::<Vec<_>>();
            debug_assert_eq!(rs.len(), 1);
            debug_assert!(rs[0].is_epsilon());
        }
    }

    //**********************************************************************************************
    // Util
    //**********************************************************************************************

    pub fn keys(&self) -> impl Iterator<Item = Ref> + '_ {
        self.nodes.keys().copied()
    }

    #[allow(dead_code)]
    pub fn print(&self)
    where
        Lbl: std::fmt::Display,
    {
        println!("{self}");
    }
}

impl<Loc: Copy, Lbl: Ord + Clone + fmt::Display> fmt::Display for Graph<Loc, Lbl>
where
    Lbl: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct DisplaySuccessors<'a, Loc, Lbl: Ord>(&'a Graph<Loc, Lbl>, Ref);

        impl<Loc: Copy, Lbl: Ord + Clone + fmt::Display> fmt::Display for DisplaySuccessors<'_, Loc, Lbl>
        where
            Lbl: fmt::Display,
        {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let graph = self.0;
                let r = self.1;
                let successors = match graph.successors(r) {
                    Ok(s) => s,
                    Err(e) => return write!(f, "ERROR {r} {:?}", e),
                };
                for (edge, s) in successors {
                    writeln!(f, "\n    {}: {{", s)?;
                    for regex in edge.regexes() {
                        writeln!(f, "        {},", regex)?;
                    }
                    write!(f, "}},")?;
                }
                writeln!(f)?;
                Ok(())
            }
        }

        for (&r, node) in &self.nodes {
            let is_mut = if node.is_mutable() { "mut " } else { "" };
            writeln!(f, "{is_mut}{r}: {{{}}}", DisplaySuccessors(self, r))?;
        }
        Ok(())
    }
}
