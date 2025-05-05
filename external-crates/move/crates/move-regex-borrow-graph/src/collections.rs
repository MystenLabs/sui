// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    bail, ensure, error,
    references::{Node, Ref},
    regex::{Extension, Regex},
    Result,
};
use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

//**************************************************************************************************
// Definitions
//**************************************************************************************************

#[derive(Clone, Debug)]
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
    nodes: BTreeMap<Ref, Node<Loc, Lbl>>,
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
        initial_refs: impl IntoIterator<Item = (K, /* is_mut */ bool)>,
    ) -> Result<(Self, BTreeMap<K, Ref>)> {
        let mut map = BTreeMap::new();
        let mut graph = Self {
            fresh_id: 0,
            abstract_size: 0,
            nodes: BTreeMap::new(),
        };
        for (k, is_mut) in initial_refs {
            let r = graph.add_ref(is_mut)?;
            ensure!(!map.contains_key(&k), "key {:?} already exists", k);
            map.insert(k, r);
        }
        graph.check_invariant();
        Ok((graph, map))
    }

    pub fn is_mutable(&self, r: Ref) -> Result<bool> {
        self.node(&r).map(|n| n.is_mutable())
    }

    fn node(&self, r: &Ref) -> Result<&Node<Loc, Lbl>> {
        self.nodes
            .get(r)
            .ok_or_else(|| error!("missing ref {:?}", r))
    }

    fn node_mut(&mut self, r: &Ref) -> Result<&mut Node<Loc, Lbl>> {
        self.nodes
            .get_mut(r)
            .ok_or_else(|| error!("missing ref {:?}", r))
    }

    fn add_ref(&mut self, is_mut: bool) -> Result<Ref> {
        let id = self.fresh_id;
        self.fresh_id += 1;
        let r = Ref::fresh(id);
        let prev = self.nodes.insert(r, Node::new(r, is_mut));
        ensure!(prev.is_none(), "ref {:?} already exists", r);
        self.abstract_size = self.abstract_size.saturating_add(1);
        Ok(r)
    }

    pub fn extend_by_epsilon(
        &mut self,
        loc: Loc,
        sources: impl IntoIterator<Item = Ref>,
        is_mut: bool,
    ) -> Result<Ref> {
        let new_ref = self.add_ref(is_mut)?;
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
        let new_ref = self.add_ref(is_mut)?;
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
            .map(|is_mut| self.add_ref(*is_mut))
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
            for mut_new_ref in &mut_new_refs {
                for s in self
                    .node(mut_new_ref)
                    .unwrap()
                    .successors()
                    .filter(|s| s != mut_new_ref)
                {
                    debug_assert!(!imm_new_refs.contains(&s));
                    debug_assert!(!mut_new_refs.contains(&s));
                }
            }
            for imm_new_ref in &imm_new_refs {
                for s in self
                    .node(imm_new_ref)
                    .unwrap()
                    .successors()
                    .filter(|s| s != imm_new_ref)
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
        self.check_invariant();
        let mut edges_to_add = vec![];
        for x in sources {
            debug_assert!(!exclude.contains(&x));
            self.determine_new_edges(&mut edges_to_add, x, &ext, new_ref, exclude)?;
        }
        for (p, r, s) in edges_to_add {
            debug_assert!(p == new_ref || s == new_ref);
            self.add_edge(p, loc, r, s)?;
        }
        self.check_invariant();
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
        for y in self
            .node(&x)?
            .predecessors()
            .filter(|y| !exclude.contains(y))
        {
            for y_to_x in self.node(&y)?.regexes(&x)? {
                edges_to_add.push((y, y_to_x.clone().extend(ext), new_ref))
            }
        }
        for y in self.node(&x)?.successors().filter(|y| !exclude.contains(y)) {
            for x_to_y in self.node(&x)?.regexes(&y)? {
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

    fn add_edge(
        &mut self,
        predecessor: Ref,
        loc: Loc,
        regex: Regex<Lbl>,
        successor: Ref,
    ) -> Result<()> {
        if predecessor == successor {
            ensure!(
                regex.is_epsilon(),
                "self edge must be epsilon {:?} --{}--> {:?}",
                predecessor,
                regex,
                successor
            );
            return Ok(());
        }
        let predecessor_node = self.node_mut(&predecessor)?;
        let size_increase = predecessor_node.add_regex(loc, regex, successor);
        self.abstract_size = self.abstract_size.saturating_add(size_increase);
        let successor_node = self.node_mut(&successor)?;
        successor_node.add_predecessor(predecessor);
        Ok(())
    }

    pub fn abstract_size(&self) -> usize {
        self.abstract_size
    }

    pub fn reference_size(&self, id: Ref) -> Result<usize> {
        self.node(&id).map(|n| n.abstract_size())
    }

    //**********************************************************************************************
    // Ref API
    //**********************************************************************************************

    pub fn release(&mut self, r: Ref) -> Result<()> {
        let Some(node) = self.nodes.remove(&r) else {
            bail!("missing ref {:?}", r)
        };
        self.abstract_size = self.abstract_size.saturating_sub(node.abstract_size());
        for other in node.successors().chain(node.predecessors()) {
            if r == other {
                // skip self epsilon
                continue;
            }
            self.abstract_size = self
                .abstract_size
                .saturating_sub(self.node_mut(&other)?.remove_neighbor(r));
        }
        Ok(())
    }

    pub fn release_all(&mut self) {
        self.abstract_size = 0;
        self.nodes.clear();
        self.fresh_id = 0
    }

    //**********************************************************************************************
    // Query API
    //**********************************************************************************************

    // returns successors
    pub fn borrowed_by(&self, r: Ref) -> Result<BTreeMap<Ref, Paths<Loc, Lbl>>> {
        let node = self.node(&r)?;
        let mut paths = BTreeMap::new();
        for s in node.successors() {
            if r == s {
                // skip self epsilon
                continue;
            }
            let _prev = paths.insert(s, node.paths(&s)?);
            debug_assert!(_prev.is_none());
        }
        Ok(paths)
    }

    // returns predecessors
    pub fn borrows_from(&self, id: Ref) -> Result<BTreeMap<Ref, Paths<Loc, Lbl>>> {
        let node = self.node(&id)?;
        let mut paths = BTreeMap::new();
        for p in node.predecessors() {
            if id == p {
                // skip self epsilon
                continue;
            }
            let _prev = paths.insert(p, self.node(&p)?.paths(&id)?);
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
        for (r, other_node) in other.nodes.iter().filter(|(r, _)| self_keys.contains(r)) {
            let self_node = self.node_mut(r)?;
            size_increase = size_increase.saturating_add(self_node.join(&self_keys, other_node));
        }
        self.abstract_size = self.abstract_size.saturating_add(size_increase);
        self.check_invariant();
        Ok(size_increase > 0)
    }

    pub fn refresh_refs(&mut self) -> Result<()> {
        let nodes = std::mem::take(&mut self.nodes);
        self.fresh_id = 0;
        self.nodes = nodes
            .into_iter()
            .map(|(r, node)| {
                let r = r.refresh()?;
                self.fresh_id = std::cmp::max(self.fresh_id, r.fresh_id()? + 1);
                let node = node.refresh_refs()?;
                Ok((r, node))
            })
            .collect::<Result<_>>()?;
        debug_assert!(self.is_fresh());
        Ok(())
    }

    pub fn canonicalize(&mut self, remapping: &BTreeMap<Ref, usize>) -> Result<()> {
        let nodes = std::mem::take(&mut self.nodes);
        self.nodes = nodes
            .into_iter()
            .map(|(r, node)| Ok((r.canonicalize(remapping)?, node.canonicalize(remapping)?)))
            .collect::<Result<_>>()?;
        self.fresh_id = 0;
        debug_assert!(self.is_canonical());
        Ok(())
    }

    pub fn is_canonical(&self) -> bool {
        self.nodes.keys().all(|r| r.is_canonical())
    }

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
            self.check_invariant();
            other.check_invariant();
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
                debug_assert_eq!(self_node.ref_(), other_node.ref_());
                debug_assert_eq!(self_node.is_mutable(), other_node.is_mutable());
            }
        }
    }

    // checks:
    // - ref --> node has ref == node.ref()
    // - successor/predecessor relationship is correctly maintained
    // - the abstract size is correct
    pub fn check_invariant(&self) {
        #[cfg(debug_assertions)]
        {
            for (id, node) in &self.nodes {
                debug_assert_eq!(id, &node.ref_());
            }
            let mut calculated_size = 0;
            for (r, node) in &self.nodes {
                node.check_invariant();
                calculated_size += node.abstract_size();
                for s in node.successors() {
                    debug_assert!(self.nodes[&s].is_predecessor(r));
                }
                for p in node.predecessors() {
                    debug_assert!(self.nodes[&p].is_successor(r));
                }
            }
            debug_assert_eq!(calculated_size, self.abstract_size);
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

impl<Loc, Lbl: Ord> fmt::Display for Graph<Loc, Lbl>
where
    Lbl: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (r, node) in &self.nodes {
            writeln!(f, "{r}: {{{node}}}")?;
        }
        Ok(())
    }
}
