// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Concrete-model soundness tests for the borrow graph.
//!
//! The other test modules check algebraic identities of `Regex`, or check a single graph
//! extension against the relations that existed just before it. These tests instead carry a
//! *concrete* model alongside the abstract graph: every live reference is assigned an actual
//! memory location (a tree root plus a path), and every operation updates both.
//!
//! The property under test is the closure/completeness claim the bytecode verifier relies on:
//!
//! > For every pair of live references `x`, `y` such that in the concrete state
//! > `loc(y) = loc(x) ++ w`, the *direct* edge `x --> y` must contain a regex matching `w`.
//!
//! `is_writable`, `are_transferrable` and `is_local_borrowed` all look only at direct
//! successors (`borrowed_by`), so if this ever fails they silently miss a hazard.
//!
//! The generated histories are restricted to states the verifier can actually reach:
//! * a `&mut` is only ever derived from a `&mut`;
//! * a call is only performed when `are_transferrable` holds for its arguments (mirroring
//!   `regex_reference_safety::abstract_state`);
//! * a mutable call result is an extension of a mutable argument, and is disjoint from every
//!   other result of the same call (this is what the callee's `Ret` check enforces).

use crate::{
    collections::{Graph, Path},
    meter::DummyMeter,
    references::Ref,
    regex::{Extension, Regex},
};
use proptest::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

type G = Graph<(), char>;

const ALPHABET: [char; 2] = ['a', 'b'];
// Paths used as witnesses when comparing two graphs edge-language to edge-language.
const WITNESS_LEN: usize = 4;
const GRAPH_CAPACITY: usize = 64;

// -------------------------------------------------------------------------------------------------
// Concrete locations
// -------------------------------------------------------------------------------------------------

/// A concrete memory location: which disjoint tree, and the path within it.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Cloc {
    root: usize,
    path: Vec<char>,
}

impl Cloc {
    /// `Some(w)` when `self ++ w == other`.
    fn suffix_to(&self, other: &Cloc) -> Option<Vec<char>> {
        if self.root != other.root || other.path.len() < self.path.len() {
            return None;
        }
        if other.path[..self.path.len()] != self.path[..] {
            return None;
        }
        Some(other.path[self.path.len()..].to_vec())
    }

    /// True when one location is a prefix of the other, i.e. the two overlap in memory.
    fn overlaps(&self, other: &Cloc) -> bool {
        self.suffix_to(other).is_some() || other.suffix_to(self).is_some()
    }

    fn extended(&self, suffix: &[char]) -> Cloc {
        Cloc {
            root: self.root,
            path: self.path.iter().chain(suffix).copied().collect(),
        }
    }
}

fn path_matches(p: &Path<(), char>, w: &[char]) -> bool {
    if p.ends_in_dot_star {
        w.starts_with(&p.labels)
    } else {
        w == p.labels.as_slice()
    }
}

fn regex_matches(r: &Regex<char>, w: &[char]) -> bool {
    if r.ends_in_dot_star {
        w.starts_with(&r.labels)
    } else {
        w == r.labels.as_slice()
    }
}

fn witness_paths() -> Vec<Vec<char>> {
    let mut paths = vec![vec![]];
    let mut frontier = vec![vec![]];
    for _ in 0..WITNESS_LEN {
        let mut next = vec![];
        for path in &frontier {
            for label in ALPHABET {
                let mut extended = path.clone();
                extended.push(label);
                paths.push(extended.clone());
                next.push(extended);
            }
        }
        frontier = next;
    }
    paths
}

// -------------------------------------------------------------------------------------------------
// Verifier predicates, mirrored
// -------------------------------------------------------------------------------------------------

// Mirrors `regex_reference_safety::abstract_state::AbstractState::is_writable`.
fn is_writable(graph: &G, r: Ref) -> bool {
    graph.is_mutable(r).unwrap()
        && graph
            .borrowed_by(r, &mut DummyMeter)
            .unwrap()
            .values()
            .all(|paths| paths.iter().all(|path| path.is_epsilon()))
}

// Mirrors `regex_reference_safety::abstract_state::AbstractState::are_transferrable`.
fn are_transferrable(graph: &G, refs: &BTreeSet<Ref>) -> bool {
    let mut_refs = refs
        .iter()
        .copied()
        .filter(|r| graph.is_mutable(*r).unwrap())
        .collect::<BTreeSet<_>>();
    for r in refs.iter().copied() {
        let is_mut = mut_refs.contains(&r);
        for (borrower, paths) in graph.borrowed_by(r, &mut DummyMeter).unwrap() {
            if !is_mut {
                if mut_refs.contains(&borrower) {
                    return false;
                }
            } else {
                for path in &paths {
                    if !path.is_epsilon() || refs.contains(&borrower) {
                        return false;
                    }
                }
            }
        }
    }
    true
}

// -------------------------------------------------------------------------------------------------
// Concrete model
// -------------------------------------------------------------------------------------------------

/// The completeness check, factored out so a single graph can be checked against more than one
/// concrete state (which is what a joined graph has to satisfy).
fn check_complete_with(graph: &G, refs: &[Ref], locs: &[Cloc]) -> Result<(), String> {
    assert_eq!(refs.len(), locs.len());
    for i in 0..refs.len() {
        let borrowed = graph.borrowed_by(refs[i], &mut DummyMeter).unwrap();
        for j in 0..refs.len() {
            if i == j {
                continue;
            }
            let Some(w) = locs[i].suffix_to(&locs[j]) else {
                continue;
            };
            let covered = borrowed
                .get(&refs[j])
                .is_some_and(|paths| paths.iter().any(|p| path_matches(p, &w)));
            if !covered {
                return Err(format!(
                    "missing edge {} --{:?}--> {} (locs {:?} / {:?}); borrowed_by({}) = {:?}",
                    refs[i],
                    w,
                    refs[j],
                    locs[i],
                    locs[j],
                    refs[i],
                    borrowed
                        .iter()
                        .map(|(r, ps)| (
                            r.to_string(),
                            ps.iter()
                                .map(|p| (p.labels.clone(), p.ends_in_dot_star))
                                .collect::<Vec<_>>()
                        ))
                        .collect::<Vec<_>>()
                ));
            }
        }
    }
    Ok(())
}

struct Model {
    graph: G,
    refs: Vec<Ref>,
    locs: Vec<Cloc>,
    muts: Vec<bool>,
    next_root: usize,
}

impl Model {
    fn new() -> Self {
        let (graph, _) = Graph::<(), char>::new(GRAPH_CAPACITY, Vec::<(u8, (), bool)>::new())
            .expect("empty graph");
        Model {
            graph,
            refs: vec![],
            locs: vec![],
            muts: vec![],
            next_root: 0,
        }
    }

    fn len(&self) -> usize {
        self.refs.len()
    }

    fn record(&mut self, r: Ref, loc: Cloc, is_mut: bool) {
        self.refs.push(r);
        self.locs.push(loc);
        self.muts.push(is_mut);
    }

    fn forget(&mut self, idx: usize) {
        self.refs.remove(idx);
        self.locs.remove(idx);
        self.muts.remove(idx);
    }

    fn release(&mut self, idx: usize) {
        let r = self.refs[idx];
        self.graph.release(r, &mut DummyMeter).unwrap();
        self.forget(idx);
    }

