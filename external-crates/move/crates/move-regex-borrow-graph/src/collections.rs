// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    Result, bail, ensure, error,
    references::{Edge, Node, Ref},
    regex::{Extension, Regex},
};
use core::fmt;
use petgraph::graphmap::DiGraphMap;
use std::collections::{BTreeMap, BTreeSet};

//**************************************************************************************************
// Definitions
//**************************************************************************************************

#[derive(Clone, Debug)]
/// Returned from the public query APIs of `borrowed_by` and `borrows_from`.
// Note this is not to be used internally and is not
pub struct Path<Loc, Lbl> {
    pub loc: Loc,
    pub(crate) labels: Vec<Lbl>,
    pub(crate) ends_in_dot_star: bool,
}

pub type Paths<Loc, Lbl> = Vec<Path<Loc, Lbl>>;

#[derive(Clone, Debug)]
pub struct Graph<Loc, Lbl: Ord> {
    fresh_id: usize,
    abstract_size: usize,
    nodes: BTreeMap<Ref, Node>,
    graph: DiGraphMap<Ref, Edge<Loc, Lbl>>,
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
        initial_refs: impl IntoIterator<Item = (K, Loc, /* is_mut */ bool)>,
    ) -> Result<(Self, BTreeMap<K, Ref>)> {
        let mut map = BTreeMap::new();
        let mut graph = Self {
            fresh_id: 0,
            abstract_size: 0,
            nodes: BTreeMap::new(),
            graph: DiGraphMap::new(),
        };
        for (k, loc, is_mut) in initial_refs {
            let r = graph.add_ref(loc, is_mut)?;
            ensure!(!map.contains_key(&k), "key {:?} already exists", k);
            map.insert(k, r);
        }
        graph.check_invariants();
        Ok((graph, map))
    }

    pub fn is_mutable(&self, r: Ref) -> Result<bool> {
        self.node(&r).map(|n| n.is_mutable())
    }

    fn node(&self, r: &Ref) -> Result<&Node> {
        self.nodes
            .get(r)
            .ok_or_else(|| error!("missing ref {:?}", r))
    }

    fn node_mut(&mut self, r: &Ref) -> Result<&mut Node> {
        self.nodes
            .get_mut(r)
            .ok_or_else(|| error!("missing ref {:?}", r))
    }

    /// Returns the direct successors of the specified reference
    fn successors(&self, r: Ref) -> Result<impl Iterator<Item = (&Edge<Loc, Lbl>, Ref)> + '_> {
        ensure!(self.graph.contains_node(r), "missing ref {:?} in graph", r);
        Ok(self
            .graph
            .edges_directed(r, petgraph::Direction::Outgoing)
            .map(move |(r_, s, e)| {
                debug_assert_eq!(r, r_);
                (e, s)
            }))
    }

    /// Returns the direct predecessors of the specified reference
    fn predecessors(&self, r: Ref) -> Result<impl Iterator<Item = (Ref, &Edge<Loc, Lbl>)> + '_> {
        ensure!(self.graph.contains_node(r), "missing ref {:?} in graph", r);
        Ok(self
            .graph
            .edges_directed(r, petgraph::Direction::Incoming)
            .map(move |(p, r_, e)| {
                debug_assert_eq!(r, r_);
                (p, e)
            }))
    }

    fn add_ref(&mut self, loc: Loc, is_mut: bool) -> Result<Ref> {
        self.check_invariants();
        let id = self.fresh_id;
        self.fresh_id += 1;
        let r = Ref::fresh(id);

        ensure!(!self.graph.contains_node(r), "ref {:?} already exists", r);
        let mut edge = Edge::<Loc, Lbl>::new();
        let size_increase = edge.insert(loc, Regex::epsilon());
        self.graph.add_edge(r, r, edge);

        let mut node = Node::new(is_mut);
        node.abstract_size = node.abstract_size.saturating_add(size_increase);
        let node_size = node.abstract_size;
        let prev = self.nodes.insert(r, node);
        ensure!(prev.is_none(), "ref {:?} already exists", r);
        self.abstract_size = self.abstract_size.saturating_add(node_size);
        self.check_invariants();
        Ok(r)
    }

    pub fn extend_by_epsilon(
        &mut self,
        loc: Loc,
        sources: impl IntoIterator<Item = Ref>,
        is_mut: bool,
    ) -> Result<Ref> {
        let new_ref = self.add_ref(loc, is_mut)?;
        let ext = Extension::Epsilon;
        self.extend_by_extension(loc, sources, ext, new_ref, &BTreeSet::new())
    }

    /// Creates a new reference whose paths are an extension of all specified sources.
    /// If sources is empty, the reference will have a single path rooted at the specified label
    pub fn extend_by_label(
        &mut self,
        loc: Loc,
        sources: impl IntoIterator<Item = Ref>,
        is_mut: bool,
        extension: Lbl,
    ) -> Result<Ref> {
        let new_ref = self.add_ref(loc, is_mut)?;
        let ext = Extension::Label(extension);
        self.extend_by_extension(loc, sources, ext, new_ref, &BTreeSet::new())
    }

    /// Creates new references based on the mutability specified. Immutable references will extend
    /// from all sources and mutable references will extends only from mutable sources.
    /// Additionally, all mutable references will be disjoint from all other references created
    pub fn extend_by_dot_star_for_call(
        &mut self,
        loc: Loc,
        all_sources: impl IntoIterator<Item = Ref>,
        mutabilities: Vec<bool>,
    ) -> Result<Vec<Ref>> {
        let all_sources = all_sources.into_iter().collect::<BTreeSet<_>>();
        let mut_sources = all_sources
            .iter()
            .copied()
            .filter_map(|r| match self.is_mutable(r) {
                Err(e) => Some(Err(e)),
                Ok(true) => Some(Ok(r)),
                Ok(false) => None,
            })
            .collect::<Result<BTreeSet<_>>>()?;
        let new_refs = mutabilities
            .iter()
            .map(|is_mut| self.add_ref(loc, *is_mut))
            .collect::<Result<Vec<_>>>()?;
        let all_new_refs = new_refs.iter().copied().collect::<BTreeSet<_>>();
        let mut mut_new_refs = BTreeSet::new();
        let mut imm_new_refs = BTreeSet::new();
        for &new_ref in &new_refs {
            if self.is_mutable(new_ref).unwrap() {
                mut_new_refs.insert(new_ref);
            } else {
                imm_new_refs.insert(new_ref);
            }
        }
        for mut_new_ref in &mut_new_refs {
            self.extend_by_extension(
                loc,
                mut_sources.iter().copied(),
                Extension::DotStar,
                *mut_new_ref,
                &all_new_refs,
            )?;
        }
        for imm_new_ref in &imm_new_refs {
            self.extend_by_extension(
                loc,
                all_sources.iter().copied(),
                Extension::DotStar,
                *imm_new_ref,
                &mut_new_refs,
            )?;
        }
        #[cfg(debug_assertions)]
        {
            for &mut_new_ref in &mut_new_refs {
                for (_, s) in self
                    .successors(mut_new_ref)
                    .unwrap()
                    .filter(|&(_, s)| s != mut_new_ref)
                {
                    debug_assert!(!imm_new_refs.contains(&s));
                    debug_assert!(!mut_new_refs.contains(&s));
                }
            }
            for &imm_new_ref in &imm_new_refs {
                for (_, s) in self
                    .successors(imm_new_ref)
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

    fn extend_by_extension(
        &mut self,
        loc: Loc,
        sources: impl IntoIterator<Item = Ref>,
        ext: Extension<Lbl>,
        new_ref: Ref,
        exclude: &BTreeSet<Ref>,
    ) -> Result<Ref> {
        self.check_invariants();
        let mut edges_to_add = vec![];
        for x in sources {
            debug_assert!(!exclude.contains(&x));
            self.determine_new_edges(&mut edges_to_add, x, &ext, new_ref, exclude)?;
        }
        for (p, r, s) in edges_to_add {
            debug_assert!(p == new_ref || s == new_ref);
            self.add_edge(p, loc, r, s)?;
        }
        self.check_invariants();
        Ok(new_ref)
    }

    fn determine_new_edges(
        &self,
        edges_to_add: &mut Vec<(Ref, Regex<Lbl>, Ref)>,
        x: Ref,
        ext: &Extension<Lbl>,
        new_ref: Ref,
        exclude: &BTreeSet<Ref>,
    ) -> Result<()> {
        for (y, edge) in self.predecessors(x)?.filter(|(y, _)| !exclude.contains(y)) {
            for y_to_x in edge.regexes() {
                edges_to_add.push((y, y_to_x.clone().extend(ext), new_ref))
            }
        }
        for (edge, y) in self.successors(x)?.filter(|(_, y)| !exclude.contains(y)) {
            for x_to_y in edge.regexes() {
                // For the edge x --> y, we adding a new edge x --> new_ref
                // In cases of a label extension, we might need to add an edge new_ref --> y
                // if the extension is a prefix of x_to_y.
                // However! In cases where an epsilon or dot-star is involved,
                // we might also have the case that we can remove x --> y as a prefix of
                // x --> new_ref
                // In the case where we have `e.remove_prefix(p)` and `e` is a list of labels
                // `fgh` and `p` is `.*`, we will consider all possible suffixes of `e`,
                // `[fgh, gh, h, epsilon]`. This could grow rather quickly, so we might
                // want to optimize this representation
                for x_to_y_suffix in x_to_y.remove_prefix(ext) {
                    edges_to_add.push((new_ref, x_to_y_suffix, y))
                }
                for regex_suffix in ext.remove_prefix(x_to_y) {
                    edges_to_add.push((y, regex_suffix, new_ref));
                }
            }
        }
        Ok(())
    }

    fn add_edge(&mut self, source: Ref, loc: Loc, regex: Regex<Lbl>, target: Ref) -> Result<()> {
        if source == target {
            ensure!(
                regex.is_epsilon(),
                "self edge must be epsilon {:?} --{}--> {:?}",
                source,
                regex,
                target
            );
            self.check_self_epsilon_invariant(source);
            return Ok(());
        }
        if !self.graph.contains_edge(source, target) {
            self.graph.add_edge(source, target, Edge::new());
        }
        let edge_mut = self.graph.edge_weight_mut(source, target).unwrap();
        let size_increase = edge_mut.insert(loc, regex);
        self.abstract_size = self.abstract_size.saturating_add(size_increase);
        let source_node = self.node_mut(&source)?;
        source_node.abstract_size = source_node.abstract_size.saturating_add(size_increase);
        Ok(())
    }

    pub fn abstract_size(&self) -> usize {
        self.abstract_size
    }

    pub fn reference_size(&self, id: Ref) -> Result<usize> {
        self.node(&id).map(|n| n.abstract_size)
    }

    //**********************************************************************************************
    // Ref API
    //**********************************************************************************************

    pub fn release(&mut self, r: Ref) -> Result<()> {
        self.check_invariants();
        let Some(rnode) = self.nodes.remove(&r) else {
            bail!("missing ref {:?}", r)
        };
        self.abstract_size = self.abstract_size.saturating_sub(rnode.abstract_size);
        self.graph.remove_edge(r, r);
        for (&n, node) in self.nodes.iter_mut() {
            self.graph.remove_edge(r, n);
            if let Some(e) = self.graph.remove_edge(n, r) {
                debug_assert_ne!(n, r);
                node.abstract_size = node.abstract_size.saturating_sub(e.abstract_size());
                self.abstract_size = self.abstract_size.saturating_sub(e.abstract_size());
            }
        }
        self.graph.remove_node(r);
        self.check_invariants();
        Ok(())
    }

    pub fn release_all(&mut self) {
        self.abstract_size = 0;
        self.nodes.clear();
        self.graph.clear();
        self.fresh_id = 0
    }

    //**********************************************************************************************
    // Query API
    //**********************************************************************************************

    /// Returns the references that extend the specified reference and the path(s) for the extension
    pub fn borrowed_by(&self, r: Ref) -> Result<BTreeMap<Ref, Paths<Loc, Lbl>>> {
        let mut paths = BTreeMap::new();
        for (edge, s) in self.successors(r)? {
            if r == s {
                // skip self epsilon
                continue;
            }
            let _prev = paths.insert(s, edge.paths());
            debug_assert!(_prev.is_none());
        }
        Ok(paths)
    }

    /// Returns the references that are extended by the specified reference and the path(s) for the
    /// extension
    pub fn borrows_from(&self, r: Ref) -> Result<BTreeMap<Ref, Paths<Loc, Lbl>>> {
        let mut paths = BTreeMap::new();
        for (p, edge) in self.predecessors(r)? {
            if r == p {
                // skip self epsilon
                continue;
            }
            let _prev = paths.insert(p, edge.paths());
            debug_assert!(_prev.is_none());
        }
        Ok(paths)
    }

    //**********************************************************************************************
    // Joining
    //**********************************************************************************************

    /// Returns true if self changed
    pub fn join(&mut self, other: &Self) -> Result<bool> {
        self.check_join_invariants(other);
        let mut size_increase = 0usize;
        let self_keys = self.keys().collect::<BTreeSet<_>>();
        for (p, s, other_edge) in other
            .graph
            .all_edges()
            .filter(|(p, s, _)| self_keys.contains(p) && self_keys.contains(s))
        {
            if !self.graph.contains_edge(p, s) {
                self.graph.add_edge(p, s, Edge::new());
            }
            let self_edge_mut = self.graph.edge_weight_mut(p, s).unwrap();
            let edge_size_increase = self_edge_mut.join(other_edge);
            let node_mut = self.node_mut(&p)?;
            node_mut.abstract_size = node_mut.abstract_size.saturating_add(edge_size_increase);
            size_increase = size_increase.saturating_add(edge_size_increase);
        }
        self.abstract_size = self.abstract_size.saturating_add(size_increase);
        self.check_invariants();
        Ok(size_increase > 0)
    }

    /// Refresh all references (making them no longer canonical)
    pub fn refresh_refs(&mut self) -> Result<()> {
        let nodes = std::mem::take(&mut self.nodes);
        let (ncap, ecap) = self.graph.capacity();
        let mut graph = std::mem::replace(&mut self.graph, DiGraphMap::with_capacity(ncap, ecap));
        self.fresh_id = 0;
        self.nodes = nodes
            .into_iter()
            .map(|(r, node)| {
                let r = r.refresh()?;
                self.fresh_id = std::cmp::max(self.fresh_id, r.fresh_id()? + 1);
                Ok((r, node))
            })
            .collect::<Result<_>>()?;
        for (p, s, edge_mut) in graph.all_edges_mut() {
            let p = p.refresh()?;
            let s = s.refresh()?;
            let edge = std::mem::replace(edge_mut, Edge::new());
            self.graph.add_edge(p, s, edge);
        }
        debug_assert!(self.is_fresh());
        Ok(())
    }

    /// Canonicalize all references according to the remapping. This allows graphs to have the same
    /// set of references before being joined.
    pub fn canonicalize(&mut self, remapping: &BTreeMap<Ref, usize>) -> Result<()> {
        let nodes = std::mem::take(&mut self.nodes);
        let (ncap, ecap) = self.graph.capacity();
        let mut graph = std::mem::replace(&mut self.graph, DiGraphMap::with_capacity(ncap, ecap));
        self.nodes = nodes
            .into_iter()
            .map(|(r, node)| Ok((r.canonicalize(remapping)?, node)))
            .collect::<Result<_>>()?;
        for (p, s, edge_mut) in graph.all_edges_mut() {
            let p = p.canonicalize(remapping)?;
            let s = s.canonicalize(remapping)?;
            let edge = std::mem::replace(edge_mut, Edge::new());
            self.graph.add_edge(p, s, edge);
        }
        self.fresh_id = 0;
        debug_assert!(self.is_canonical());
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
            let mut is_canonical_opt = None;
            for r in self.nodes.keys().copied() {
                debug_assert!(self.graph.contains_node(r));
                self.check_self_epsilon_invariant(r);
                match is_canonical_opt {
                    None => is_canonical_opt = Some(r.is_canonical()),
                    Some(is_canonical) => debug_assert_eq!(is_canonical, r.is_canonical()),
                }
            }
            for r in self.graph.nodes() {
                debug_assert!(self.nodes.contains_key(&r));
            }
            let mut calculated_size = 0;
            for (&r, node) in &self.nodes {
                let mut node_size = 1;
                for (edge, s) in self.successors(r).unwrap() {
                    debug_assert!(self.graph.contains_edge(r, s));
                    edge.check_invariants();
                    node_size += edge.abstract_size();
                }
                assert_eq!(node.abstract_size, node_size);
                calculated_size += node.abstract_size;
            }
            debug_assert_eq!(calculated_size, self.abstract_size);
        }
    }

    fn check_self_epsilon_invariant(&self, r: Ref) {
        #[cfg(debug_assertions)]
        {
            let edge_opt = self.graph.edge_weight(r, r);
            debug_assert!(edge_opt.is_some());
            let rs = self
                .graph
                .edge_weight(r, r)
                .unwrap()
                .regexes()
                .collect::<Vec<_>>();
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