    fn new_root(&mut self, is_mut: bool) {
        let r = self
            .graph
            .extend_by_epsilon((), std::iter::empty(), is_mut, &mut DummyMeter)
            .unwrap();
        let root = self.next_root;
        self.next_root += 1;
        self.record(r, Cloc { root, path: vec![] }, is_mut);
    }

    fn alias(&mut self, base: usize, is_mut: bool) {
        // A `&mut` is only ever produced from a `&mut` (`CopyLoc`); `FreezeRef` goes the other way.
        let is_mut = is_mut && self.muts[base];
        let r = self
            .graph
            .extend_by_epsilon(
                (),
                std::iter::once(self.refs[base]),
                is_mut,
                &mut DummyMeter,
            )
            .unwrap();
        let loc = self.locs[base].clone();
        self.record(r, loc, is_mut);
    }

    fn field(&mut self, base: usize, label: char, is_mut: bool) {
        let is_mut = is_mut && self.muts[base];
        let r = self
            .graph
            .extend_by_label(
                (),
                std::iter::once(self.refs[base]),
                is_mut,
                label,
                &mut DummyMeter,
            )
            .unwrap();
        let loc = self.locs[base].extended(&[label]);
        self.record(r, loc, is_mut);
    }

    /// Returns false when the call is not performed (the verifier would have rejected it, or
    /// there is nothing to model).
    fn call(&mut self, arg_idxs: &BTreeSet<usize>, results: &[(bool, usize, Vec<char>)]) -> bool {
        if arg_idxs.is_empty() {
            return false;
        }
        let arg_refs = arg_idxs
            .iter()
            .map(|&i| self.refs[i])
            .collect::<BTreeSet<_>>();
        if !are_transferrable(&self.graph, &arg_refs) {
            return false;
        }
        let mut_arg_idxs = arg_idxs
            .iter()
            .copied()
            .filter(|&i| self.muts[i])
            .collect::<Vec<_>>();
        let arg_idx_vec = arg_idxs.iter().copied().collect::<Vec<_>>();

        // Pick concrete locations for the results, dropping any result that would violate the
        // disjointness the callee's `Ret` check guarantees.
        let mut kept: Vec<(bool, Cloc)> = vec![];
        for (want_mut, src_choice, suffix) in results {
            let (is_mut, src) = if *want_mut && !mut_arg_idxs.is_empty() {
                (true, mut_arg_idxs[src_choice % mut_arg_idxs.len()])
            } else {
                (false, arg_idx_vec[src_choice % arg_idx_vec.len()])
            };
            let loc = self.locs[src].extended(suffix);
            let disjoint_enough = kept
                .iter()
                .all(|(other_mut, other_loc)| !(is_mut || *other_mut) || !loc.overlaps(other_loc));
            if disjoint_enough {
                kept.push((is_mut, loc));
            }
        }

        let mutabilities = kept.iter().map(|(m, _)| *m).collect::<Vec<_>>();
        let new_refs = self
            .graph
            .extend_by_dot_star_for_call((), &arg_refs, mutabilities, &mut DummyMeter)
            .unwrap();
        assert_eq!(new_refs.len(), kept.len());
        for (r, (is_mut, loc)) in new_refs.into_iter().zip(kept) {
            self.record(r, loc, is_mut);
        }
        // `AbstractState::call` releases the argument references after the call.
        for idx in arg_idxs.iter().rev() {
            self.release(*idx);
        }
        true
    }

    /// THE property: every concrete containment between two live references is covered by a
    /// direct edge.
    fn check_complete(&self) -> Result<(), String> {
        check_complete_with(&self.graph, &self.refs, &self.locs)
    }

    /// A corollary of `check_complete` that is worth failing separately: a reference with a
    /// strictly-inside live reference must not be writable.
    fn check_writability(&self) -> Result<(), String> {
        for i in 0..self.len() {
            if !self.muts[i] || !is_writable(&self.graph, self.refs[i]) {
                continue;
            }
            for j in 0..self.len() {
                if i == j {
                    continue;
                }
                if self.locs[i]
                    .suffix_to(&self.locs[j])
                    .is_some_and(|w| !w.is_empty())
                {
                    return Err(format!(
                        "{} is writable but {} lives strictly inside it ({:?} / {:?})",
                        self.refs[i], self.refs[j], self.locs[i], self.locs[j]
                    ));
                }
            }
        }
        Ok(())
    }
}

// -------------------------------------------------------------------------------------------------
// Op generation
// -------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum RawOp {
    Root(bool),
    Alias(u8, bool),
    Field(u8, u8, bool),
    Release(u8),
    Call(Vec<u8>, Vec<(bool, u8, Vec<u8>)>),
}

fn arb_raw_op() -> impl Strategy<Value = RawOp> {
    prop_oneof![
        2 => any::<bool>().prop_map(RawOp::Root),
        5 => (any::<u8>(), any::<bool>()).prop_map(|(b, m)| RawOp::Alias(b, m)),
        8 => (any::<u8>(), any::<u8>(), any::<bool>())
            .prop_map(|(b, l, m)| RawOp::Field(b, l, m)),
        2 => any::<u8>().prop_map(RawOp::Release),
        6 => (
            prop::collection::vec(any::<u8>(), 1..=3),
            prop::collection::vec(
                (any::<bool>(), any::<u8>(), prop::collection::vec(any::<u8>(), 0..=2)),
                0..=3,
            ),
        )
            .prop_map(|(a, r)| RawOp::Call(a, r)),
    ]
}

fn run_history(ops: &[RawOp]) -> Model {
    let mut model = Model::new();
    // Always start with a root so the first ops have something to work with.
    model.new_root(true);
    for op in ops {
        // Bound the graph so the debug capacity assertions stay satisfied.
        if model.len() >= GRAPH_CAPACITY / 2 {
            break;
        }
        match op {
            RawOp::Root(is_mut) => model.new_root(*is_mut),
            RawOp::Alias(base, is_mut) => {
                if model.len() == 0 {
                    continue;
                }
                model.alias(*base as usize % model.len(), *is_mut)
            }
            RawOp::Field(base, label, is_mut) => {
                if model.len() == 0 {
                    continue;
                }
                let label = ALPHABET[*label as usize % ALPHABET.len()];
                model.field(*base as usize % model.len(), label, *is_mut)
            }
            RawOp::Release(idx) => {
                if model.len() <= 1 {
                    continue;
                }
                model.release(*idx as usize % model.len())
            }
            RawOp::Call(args, results) => {
                if model.len() == 0 {
                    continue;
                }
                let arg_idxs = args
                    .iter()
                    .map(|a| *a as usize % model.len())
                    .collect::<BTreeSet<_>>();
                let results = results
                    .iter()
                    .map(|(is_mut, src, suffix)| {
                        let suffix = suffix
                            .iter()
                            .map(|s| ALPHABET[*s as usize % ALPHABET.len()])
                            .collect::<Vec<_>>();
                        (*is_mut, *src as usize, suffix)
                    })
                    .collect::<Vec<_>>();
                model.call(&arg_idxs, &results);
            }
        }
    }
    model
}

// -------------------------------------------------------------------------------------------------
// Relation snapshots (abstract, over the witness alphabet)
// -------------------------------------------------------------------------------------------------

/// `relation[(x, y)]` = the set of witness paths `w` matched by the direct edge `x --> y`.
/// The self-epsilon edge is added explicitly because the query API omits it.
fn relation(graph: &G, refs: &[Ref]) -> BTreeMap<(Ref, Ref), BTreeSet<Vec<char>>> {
    let witnesses = witness_paths();
    let live = refs.iter().copied().collect::<BTreeSet<_>>();
    let mut out = BTreeMap::new();
    for &x in refs {
        out.insert((x, x), BTreeSet::from([Vec::new()]));
        for (y, paths) in graph.borrowed_by(x, &mut DummyMeter).unwrap() {
            if !live.contains(&y) {
                continue;
            }
            let matched = witnesses
                .iter()
                .filter(|w| paths.iter().any(|p| path_matches(p, w)))
                .cloned()
                .collect::<BTreeSet<_>>();
            out.entry((x, y))
                .or_insert_with(BTreeSet::new)
                .extend(matched);
        }
    }
    out
}

fn matched(rel: &BTreeMap<(Ref, Ref), BTreeSet<Vec<char>>>, x: Ref, y: Ref) -> BTreeSet<Vec<char>> {
    rel.get(&(x, y)).cloned().unwrap_or_default()
}

// -------------------------------------------------------------------------------------------------
// Properties
// -------------------------------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    /// The closure/completeness property, checked after every step of a random valid history.
    #[test]
    fn graph_is_complete_against_the_concrete_model(
        ops in prop::collection::vec(arb_raw_op(), 0..=14),
    ) {
        let mut model = Model::new();
        model.new_root(true);
        for op in &ops {
            if model.len() >= GRAPH_CAPACITY / 2 {
                break;
            }
            match op {
                RawOp::Root(is_mut) => model.new_root(*is_mut),
                RawOp::Alias(base, is_mut) => {
                    if model.len() == 0 { continue }
                    model.alias(*base as usize % model.len(), *is_mut)
                }
                RawOp::Field(base, label, is_mut) => {
                    if model.len() == 0 { continue }
                    let label = ALPHABET[*label as usize % ALPHABET.len()];
                    model.field(*base as usize % model.len(), label, *is_mut)
                }
                RawOp::Release(idx) => {
                    if model.len() <= 1 { continue }
                    model.release(*idx as usize % model.len())
                }
                RawOp::Call(args, results) => {
                    if model.len() == 0 { continue }
                    let arg_idxs = args
                        .iter()
                        .map(|a| *a as usize % model.len())
                        .collect::<BTreeSet<_>>();
                    let results = results
                        .iter()
                        .map(|(is_mut, src, suffix)| {
                            let suffix = suffix
                                .iter()
                                .map(|s| ALPHABET[*s as usize % ALPHABET.len()])
                                .collect::<Vec<_>>();
                            (*is_mut, *src as usize, suffix)
                        })
                        .collect::<Vec<_>>();
                    model.call(&arg_idxs, &results);
                }
            }
            prop_assert!(model.check_complete().is_ok(), "{}", model.check_complete().unwrap_err());
            prop_assert!(
                model.check_writability().is_ok(),
                "{}",
                model.check_writability().unwrap_err()
            );
        }
    }

    /// `release` must not lose any relation between the surviving references. This is the
    /// operation that most directly depends on the graph being transitively closed.
    #[test]
    fn release_preserves_relations_between_survivors(
        ops in prop::collection::vec(arb_raw_op(), 0..=12),
        victim in any::<u8>(),
    ) {
        let mut model = run_history(&ops);
        // An early return rather than `prop_assume!`, so raising the case count does not trip
        // proptest's global-reject limit.
        if model.len() < 2 {
            return Ok(());
        }
        let victim = victim as usize % model.len();
        let victim_ref = model.refs[victim];
        let survivors = model
            .refs
            .iter()
            .copied()
            .filter(|r| *r != victim_ref)
            .collect::<Vec<_>>();
        let before = relation(&model.graph, &survivors);
        model.release(victim);
        let after = relation(&model.graph, &survivors);
        for &x in &survivors {
            for &y in &survivors {
                prop_assert_eq!(
                    matched(&before, x, y),
                    matched(&after, x, y),
                    "release({}) changed the relation {} -> {}",
                    victim_ref,
                    x,
                    y
                );
            }
        }
    }

    /// Every relation reachable in two hops must be covered by one direct edge. The abstract
    /// dot-star edges a call installs between its immutable results are deliberately excluded:
    /// they are a "we do not know how these two overlap" marker rather than a claim that any
    /// concrete two-hop path exists (`graph_is_complete_against_the_concrete_model` is the
    /// property that actually pins down soundness for calls).
    #[test]
    fn label_and_epsilon_graphs_are_transitively_closed(
        ops in prop::collection::vec(
            prop_oneof![
                (any::<u8>(), any::<bool>()).prop_map(|(b, m)| RawOp::Alias(b, m)),
                (any::<u8>(), any::<u8>(), any::<bool>())
                    .prop_map(|(b, l, m)| RawOp::Field(b, l, m)),
                any::<bool>().prop_map(RawOp::Root),
                any::<u8>().prop_map(RawOp::Release),
            ],
            0..=12,
        ),
    ) {
        let model = run_history(&ops);
        let refs = model.refs.clone();
        let rel = relation(&model.graph, &refs);
        for &x in &refs {
            for &z in &refs {
                let xz = matched(&rel, x, z);
                if xz.is_empty() { continue }
                for &y in &refs {
                    let zy = matched(&rel, z, y);
                    if zy.is_empty() { continue }
                    let xy = matched(&rel, x, y);
                    for u in &xz {
                        for v in &zy {
                            let uv = u.iter().chain(v).copied().collect::<Vec<_>>();
                            if uv.len() > WITNESS_LEN { continue }
                            prop_assert!(
                                xy.contains(&uv),
                                "closure broken: {} --{:?}--> {} --{:?}--> {} but {} --> {} = {:?}",
                                x, u, z, v, y, x, y, xy
                            );
                        }
                    }
                }
            }
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Join
// -------------------------------------------------------------------------------------------------

// Canonicalizes `refs` to 0..n, releasing everything else first, and returns the canonical refs.
fn canonicalize_to(model: &mut Model, keep: &[Ref]) -> Vec<Ref> {
    let keep_set = keep.iter().copied().collect::<BTreeSet<_>>();
    let doomed = model
        .refs
        .iter()
        .copied()
        .filter(|r| !keep_set.contains(r))
        .collect::<Vec<_>>();
    for r in doomed {
        let idx = model.refs.iter().position(|x| *x == r).unwrap();
        model.release(idx);
    }
    let remapping = keep
        .iter()
        .enumerate()
        .map(|(i, r)| (*r, i as u32))
        .collect::<BTreeMap<_, _>>();
    let canonical = keep
        .iter()
        .map(|r| r.canonicalize(&remapping).unwrap())
        .collect::<Vec<_>>();
    model.graph.canonicalize(&remapping).unwrap();
    for r in model.refs.iter_mut() {
        *r = r.canonicalize(&remapping).unwrap();
    }
    canonical
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    /// A join at a control-flow merge must keep every relation that either branch had.
    #[test]
    fn join_covers_both_branches(
        shared in prop::collection::vec(arb_raw_op(), 0..=5),
        left_ops in prop::collection::vec(arb_raw_op(), 0..=5),
        right_ops in prop::collection::vec(arb_raw_op(), 0..=5),
    ) {
        let base = run_history(&shared);
        if base.len() < 2 {
            return Ok(());
        }
        let keep = base.refs.clone();

        let build = |extra: &[RawOp]| {
            let mut model = Model {
                graph: base.graph.clone(),
                refs: base.refs.clone(),
                locs: base.locs.clone(),
                muts: base.muts.clone(),
                next_root: base.next_root,
            };
            for op in extra {
                if model.len() >= GRAPH_CAPACITY / 2 { break }
                match op {
                    RawOp::Root(is_mut) => model.new_root(*is_mut),
                    RawOp::Alias(b, m) => model.alias(*b as usize % model.len(), *m),
                    RawOp::Field(b, l, m) => {
                        let label = ALPHABET[*l as usize % ALPHABET.len()];
                        model.field(*b as usize % model.len(), label, *m)
                    }
                    // Releases and calls would drop references we need to keep on both sides.
                    RawOp::Release(_) | RawOp::Call(..) => continue,
                }
            }
            model
        };

        let mut left = build(&left_ops);
        let mut right = build(&right_ops);
        let left_canon = canonicalize_to(&mut left, &keep);
        let right_canon = canonicalize_to(&mut right, &keep);
        prop_assert_eq!(&left_canon, &right_canon);

        let left_before = relation(&left.graph, &left_canon);
        let right_before = relation(&right.graph, &right_canon);
        let changed = left.graph.join(&right.graph, &mut DummyMeter).unwrap();
        let joined = relation(&left.graph, &left_canon);

        let mut any_new = false;
        for &x in &left_canon {
            for &y in &left_canon {
                let want = matched(&left_before, x, y)
                    .union(&matched(&right_before, x, y))
                    .cloned()
                    .collect::<BTreeSet<_>>();
                let got = matched(&joined, x, y);
                prop_assert!(
                    want.is_subset(&got),
                    "join dropped relations for {} -> {}: want {:?} got {:?}",
                    x, y, want, got
                );
                if got != matched(&left_before, x, y) {
                    any_new = true;
                }
            }
        }
        // `join` reports "changed"; if it says nothing changed nothing may have been added.
        if !changed {
            prop_assert!(!any_new, "join reported no change but relations grew");
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Regex unit tests: the doc-comment tables and the Walk state machine
// -------------------------------------------------------------------------------------------------

fn regex_of(labels: &str, dot_star: bool) -> Regex<char> {
    Regex {
        labels: labels.chars().collect(),
        ends_in_dot_star: dot_star,
    }
}

fn show(r: &Regex<char>) -> String {
    let mut s = r.labels.iter().collect::<String>();
    if r.ends_in_dot_star {
        s.push_str(".*");
    }
    s
}

/// `Regex::extend` has no example table in its doc comment, unlike the two `remove_prefix`
/// functions. These are the cases the comment describes in prose, including the short-circuit.
#[test]
fn regex_extend_table() {
    let cases: &[(&str, bool, Extension<char>, &str)] = &[
        ("", false, Extension::Epsilon, ""),
        ("", false, Extension::Label('a'), "a"),
        ("", false, Extension::DotStar, ".*"),
        ("a", false, Extension::Epsilon, "a"),
        ("a", false, Extension::Label('b'), "ab"),
        ("a", false, Extension::DotStar, "a.*"),
        ("ab", false, Extension::Label('c'), "abc"),
        // The short-circuit: once the regex ends in dot-star, extensions are dropped.
        ("", true, Extension::Epsilon, ".*"),
        ("", true, Extension::Label('a'), ".*"),
        ("", true, Extension::DotStar, ".*"),
        ("a", true, Extension::Epsilon, "a.*"),
        ("a", true, Extension::Label('b'), "a.*"),
        ("a", true, Extension::DotStar, "a.*"),
    ];
    for (labels, dot_star, ext, expected) in cases {
        let got = regex_of(labels, *dot_star).extend(ext);
        assert_eq!(
            show(&got),
            *expected,
            "{}.extend({:?})",
            show(&regex_of(labels, *dot_star)),
            ext
        );
    }
}

/// The short-circuit drops labels, so it must only ever *widen* the language. `p.*` extended by
/// `l` denotes `p.*l`, and the implementation returns `p.*`, which contains it.
#[test]
fn regex_extend_short_circuit_only_widens() {
    let witnesses = witness_paths();
    for prefix in ["", "a", "ab"] {
        for label in ALPHABET {
            let before = regex_of(prefix, true);
            let after = before.clone().extend(&Extension::Label(label));
            for w in &witnesses {
                // w in `before . label` implies w in `after`
                let in_concat = !w.is_empty()
                    && w[w.len() - 1] == label
                    && regex_matches(&before, &w[..w.len() - 1]);
                assert!(
                    !in_concat || regex_matches(&after, w),
                    "{}.extend({}) = {} dropped {:?}",
                    show(&before),
                    label,
                    show(&after),
                    w
                );
            }
        }
    }
}

/// `Walk::next` is a no-op once the walk is sitting on a dot-star, so the `Extension::DotStar`
/// arm of `Regex::remove_prefix` would loop forever without its `ends_in_dot_star` early return.
/// This pins that behavior; if the early return is ever removed, this test hangs rather than
/// silently changing the answer.
#[test]
fn remove_dot_star_prefix_from_dot_star_regex_terminates() {
    for (labels, expected) in [("", ".*"), ("a", ".*"), ("abc", ".*")] {
        let r = regex_of(labels, true);
        let got = r.remove_prefix(&Extension::DotStar);
        assert_eq!(got.len(), 1);
        assert_eq!(show(&got[0]), expected);
    }
}

/// Both `remove_prefix` implementations return the empty set on their `debug_assert!(false)`
/// arms. The empty set means "no relation", i.e. an *under*-approximation, so those arms must be
/// unreachable. Every reachable input shape either produces a non-empty answer or is one of the
/// genuinely-no-prefix cases enumerated in the doc comments. This exhaustively checks the shapes
/// that reach the walk.
#[test]
fn remove_prefix_never_returns_empty_except_for_label_mismatch() {
    let regexes = [
        regex_of("", false),
        regex_of("a", false),
        regex_of("ab", false),
        regex_of("aba", false),
        regex_of("", true),
        regex_of("a", true),
        regex_of("ab", true),
    ];
    let exts = [
        Extension::Epsilon,
        Extension::Label('a'),
        Extension::Label('b'),
        Extension::DotStar,
    ];
    for r in &regexes {
        for e in &exts {
            let got = r.remove_prefix(e);
            // The only way to legitimately get [] is a leading-label mismatch (or epsilon).
            let expect_empty = match e {
                Extension::Epsilon | Extension::DotStar => false,
                Extension::Label(l) => {
                    !r.ends_in_dot_star && (r.labels.is_empty() || r.labels[0] != *l)
                        || (r.ends_in_dot_star && !r.labels.is_empty() && r.labels[0] != *l)
                }
            };
            assert_eq!(
                got.is_empty(),
                expect_empty,
                "{}.remove_prefix({:?}) = {:?}",
                show(r),
                e,
                got.iter().map(show).collect::<Vec<_>>()
            );
        }
    }
    for e in &exts {
        for r in &regexes {
            let got = e.remove_prefix(r);
            let expect_empty = match (e, r.labels.first()) {
                (_, None) => false,
                (Extension::DotStar, Some(_)) => false,
                (Extension::Epsilon, Some(_)) => true,
                (Extension::Label(l), Some(first)) => {
                    l != first
                        || (r.labels.len() > 1 && !r.ends_in_dot_star)
                        || (r.labels.len() > 1 && r.ends_in_dot_star)
                }
            };
            assert_eq!(
                got.is_empty(),
                expect_empty,
                "{:?}.remove_prefix({}) = {:?}",
                e,
                show(r),
                got.iter().map(show).collect::<Vec<_>>()
            );
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Targeted graph unit tests
// -------------------------------------------------------------------------------------------------

/// A call whose immutable results may overlap each other installs dot-star edges in both
/// directions between them. That makes the *abstract* relation non-transitive (following
/// `n1 --.*--> n2 --.*--> n1` yields dot-star, while the self edge is epsilon). It is only sound
/// because no concrete state has both `n2` strictly inside `n1` and `n1` strictly inside `n2`.
/// This test pins the shape so a future change to `extend_by_dot_star_for_call` is visible.
#[test]
fn immutable_call_results_form_an_abstract_dot_star_cycle() {
    let (mut graph, sources) = Graph::<(), char>::new(4, [(0, (), true)]).unwrap();
    let root = sources[&0];
    let results = graph
        .extend_by_dot_star_for_call(
            (),
            &BTreeSet::from([root]),
            vec![false, false],
            &mut DummyMeter,
        )
        .unwrap();
    let (n1, n2) = (results[0], results[1]);

    let n1_succ = graph.borrowed_by(n1, &mut DummyMeter).unwrap();
    let n2_succ = graph.borrowed_by(n2, &mut DummyMeter).unwrap();
    assert!(n1_succ[&n2].iter().any(|p| p.is_dot_star()));
    assert!(n2_succ[&n1].iter().any(|p| p.is_dot_star()));
    // ... but the self relation stays epsilon-only, so the abstract relation is not closed.
    assert!(!n1_succ.contains_key(&n1));
    // Neither result is writable-relevant here (both are immutable), but the parent is not
    // writable, since the results may be strictly inside it.
    assert!(!is_writable(&graph, root));
}

/// The mutable half of the same operation: mutable results get edges only from mutable sources,
/// and no edges at all to or from any other result of the same call.
#[test]
fn mutable_call_results_are_isolated_from_other_results() {
    let (mut graph, sources) = Graph::<(), char>::new(8, [(0, (), true), (1, (), false)]).unwrap();
    let (m, i) = (sources[&0], sources[&1]);
    let results = graph
        .extend_by_dot_star_for_call(
            (),
            &BTreeSet::from([m, i]),
            vec![true, true, false],
            &mut DummyMeter,
        )
        .unwrap();
    let (m1, m2, i1) = (results[0], results[1], results[2]);

    for r in [m1, m2] {
        let preds = graph.borrows_from(r, &mut DummyMeter).unwrap();
        assert!(preds.contains_key(&m), "mut result must extend the mut arg");
        assert!(
            !preds.contains_key(&i),
            "mut result must not extend the imm arg"
        );
        let succs = graph.borrowed_by(r, &mut DummyMeter).unwrap();
        for other in [m1, m2, i1] {
            if other == r {
                continue;
            }
            assert!(!succs.contains_key(&other));
            assert!(!preds.contains_key(&other));
        }
    }
    let i1_preds = graph.borrows_from(i1, &mut DummyMeter).unwrap();
    assert!(i1_preds.contains_key(&m));
    assert!(i1_preds.contains_key(&i));
}

/// A dot-star edge above a labelled edge must collapse into a single dot-star edge when the
/// intermediate reference is released; nothing may be lost.
#[test]
fn release_of_dot_star_middle_keeps_the_deep_relation() {
    let (mut graph, sources) = Graph::<(), char>::new(4, [(0, (), true)]).unwrap();
    let root = sources[&0];
    let call = graph
        .extend_by_dot_star_for_call((), &BTreeSet::from([root]), vec![true], &mut DummyMeter)
        .unwrap()[0];
    let inner = graph
        .extend_by_label((), [call], true, 'f', &mut DummyMeter)
        .unwrap();
    let deep = graph
        .extend_by_label((), [inner], true, 'g', &mut DummyMeter)
        .unwrap();

    graph.release(call, &mut DummyMeter).unwrap();
    graph.release(inner, &mut DummyMeter).unwrap();

    let succ = graph.borrowed_by(root, &mut DummyMeter).unwrap();
    let to_deep = succ.get(&deep).expect("root must still reach deep");
    // root --.*--> deep covers `w.f.g` for every w.
    assert!(to_deep.iter().any(|p| p.is_dot_star()));
    assert!(!is_writable(&graph, root));
}

// -------------------------------------------------------------------------------------------------
// Joined graphs must stay complete for BOTH concrete states
// -------------------------------------------------------------------------------------------------

/// One step applied to both branches. The *shape* (alias vs field) and the mutability are shared
/// so the two branches end up with the same reference set and the same mutabilities -- a
/// requirement of `Graph::join`. The base and the label are chosen independently, so the same
/// canonical reference ends up at a different concrete location on each branch, which is exactly
/// what happens at a real control-flow merge.
#[derive(Clone, Debug)]
struct TwinOp {
    is_field: bool,
    is_mut: bool,
    base_a: u8,
    base_b: u8,
    label_a: u8,
    label_b: u8,
}

fn arb_twin_op() -> impl Strategy<Value = TwinOp> {
    (
        any::<bool>(),
        any::<bool>(),
        any::<u8>(),
        any::<u8>(),
        any::<u8>(),
        any::<u8>(),
    )
        .prop_map(
            |(is_field, is_mut, base_a, base_b, label_a, label_b)| TwinOp {
                is_field,
                is_mut,
                base_a,
                base_b,
                label_a,
                label_b,
            },
        )
}

struct Branch {
    graph: G,
    refs: Vec<Ref>,
    locs: Vec<Cloc>,
    muts: Vec<bool>,
}

impl Branch {
    fn new() -> Self {
        let (mut graph, _) = Graph::<(), char>::new(GRAPH_CAPACITY, Vec::<(u8, (), bool)>::new())
            .expect("empty graph");
        let root = graph
            .extend_by_epsilon((), std::iter::empty(), true, &mut DummyMeter)
            .unwrap();
        Branch {
            graph,
            refs: vec![root],
            locs: vec![Cloc {
                root: 0,
                path: vec![],
            }],
            muts: vec![true],
        }
    }

    fn step(&mut self, is_field: bool, is_mut: bool, base_choice: u8, label_choice: u8) {
        // A `&mut` needs a `&mut` base; there is always at least the root.
        let candidates = if is_mut {
            (0..self.refs.len())
                .filter(|&i| self.muts[i])
                .collect::<Vec<_>>()
        } else {
            (0..self.refs.len()).collect::<Vec<_>>()
        };
        let base = candidates[base_choice as usize % candidates.len()];
        let label = ALPHABET[label_choice as usize % ALPHABET.len()];
        let (new_ref, loc) = if is_field {
            (
                self.graph
                    .extend_by_label(
                        (),
                        std::iter::once(self.refs[base]),
                        is_mut,
                        label,
                        &mut DummyMeter,
                    )
                    .unwrap(),
                self.locs[base].extended(&[label]),
            )
        } else {
            (
                self.graph
                    .extend_by_epsilon(
                        (),
                        std::iter::once(self.refs[base]),
                        is_mut,
                        &mut DummyMeter,
                    )
                    .unwrap(),
                self.locs[base].clone(),
            )
        };
        self.refs.push(new_ref);
        self.locs.push(loc);
        self.muts.push(is_mut);
    }

    fn canonicalize(&mut self) {
        let remapping = self
            .refs
            .iter()
            .enumerate()
            .map(|(i, r)| (*r, i as u32))
            .collect::<BTreeMap<_, _>>();
        self.refs = self
            .refs
            .iter()
            .map(|r| r.canonicalize(&remapping).unwrap())
            .collect();
        self.graph.canonicalize(&remapping).unwrap();
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 384, ..ProptestConfig::default() })]

    /// After a join the graph describes a *set* of concrete states, one per incoming branch. It
    /// must remain complete for every one of them, and must stay complete after the block
    /// continues to extend it. This is the loop / if-else fixpoint soundness argument.
    #[test]
    fn joined_graph_is_complete_for_every_branch(
        twins in prop::collection::vec(arb_twin_op(), 0..=7),
        after in prop::collection::vec(arb_twin_op(), 0..=4),
        call_arg in any::<u8>(),
        call_mut in any::<bool>(),
        call_suffix_a in prop::collection::vec(any::<u8>(), 0..=2),
        call_suffix_b in prop::collection::vec(any::<u8>(), 0..=2),
    ) {
        let mut left = Branch::new();
        let mut right = Branch::new();
        for t in &twins {
            left.step(t.is_field, t.is_mut, t.base_a, t.label_a);
            right.step(t.is_field, t.is_mut, t.base_b, t.label_b);
        }
        left.canonicalize();
        right.canonicalize();
        prop_assert_eq!(&left.refs, &right.refs);
        prop_assert_eq!(&left.muts, &right.muts);

        // Each branch on its own must be complete for its own concrete state.
        prop_assert!(
            check_complete_with(&left.graph, &left.refs, &left.locs).is_ok(),
            "{}",
            check_complete_with(&left.graph, &left.refs, &left.locs).unwrap_err()
        );

        left.graph.join(&right.graph, &mut DummyMeter).unwrap();

        // The joined graph must be complete for both.
        for (which, locs) in [("left", &left.locs), ("right", &right.locs)] {
            let res = check_complete_with(&left.graph, &left.refs, locs);
            prop_assert!(res.is_ok(), "after join, {} state: {}", which, res.unwrap_err());
        }

        // Continue the block. Both concrete states evolve; the single joined graph must track
        // both. Mutabilities are shared so the two states stay comparable.
        let mut joined = Branch {
            graph: left.graph,
            refs: left.refs.clone(),
            locs: left.locs.clone(),
            muts: left.muts.clone(),
        };
        let mut shadow_locs = right.locs.clone();
        joined.graph.refresh_refs().unwrap();
        joined.refs = joined.refs.iter().map(|r| r.refresh().unwrap()).collect();

        for t in &after {
            let candidates = if t.is_mut {
                (0..joined.refs.len()).filter(|&i| joined.muts[i]).collect::<Vec<_>>()
            } else {
                (0..joined.refs.len()).collect::<Vec<_>>()
            };
            let base = candidates[t.base_a as usize % candidates.len()];
            let label = ALPHABET[t.label_a as usize % ALPHABET.len()];
            let new_ref = if t.is_field {
                joined.graph
                    .extend_by_label((), std::iter::once(joined.refs[base]), t.is_mut, label, &mut DummyMeter)
                    .unwrap()
            } else {
                joined.graph
                    .extend_by_epsilon((), std::iter::once(joined.refs[base]), t.is_mut, &mut DummyMeter)
                    .unwrap()
            };
            joined.refs.push(new_ref);
            joined.muts.push(t.is_mut);
            if t.is_field {
                joined.locs.push(joined.locs[base].extended(&[label]));
                shadow_locs.push(shadow_locs[base].extended(&[label]));
            } else {
                joined.locs.push(joined.locs[base].clone());
                shadow_locs.push(shadow_locs[base].clone());
            }
            for (which, locs) in [("left", &joined.locs), ("right", &shadow_locs)] {
                let res = check_complete_with(&joined.graph, &joined.refs, locs);
                prop_assert!(res.is_ok(), "after join+extend, {} state: {}", which, res.unwrap_err());
            }
        }

        // Finally a one-argument call. The callee can return a reference at a different concrete
        // offset on each branch; the single dot-star edge has to cover both.
        let arg = call_arg as usize % joined.refs.len();
        let arg_set = BTreeSet::from([joined.refs[arg]]);
        if are_transferrable(&joined.graph, &arg_set) {
            let is_mut = call_mut && joined.muts[arg];
            let suffix_a = call_suffix_a
                .iter()
                .map(|s| ALPHABET[*s as usize % ALPHABET.len()])
                .collect::<Vec<_>>();
            let suffix_b = call_suffix_b
                .iter()
                .map(|s| ALPHABET[*s as usize % ALPHABET.len()])
                .collect::<Vec<_>>();
            let result = joined
                .graph
                .extend_by_dot_star_for_call((), &arg_set, vec![is_mut], &mut DummyMeter)
                .unwrap()[0];
            joined.refs.push(result);
            joined.muts.push(is_mut);
            joined.locs.push(joined.locs[arg].extended(&suffix_a));
            shadow_locs.push(shadow_locs[arg].extended(&suffix_b));
            // `call` releases the argument.
            joined.graph.release(joined.refs[arg], &mut DummyMeter).unwrap();
            joined.refs.remove(arg);
            joined.muts.remove(arg);
            joined.locs.remove(arg);
            shadow_locs.remove(arg);
            for (which, locs) in [("left", &joined.locs), ("right", &shadow_locs)] {
                let res = check_complete_with(&joined.graph, &joined.refs, locs);
                prop_assert!(res.is_ok(), "after join+call, {} state: {}", which, res.unwrap_err());
            }
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Canonicalization hazards
// -------------------------------------------------------------------------------------------------

/// `Graph::canonicalize` collects into a `BTreeMap<Ref, Node>`, so a remapping that sends two
/// distinct references to the same canonical id silently drops one node from `Graph::nodes`
/// while leaving it (and its edges) in the underlying `GraphMap`. Only a `debug_assert_eq!` on
/// the node counts catches it; in a release build the duplicate weight makes
/// `borrowed_by`/`borrows_from` overwrite one entry with the other and silently lose edges.
///
/// The only production caller (`regex_reference_safety::AbstractState::canonicalize`) builds an
/// injective map (`local_root -> 0`, `local i -> i + 1`), so this is currently unreachable. This
/// test pins the hazard: if the debug assertion ever goes away, it starts failing.
#[test]
#[should_panic]
fn canonicalize_with_non_injective_remapping_collapses_nodes() {
    let (mut graph, refs) = Graph::<(), char>::new(4, [(0, (), true), (1, (), true)]).unwrap();
    let remapping = BTreeMap::from([(refs[&0], 0u32), (refs[&1], 0u32)]);
    graph.canonicalize(&remapping).unwrap();
}

/// The `canonicalize -> refresh -> canonicalize` cycle a block boundary performs must not change
/// what the graph says about any pair of references.
#[test]
fn canonicalize_refresh_cycle_preserves_every_relation() {
    let mut branch = Branch::new();
    branch.step(true, true, 0, 0); // root.a
    branch.step(true, true, 1, 1); // root.a.b
    branch.step(false, false, 2, 0); // freeze-style alias of root.a.b
    branch.step(true, false, 0, 1); // imm root.b
    branch.canonicalize();

    let before = relation(&branch.graph, &branch.refs);
    branch.graph.refresh_refs().unwrap();
    branch.refs = branch.refs.iter().map(|r| r.refresh().unwrap()).collect();
    branch.canonicalize();
    let after = relation(&branch.graph, &branch.refs);

    for &x in &branch.refs {
        for &y in &branch.refs {
            assert_eq!(
                matched(&before, x, y),
                matched(&after, x, y),
                "canonicalize/refresh cycle changed {} -> {}",
                x,
                y
            );
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Structural invariants the edge-derivation rules silently depend on
// -------------------------------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    /// `determine_all_new_edges` derives `new --> x` only from the *outgoing* edges of a source.
    /// So for every reference `x` that concretely aliases a source, the graph must hold the edge
    /// in the source-to-`x` direction as well, not just `x`-to-source. This is the specific
    /// consequence of completeness that the edge-derivation rules depend on; it is checked here
    /// on its own so a regression points straight at the rule that broke.
    ///
    /// Note this is a claim about *concretely* aliasing references. It is deliberately not the
    /// stronger claim that a path matching the empty word implies a reverse edge: a dot-star
    /// edge matches the empty word without asserting an alias, and `Path::is_epsilon()` (which
    /// is what `is_writable`/`are_transferrable` consult) treats dot-star as a strict extension.
    #[test]
    fn concrete_aliases_have_edges_in_both_directions(
        ops in prop::collection::vec(arb_raw_op(), 0..=14),
    ) {
        let model = run_history(&ops);
        for i in 0..model.len() {
            for j in 0..model.len() {
                if i == j || model.locs[i] != model.locs[j] {
                    continue;
                }
                for (from, to) in [(i, j), (j, i)] {
                    let succ = model.graph.borrowed_by(model.refs[from], &mut DummyMeter).unwrap();
                    prop_assert!(
                        succ.get(&model.refs[to])
                            .is_some_and(|ps| ps.iter().any(|p| p.labels.is_empty())),
                        "aliases {} and {} (both at {:?}) have no {} --> {} edge",
                        model.refs[i],
                        model.refs[j],
                        model.locs[i],
                        model.refs[from],
                        model.refs[to],
                    );
                }
            }
        }
    }

    /// Every edge the graph holds must have a non-empty regex set, and no node may carry a
    /// non-epsilon self edge (`is_writable` would otherwise report a reference as borrowing
    /// itself).
    #[test]
    fn self_edges_stay_epsilon_only(ops in prop::collection::vec(arb_raw_op(), 0..=14)) {
        let model = run_history(&ops);
        for &x in &model.refs {
            let succ = model.graph.borrowed_by(x, &mut DummyMeter).unwrap();
            prop_assert!(!succ.contains_key(&x), "{} has a non-trivial self edge", x);
            for (_, paths) in &succ {
                prop_assert!(!paths.is_empty());
            }
        }
    }
}

/// `extend_by_dot_star_for_call` ties a mutable result only to the *mutable* sources. With no
/// mutable source at all it produces a reference with no incoming edges whatsoever, i.e. one the
/// graph believes is unrelated to everything -- writing through it, or through anything else,
/// is then unconstrained. The bytecode verifier never asks for this (a callee cannot manufacture
/// a `&mut` from an `&`), but nothing in this crate rules it out, so the shape is pinned here.
#[test]
fn mutable_result_without_a_mutable_source_is_unconstrained() {
    let (mut graph, sources) = Graph::<(), char>::new(4, [(0, (), false)]).unwrap();
    let imm = sources[&0];
    let result = graph
        .extend_by_dot_star_for_call((), &BTreeSet::from([imm]), vec![true], &mut DummyMeter)
        .unwrap()[0];

    assert!(
        graph
            .borrows_from(result, &mut DummyMeter)
            .unwrap()
            .is_empty(),
        "mut result should have been tied to something"
    );
    assert!(
        graph.borrowed_by(imm, &mut DummyMeter).unwrap().is_empty(),
        "imm source should have been tied to the mut result"
    );
    assert!(is_writable(&graph, result));
}

/// Same shape, with no sources at all.
#[test]
fn call_with_no_reference_arguments_produces_unconstrained_results() {
    let (mut graph, _) = Graph::<(), char>::new(4, Vec::<(u8, (), bool)>::new()).unwrap();
    let results = graph
        .extend_by_dot_star_for_call(
            (),
            &BTreeSet::new(),
            vec![true, false, false],
            &mut DummyMeter,
        )
        .unwrap();
    let (m, i1, i2) = (results[0], results[1], results[2]);
    assert!(graph.borrows_from(m, &mut DummyMeter).unwrap().is_empty());
    assert!(graph.borrowed_by(m, &mut DummyMeter).unwrap().is_empty());
    // The two immutable results still get the "might overlap" dot-star pair.
    assert!(
        graph.borrowed_by(i1, &mut DummyMeter).unwrap()[&i2]
            .iter()
            .any(|p| p.is_dot_star())
    );
    assert!(is_writable(&graph, m));
}

// -------------------------------------------------------------------------------------------------
// Precision losses that are conservative, pinned so a regression is visible
// -------------------------------------------------------------------------------------------------

/// The reference a call returns is recorded as `arg --.*--> result`, and dot-star is *not*
/// `Path::is_epsilon()`. So an alias of the argument that survives the call is reported as being
/// strictly borrowed even when the result is concretely the very same location. That is the
/// conservative direction (fewer writes allowed), but it costs expressivity, so pin it.
#[test]
fn call_results_are_never_recognised_as_epsilon_aliases() {
    let (mut graph, sources) = Graph::<(), char>::new(4, [(0, (), true)]).unwrap();
    let arg = sources[&0];
    let alias = graph
        .extend_by_epsilon((), [arg], true, &mut DummyMeter)
        .unwrap();
    let result = graph
        .extend_by_dot_star_for_call((), &BTreeSet::from([arg]), vec![true], &mut DummyMeter)
        .unwrap()[0];
    graph.release(arg, &mut DummyMeter).unwrap();

    let succ = graph.borrowed_by(alias, &mut DummyMeter).unwrap();
    let to_result = &succ[&result];
    assert!(to_result.iter().all(|p| !p.is_epsilon()));
    assert!(to_result.iter().any(|p| p.is_dot_star()));
    // The dot-star language does contain the empty word, so `result` may concretely be `alias`,
    // yet `alias` is reported as not writable.
    assert!(!is_writable(&graph, alias));
}

/// `Regex::extend` short-circuits on dot-star, so `x --.*--> y` extended by a label stays `.*`
/// rather than becoming `.*f`. The result over-approximates (it now also matches the empty word),
/// which is why `x --> z` below carries both `.*` and the exact `a`.
#[test]
fn dot_star_edge_extended_by_a_label_stays_dot_star() {
    let (mut graph, sources) = Graph::<(), char>::new(6, [(0, (), true)]).unwrap();
    let root = sources[&0];
    let alias = graph
        .extend_by_epsilon((), [root], true, &mut DummyMeter)
        .unwrap();
    let call = graph
        .extend_by_dot_star_for_call((), &BTreeSet::from([root]), vec![true], &mut DummyMeter)
        .unwrap()[0];
    graph.release(root, &mut DummyMeter).unwrap();
    let field = graph
        .extend_by_label((), [call], true, 'a', &mut DummyMeter)
        .unwrap();

    let succ = graph.borrowed_by(alias, &mut DummyMeter).unwrap();
    let to_field = &succ[&field];
    // Exact relation `a` (via the alias edge) plus the widened `.*` (via the call edge).
    assert!(to_field.iter().any(|p| p.is_label(&'a')));
    assert!(to_field.iter().any(|p| p.is_dot_star()));
    // No reverse edge: `field` is strictly inside `alias` in every concrete state.
    assert!(
        !graph
            .borrowed_by(field, &mut DummyMeter)
            .unwrap()
            .contains_key(&alias)
    );
}

// -------------------------------------------------------------------------------------------------
// The assumption `extend_by_dot_star_for_call` takes entirely on trust
// -------------------------------------------------------------------------------------------------

/// `extend_by_dot_star_for_call` installs **no** edge between a mutable result and any other
/// result of the same call. Nothing in this crate checks that, so the graph is complete only
/// because the *callee's* `Ret` check (`are_transferrable`) refuses to return a `&mut` that
/// overlaps any other returned reference.
///
/// This test exhibits the exact state that would exist if that guarantee were ever weakened:
/// a mutable result `m` at `root.a` and an immutable result `i` at `root.a.b`, with no edge
/// between them, so `m` is reported writable while `i` lives strictly inside it. Randomised
/// search over the concrete model finds this within a handful of histories once the
/// disjointness constraint is dropped.
#[test]
fn overlapping_call_results_would_break_completeness() {
    let (mut graph, sources) = Graph::<(), char>::new(6, [(0, (), true)]).unwrap();
    let root = sources[&0];
    let results = graph
        .extend_by_dot_star_for_call(
            (),
            &BTreeSet::from([root]),
            vec![/* mut */ true, /* imm */ false],
            &mut DummyMeter,
        )
        .unwrap();
    let (m, i) = (results[0], results[1]);
    graph.release(root, &mut DummyMeter).unwrap();

    // Pretend the callee returned `&mut root.a` and `&root.a.b`.
    let locs = [
        Cloc {
            root: 0,
            path: vec!['a'],
        },
        Cloc {
            root: 0,
            path: vec!['a', 'b'],
        },
    ];
    let err = check_complete_with(&graph, &[m, i], &locs)
        .expect_err("a mut result overlapping another result must not be representable");
    assert!(err.contains("missing edge"), "{err}");
    // ... and the practical consequence: the write through `m` is allowed.
    assert!(is_writable(&graph, m));
}

// -------------------------------------------------------------------------------------------------
// GraphMap: node index generation and reuse
// -------------------------------------------------------------------------------------------------

/// `minimize()` recomputes `next` from the surviving nodes, so it can hand the same `id` out
/// twice. Only the generation bump keeps the two `NodeIndex` values distinct. Hammer
/// add/remove/minimize/clear and assert no index is ever issued twice while both are live, and
/// that a stale index never resolves to a live node.
#[test]
fn graph_map_never_issues_a_colliding_node_index() {
    use crate::graph_map::{GraphMap, NodeIndex};

    let mut g: GraphMap<u32, u32> = GraphMap::new(32);
    let mut live: BTreeSet<NodeIndex> = BTreeSet::new();
    let mut ever_issued: BTreeSet<NodeIndex> = BTreeSet::new();
    let mut counter = 0u32;
    // A fixed script that exercises the id-reuse window: fill, drop the top, minimize, refill.
    let script: &[&str] = &[
        "add", "add", "add", "add", "drop_max", "drop_max", "minimize", "add", "add", "drop_min",
        "minimize", "add", "add", "add", "clear", "add", "add", "minimize", "add", "drop_max",
        "minimize", "add", "add",
    ];
    for step in script {
        match *step {
            "add" => {
                counter += 1;
                let idx = g.add_node(counter).unwrap();
                assert!(
                    live.insert(idx),
                    "add_node handed out {idx:?} while it was still live"
                );
                assert!(
                    ever_issued.insert(idx),
                    "add_node reissued {idx:?} after it had been retired"
                );
                for other in &live {
                    if *other != idx {
                        g.add_edge(*other, counter, idx).unwrap();
                    }
                }
            }
            "drop_max" | "drop_min" => {
                let victim = if *step == "drop_max" {
                    live.iter().next_back().copied()
                } else {
                    live.iter().next().copied()
                };
                let Some(victim) = victim else { continue };
                g.remove_node(victim).unwrap();
                live.remove(&victim);
                assert!(!g.contains_node(victim));
            }
            "minimize" => g.minimize().unwrap(),
            "clear" => {
                g.clear().unwrap();
                for stale in &live {
                    assert!(!g.contains_node(*stale), "{stale:?} survived clear()");
                }
                live.clear();
            }
            other => unreachable!("{other}"),
        }
        g.check_invariants();
        assert_eq!(g.node_count(), live.len());
        for idx in &live {
            assert!(g.contains_node(*idx));
        }
        for idx in ever_issued.difference(&live) {
            assert!(!g.contains_node(*idx), "retired {idx:?} resolves to a node");
        }
    }
}

/// `minimize()` really does lower `next`, so the raw `id` is reused. Pin that the generation is
/// what keeps the indices apart -- if the generation bump were dropped, this would start
/// returning equal indices and every stale `NodeIndex` in the verifier would alias a live node.
#[test]
fn minimize_reuses_ids_but_not_indices() {
    use crate::graph_map::GraphMap;

    let mut g: GraphMap<u32, u32> = GraphMap::new(8);
    let a = g.add_node(1).unwrap();
    let b = g.add_node(2).unwrap();
    g.remove_node(b).unwrap();
    g.minimize().unwrap();
    let c = g.add_node(3).unwrap();
    assert_ne!(b, c, "minimize reused a NodeIndex verbatim");
    assert!(!g.contains_node(b));
    assert!(g.contains_node(a));
    assert!(g.contains_node(c));
    assert_eq!(g.node_weight(c), Some(&3));
}
