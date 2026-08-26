// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Tests the borrow graph against a concrete model of memory.
//!
//! The other test modules check algebraic identities of `Regex`, or check a single graph
//! extension against the relations that existed just before it. These tests instead carry a
//! concrete model alongside the abstract graph. Every live reference is given an actual memory
//! location, a disjoint tree root plus a path within it, and every operation updates both.
//!
//! The property under test is the closure claim the bytecode verifier relies on. For every pair
//! of live references `x` and `y` where the concrete state has `loc(y) = loc(x) ++ w`, the direct
//! edge `x --> y` must contain a regex matching `w`. `is_writable`, `are_transferrable` and
//! `is_local_borrowed` all consult only the direct successors from `borrowed_by`, so if this ever
//! fails they silently miss a hazard.
//!
//! The generated histories are restricted to the states the verifier can actually reach. A `&mut`
//! is only ever derived from a `&mut`. A call is only performed when `are_transferrable` holds
//! for its arguments, mirroring `regex_reference_safety::abstract_state`. A mutable call result
//! extends a mutable argument and is disjoint from every other result of the same call, which is
//! what the callee's `Ret` check enforces.

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
// The longest word `witness_paths` generates. Differences deeper than this go unseen.
const WITNESS_LEN: usize = 4;
const GRAPH_CAPACITY: usize = 64;

// -------------------------------------------------------------------------------------------------
// Concrete locations
// -------------------------------------------------------------------------------------------------

/// A concrete memory location. A disjoint tree root, and the path within it.
///
/// This assumes distinct labels name disjoint memory. That holds for locals and struct fields,
/// but not for enum variant fields, where `VariantField(E, One, 0)` and `VariantField(E, Two, 0)`
/// are distinct labels over the same bytes. The overlap is a modelling choice in
/// `regex_reference_safety` rather than anything the graph can see, so it is covered by the
/// transactional test `reference_safety/enum_type_confusion_attempts.mvir`.
#[derive(Clone, PartialEq, Eq, Debug)]
struct ConcreteLocation {
    root: usize,
    path: Vec<char>,
}

impl ConcreteLocation {
    /// `Some(w)` when `prefix ++ w == self`.
    fn strip_prefix(&self, prefix: &ConcreteLocation) -> Option<&[char]> {
        if self.root != prefix.root {
            return None;
        }
        self.path.strip_prefix(&prefix.path[..])
    }

    /// Returns true when one location is a prefix of the other, i.e. the two overlap in memory.
    fn overlaps(&self, other: &ConcreteLocation) -> bool {
        self.strip_prefix(other).is_some() || other.strip_prefix(self).is_some()
    }

    fn extended(&self, suffix: &[char]) -> ConcreteLocation {
        ConcreteLocation {
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

/// A subset of all possible words in the language of ALPHABET. Each word is at most WITNESS_LEN
/// characters long. Used to compare two regexes by the words they match.
fn witness_paths() -> Vec<Vec<char>> {
    // Each generation is every word one character longer than the previous generation's.
    std::iter::successors(Some(vec![vec![]]), |boundary| {
        let next = boundary
            .iter()
            .flat_map(|word| {
                ALPHABET.map(|c| {
                    let mut longer = word.clone();
                    longer.push(c);
                    longer
                })
            })
            .collect();
        Some(next)
    })
    .take(WITNESS_LEN + 1)
    .flatten()
    .collect()
}

// -------------------------------------------------------------------------------------------------
// Verifier predicates, mirrored
// -------------------------------------------------------------------------------------------------

/// The Ref is writable if it has no non-epsilon extensions.
/// Mirrors `regex_reference_safety::abstract_state::AbstractState::is_writable`.
fn is_writable(graph: &G, r: Ref) -> bool {
    graph.is_mutable(r).unwrap()
        && graph
            .borrowed_by(r, &mut DummyMeter)
            .unwrap()
            .values()
            .all(|paths| paths.iter().all(|path| path.is_epsilon()))
}

/// "Transferrable"  refers to the set of references leaving their current scope. That is either as
/// a return values or arguments to a function call.
/// Pessimistically, we assume that any mutable reference will be written to on all possible
/// extensions. As such, any mutable reference that is transferred cannot have any non-epsilon
/// extensions and cannot be reachable from any other reference that is also being transferred.
/// Mirrors `regex_reference_safety::abstract_state::AbstractState::are_transferrable`.
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

/// The model tracks mutability independently of the graph.
/// This checks that both agree.
fn check_mutability_agrees(graph: &G, live: &[LiveRef]) -> Result<(), String> {
    for x in live {
        let in_graph = graph.is_mutable(x.r).unwrap();
        if in_graph != x.is_mut {
            return Err(format!(
                "{} is_mut = {} in the model but {} in the graph",
                x.r, x.is_mut, in_graph
            ));
        }
    }
    Ok(())
}

/// Checks that the graph is complete against the concrete model.
/// Every concrete location for each reference must be covered in the graph.
fn check_complete(graph: &G, live: &[LiveRef]) -> Result<(), String> {
    check_mutability_agrees(graph, live)?;
    for x in live {
        let borrowed = graph.borrowed_by(x.r, &mut DummyMeter).unwrap();
        for y in live {
            if x.r == y.r {
                continue;
            }
            let Some(w) = y.loc.strip_prefix(&x.loc) else {
                continue;
            };
            let covered = borrowed
                .get(&y.r)
                .is_some_and(|paths| paths.iter().any(|p| path_matches(p, w)));
            if !covered {
                return Err(format!(
                    "missing edge {} --{:?}--> {} (locs {:?} / {:?}); borrowed_by({}) = {:?}",
                    x.r,
                    w,
                    y.r,
                    x.loc,
                    y.loc,
                    x.r,
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

/// A live reference, paired with the mutability and concrete location the model tracks for it.
#[derive(Clone, Debug)]
struct LiveRef {
    r: Ref,
    is_mut: bool,
    loc: ConcreteLocation,
}

struct Model {
    graph: G,
    live: Vec<LiveRef>,
    next_root: usize,
}

impl Model {
    fn new() -> Self {
        let (graph, _) = Graph::<(), char>::new(GRAPH_CAPACITY, Vec::<(u8, (), bool)>::new())
            .expect("empty graph");
        Model {
            graph,
            live: vec![],
            next_root: 0,
        }
    }

    fn len(&self) -> usize {
        self.live.len()
    }

    fn record(&mut self, r: Ref, is_mut: bool, loc: ConcreteLocation) {
        self.live.push(LiveRef { r, is_mut, loc });
    }

    fn release(&mut self, idx: usize) {
        self.graph
            .release(self.live[idx].r, &mut DummyMeter)
            .unwrap();
        self.live.remove(idx);
    }

    fn new_root(&mut self, is_mut: bool) {
        let r = self
            .graph
            .extend_by_epsilon((), std::iter::empty(), is_mut, &mut DummyMeter)
            .unwrap();
        let root = self.next_root;
        self.next_root += 1;
        self.record(r, is_mut, ConcreteLocation { root, path: vec![] });
    }

    fn alias(&mut self, base: usize, is_mut: bool) {
        let is_mut = is_mut && self.live[base].is_mut;
        let r = self
            .graph
            .extend_by_epsilon(
                (),
                std::iter::once(self.live[base].r),
                is_mut,
                &mut DummyMeter,
            )
            .unwrap();
        let loc = self.live[base].loc.clone();
        self.record(r, is_mut, loc);
    }

    fn field(&mut self, base: usize, label: char, is_mut: bool) {
        let is_mut = is_mut && self.live[base].is_mut;
        let r = self
            .graph
            .extend_by_label(
                (),
                std::iter::once(self.live[base].r),
                is_mut,
                label,
                &mut DummyMeter,
            )
            .unwrap();
        let loc = self.live[base].loc.extended(&[label]);
        self.record(r, is_mut, loc);
    }

    /// Returns false when the call is not performed (the verifier would have rejected it, or
    /// there is nothing to model).
    fn call(&mut self, arg_idxs: &BTreeSet<usize>, results: &[(bool, usize, Vec<char>)]) -> bool {
        if arg_idxs.is_empty() {
            return false;
        }
        let arg_refs = arg_idxs
            .iter()
            .map(|&i| self.live[i].r)
            .collect::<BTreeSet<_>>();
        if !are_transferrable(&self.graph, &arg_refs) {
            return false;
        }
        let mut_arg_idxs = arg_idxs
            .iter()
            .copied()
            .filter(|&i| self.live[i].is_mut)
            .collect::<Vec<_>>();
        let arg_idx_vec = arg_idxs.iter().copied().collect::<Vec<_>>();

        // Pick concrete locations for the results, dropping any result that would violate the
        // disjointness the callee's `Ret` check guarantees.
        let mut kept: Vec<(bool, ConcreteLocation)> = vec![];
        for (want_mut, src_choice, suffix) in results {
            let (is_mut, src) = if *want_mut && !mut_arg_idxs.is_empty() {
                (true, mut_arg_idxs[src_choice % mut_arg_idxs.len()])
            } else {
                (false, arg_idx_vec[src_choice % arg_idx_vec.len()])
            };
            let loc = self.live[src].loc.extended(suffix);
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
            self.record(r, is_mut, loc);
        }
        // `AbstractState::call` releases the argument references after the call.
        for idx in arg_idxs.iter().rev() {
            self.release(*idx);
        }
        true
    }

    /// Every concrete containment between two live references must be covered by a direct edge.
    fn check_complete(&self) -> Result<(), String> {
        check_complete(&self.graph, &self.live)
    }

    /// A corollary of `check_complete`, checked on its own so a failure points at the right rule.
    /// A reference must not be writable while another reference lives strictly inside it.
    fn check_writability(&self) -> Result<(), String> {
        for x in &self.live {
            if !x.is_mut || !is_writable(&self.graph, x.r) {
                continue;
            }
            for y in &self.live {
                if x.r == y.r {
                    continue;
                }
                if y.loc.strip_prefix(&x.loc).is_some_and(|w| !w.is_empty()) {
                    return Err(format!(
                        "{} is writable but {} lives strictly inside it ({:?} / {:?})",
                        x.r, y.r, x.loc, y.loc
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

impl Model {
    fn apply(&mut self, op: &RawOp) {
        match op {
            RawOp::Root(is_mut) => self.new_root(*is_mut),
            RawOp::Alias(base, is_mut) => {
                if self.len() == 0 {
                    return;
                }
                self.alias(*base as usize % self.len(), *is_mut)
            }
            RawOp::Field(base, label, is_mut) => {
                if self.len() == 0 {
                    return;
                }
                let label = ALPHABET[*label as usize % ALPHABET.len()];
                self.field(*base as usize % self.len(), label, *is_mut)
            }
            RawOp::Release(idx) => {
                if self.len() <= 1 {
                    return;
                }
                self.release(*idx as usize % self.len())
            }
            RawOp::Call(args, results) => {
                if self.len() == 0 {
                    return;
                }
                let arg_idxs = args
                    .iter()
                    .map(|a| *a as usize % self.len())
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
                self.call(&arg_idxs, &results);
            }
        }
    }
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
        model.apply(op);
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
            out.entry((x, y)).or_default().extend(matched);
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
            model.apply(op);
            let complete = model.check_complete();
            prop_assert!(complete.is_ok(), "{}", complete.unwrap_err());
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
        // An early return rather than `prop_assume!`. Raising the case count would otherwise
        // trip proptest's global-reject limit.
        if model.len() < 2 {
            return Ok(());
        }
        let victim = victim as usize % model.len();
        let victim_ref = model.live[victim].r;
        let survivors = model
            .live
            .iter()
            .map(|l| l.r)
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

    /// Every relation reachable in two hops must be covered by one direct edge. Calls are
    /// excluded. The dot-star edges a call installs between its immutable results only mark that
    /// we do not know how the two overlap, and do not claim any concrete two-hop path exists.
    /// `graph_is_complete_against_the_concrete_model` is what pins down soundness for calls.
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
        let refs = model.live.iter().map(|l| l.r).collect::<Vec<_>>();
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
        .live
        .iter()
        .map(|l| l.r)
        .filter(|r| !keep_set.contains(r))
        .collect::<Vec<_>>();
    for r in doomed {
        let idx = model.live.iter().position(|l| l.r == r).unwrap();
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
    for l in &mut model.live {
        l.r = l.r.canonicalize(&remapping).unwrap();
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
        let keep = base.live.iter().map(|l| l.r).collect::<Vec<_>>();

        let build = |extra: &[RawOp]| {
            let mut model = Model {
                graph: base.graph.clone(),
                live: base.live.clone(),
                next_root: base.next_root,
            };
            for op in extra {
                if model.len() >= GRAPH_CAPACITY / 2 { break }
                // Releases and calls would drop references we need to keep on both sides.
                if matches!(op, RawOp::Release(_) | RawOp::Call(..)) {
                    continue;
                }
                model.apply(op);
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

/// Unlike the two `remove_prefix` functions, `Regex::extend` has no example table in its doc
/// comment. These are the cases that comment describes in prose, including the short-circuit.
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
        // Once the regex ends in dot-star the extension is dropped.
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

/// Dropping labels must only ever widen the language. `p.*` extended by `l` denotes `p.*l`, and
/// the implementation returns `p.*`, which contains it.
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

/// `Walk::next` is a no-op once the walk sits on a dot-star, so the `Extension::DotStar` arm of
/// `Regex::remove_prefix` would loop forever without its `ends_in_dot_star` early return. If that
/// early return is ever removed this test hangs rather than quietly changing the answer.
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
/// arms. An empty set means no relation at all, which is an under-approximation, so those arms
/// must be unreachable. Every reachable input either produces a non-empty answer or is one of the
/// genuinely-no-prefix cases enumerated in the doc comments. This walks every shape that can
/// reach the walk.
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
                (Extension::Label(l), Some(first)) => l != first || r.labels.len() > 1,
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
/// directions between them. That leaves the abstract relation non-transitive, since following
/// `n1 --.*--> n2 --.*--> n1` yields dot-star while the self edge is epsilon. It is sound only
/// because no concrete state has `n2` strictly inside `n1` and `n1` strictly inside `n2` at once.
/// Pinned here so a change to `extend_by_dot_star_for_call` is visible.
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
    // Both results are immutable so writability does not apply to them, but the parent is not
    // writable, since the results may be strictly inside it.
    assert!(!is_writable(&graph, root));
}

/// The mutable half of the same operation. Mutable results get edges only from mutable sources,
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
/// intermediate reference is released. Nothing may be lost.
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

/// One step applied to both branches. The shape, alias or field, and the mutability are shared,
/// so both branches end up with the same reference set and the same mutabilities--`Graph::join`
/// requires that. The base and the label are chosen independently, so the same canonical
/// reference lands at a different concrete location on each branch, which is what happens at a
/// real control-flow merge.
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
    live: Vec<LiveRef>,
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
            live: vec![LiveRef {
                r: root,
                is_mut: true,
                loc: ConcreteLocation {
                    root: 0,
                    path: vec![],
                },
            }],
        }
    }

    fn step(&mut self, is_field: bool, is_mut: bool, base_choice: u8, label_choice: u8) {
        // A `&mut` needs a `&mut` base; there is always at least the root.
        let candidates = if is_mut {
            (0..self.live.len())
                .filter(|&i| self.live[i].is_mut)
                .collect::<Vec<_>>()
        } else {
            (0..self.live.len()).collect::<Vec<_>>()
        };
        let base = candidates[base_choice as usize % candidates.len()];
        let label = ALPHABET[label_choice as usize % ALPHABET.len()];
        let (new_ref, loc) = if is_field {
            (
                self.graph
                    .extend_by_label(
                        (),
                        std::iter::once(self.live[base].r),
                        is_mut,
                        label,
                        &mut DummyMeter,
                    )
                    .unwrap(),
                self.live[base].loc.extended(&[label]),
            )
        } else {
            (
                self.graph
                    .extend_by_epsilon(
                        (),
                        std::iter::once(self.live[base].r),
                        is_mut,
                        &mut DummyMeter,
                    )
                    .unwrap(),
                self.live[base].loc.clone(),
            )
        };
        self.live.push(LiveRef {
            r: new_ref,
            is_mut,
            loc,
        });
    }

    fn canonicalize(&mut self) {
        let remapping = self
            .live
            .iter()
            .enumerate()
            .map(|(i, l)| (l.r, i as u32))
            .collect::<BTreeMap<_, _>>();
        for l in &mut self.live {
            l.r = l.r.canonicalize(&remapping).unwrap();
        }
        self.graph.canonicalize(&remapping).unwrap();
    }

    /// The same references and mutabilities, moved onto a different concrete state. Lets a
    /// joined graph be checked against the other branch's locations.
    fn relocated(&self, locs: &[ConcreteLocation]) -> Vec<LiveRef> {
        self.live
            .iter()
            .zip(locs)
            .map(|(l, loc)| LiveRef {
                r: l.r,
                is_mut: l.is_mut,
                loc: loc.clone(),
            })
            .collect()
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 384, ..ProptestConfig::default() })]

    /// After a join the graph describes one concrete state per incoming branch. It must stay
    /// complete for every one of them, and must stay complete as the block goes on extending it.
    /// This is the loop and if-else fixpoint soundness argument.
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
        let signature = |b: &Branch| b.live.iter().map(|l| (l.r, l.is_mut)).collect::<Vec<_>>();
        prop_assert_eq!(signature(&left), signature(&right));

        // Each branch on its own must be complete for its own concrete state.
        prop_assert!(
            check_complete(&left.graph, &left.live).is_ok(),
            "{}",
            check_complete(&left.graph, &left.live).unwrap_err()
        );

        left.graph.join(&right.graph, &mut DummyMeter).unwrap();

        // The joined graph must be complete for both. `right.live` holds the same reference
        // sequence carrying the other branch's locations.
        for (which, live) in [("left", &left.live), ("right", &right.live)] {
            let res = check_complete(&left.graph, live);
            prop_assert!(res.is_ok(), "after join, {} state: {}", which, res.unwrap_err());
        }

        // Continue the block. Both concrete states evolve and the one joined graph must track
        // both. Mutabilities are shared so the two states stay comparable.
        let mut joined = Branch {
            graph: left.graph,
            live: left.live.clone(),
        };
        let mut shadow_locs = right.live.iter().map(|l| l.loc.clone()).collect::<Vec<_>>();
        joined.graph.refresh_refs().unwrap();
        for l in &mut joined.live {
            l.r = l.r.refresh().unwrap();
        }

        for t in &after {
            let candidates = if t.is_mut {
                (0..joined.live.len()).filter(|&i| joined.live[i].is_mut).collect::<Vec<_>>()
            } else {
                (0..joined.live.len()).collect::<Vec<_>>()
            };
            let base = candidates[t.base_a as usize % candidates.len()];
            let label = ALPHABET[t.label_a as usize % ALPHABET.len()];
            let new_ref = if t.is_field {
                joined.graph
                    .extend_by_label((), std::iter::once(joined.live[base].r), t.is_mut, label, &mut DummyMeter)
                    .unwrap()
            } else {
                joined.graph
                    .extend_by_epsilon((), std::iter::once(joined.live[base].r), t.is_mut, &mut DummyMeter)
                    .unwrap()
            };
            let (loc, shadow_loc) = if t.is_field {
                (joined.live[base].loc.extended(&[label]), shadow_locs[base].extended(&[label]))
            } else {
                (joined.live[base].loc.clone(), shadow_locs[base].clone())
            };
            joined.live.push(LiveRef { r: new_ref, is_mut: t.is_mut, loc });
            shadow_locs.push(shadow_loc);
            let shadow = joined.relocated(&shadow_locs);
            for (which, live) in [("left", &joined.live), ("right", &shadow)] {
                let res = check_complete(&joined.graph, live);
                prop_assert!(res.is_ok(), "after join+extend, {} state: {}", which, res.unwrap_err());
            }
        }

        // Finally a one-argument call. The callee can return a reference at a different concrete
        // offset on each branch, and the one dot-star edge has to cover both.
        let arg = call_arg as usize % joined.live.len();
        let arg_set = BTreeSet::from([joined.live[arg].r]);
        if are_transferrable(&joined.graph, &arg_set) {
            let is_mut = call_mut && joined.live[arg].is_mut;
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
            let loc = joined.live[arg].loc.extended(&suffix_a);
            joined.live.push(LiveRef { r: result, is_mut, loc });
            shadow_locs.push(shadow_locs[arg].extended(&suffix_b));
            // `call` releases the argument.
            joined.graph.release(joined.live[arg].r, &mut DummyMeter).unwrap();
            joined.live.remove(arg);
            shadow_locs.remove(arg);
            let shadow = joined.relocated(&shadow_locs);
            for (which, live) in [("left", &joined.live), ("right", &shadow)] {
                let res = check_complete(&joined.graph, live);
                prop_assert!(res.is_ok(), "after join+call, {} state: {}", which, res.unwrap_err());
            }
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Canonicalization hazards
// -------------------------------------------------------------------------------------------------

/// A remapping that sends two distinct references to the same canonical id would collapse them
/// into one entry in `Graph::nodes` while both nodes, and their edges, stayed in the `GraphMap`,
/// and `borrowed_by` and `borrows_from` would then lose edges. `canonicalize` rejects it by name
/// before touching anything, so a release build gets an `InvariantViolation` rather than a graph
/// that quietly drops edges.
///
/// The one production caller, `regex_reference_safety::AbstractState::canonicalize`, builds an
/// injective map of `local_root -> 0` and `local i -> i + 1`, so this never fires in practice.
#[test]
#[should_panic(expected = "remapping is not injective")]
fn canonicalize_rejects_a_non_injective_remapping() {
    let (mut graph, refs) = Graph::<(), char>::new(4, [(0, (), true), (1, (), true)]).unwrap();
    let remapping = BTreeMap::from([(refs[&0], 0u32), (refs[&1], 0u32)]);
    let _ = graph.canonicalize(&remapping);
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

    let refs = |b: &Branch| b.live.iter().map(|l| l.r).collect::<Vec<_>>();
    let before = relation(&branch.graph, &refs(&branch));
    branch.graph.refresh_refs().unwrap();
    for l in &mut branch.live {
        l.r = l.r.refresh().unwrap();
    }
    branch.canonicalize();
    let after = relation(&branch.graph, &refs(&branch));

    for &x in &refs(&branch) {
        for &y in &refs(&branch) {
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

    /// `determine_all_new_edges` derives `new --> x` only from the outgoing edges of a source.
    /// So for every reference `x` that concretely aliases a source, the graph must hold the edge
    /// in the source-to-`x` direction as well, not just `x`-to-source. The edge-derivation rules
    /// depend on exactly this consequence of completeness, so it is checked on its own and a
    /// regression points straight at the rule that broke.
    ///
    /// Note this is a claim about concretely aliasing references. It is deliberately not the
    /// stronger claim that a path matching the empty word implies a reverse edge. A dot-star edge
    /// matches the empty word without asserting an alias, and `Path::is_epsilon()`, which is what
    /// `is_writable` and `are_transferrable` consult, treats dot-star as a strict extension.
    #[test]
    fn concrete_aliases_have_edges_in_both_directions(
        ops in prop::collection::vec(arb_raw_op(), 0..=14),
    ) {
        let model = run_history(&ops);
        for i in 0..model.len() {
            for j in (i + 1)..model.len() {
                if model.live[i].loc != model.live[j].loc {
                    continue;
                }
                for (from, to) in [(i, j), (j, i)] {
                    let succ = model.graph.borrowed_by(model.live[from].r, &mut DummyMeter).unwrap();
                    prop_assert!(
                        succ.get(&model.live[to].r)
                            .is_some_and(|ps| ps.iter().any(|p| p.labels.is_empty())),
                        "aliases {} and {} (both at {:?}) have no {} --> {} edge",
                        model.live[i].r,
                        model.live[j].r,
                        model.live[i].loc,
                        model.live[from].r,
                        model.live[to].r,
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
        for x in model.live.iter().map(|l| l.r) {
            let succ = model.graph.borrowed_by(x, &mut DummyMeter).unwrap();
            prop_assert!(!succ.contains_key(&x), "{} has a non-trivial self edge", x);
            for paths in succ.values() {
                prop_assert!(!paths.is_empty());
            }
        }
    }
}

/// `extend_by_dot_star_for_call` ties a mutable result only to the mutable sources. Given no
/// mutable source at all it produces a reference with no incoming edges, one the graph believes
/// is unrelated to everything, and writing through it or through anything else is then
/// unconstrained. The bytecode verifier never asks for this, since a callee cannot manufacture a
/// `&mut` from an `&`, but nothing in this crate rules it out, so the shape is pinned here.
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
        "mutable result unexpectedly has incoming edges"
    );
    assert!(
        graph.borrowed_by(imm, &mut DummyMeter).unwrap().is_empty(),
        "immutable source unexpectedly has an edge to the mutable result"
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

/// The reference a call returns is recorded as `arg --.*--> result`, and dot-star is not
/// `Path::is_epsilon()`. So an alias of the argument that survives the call is reported as
/// strictly borrowed even when the result is concretely the very same location. That errs in the
/// conservative direction and allows fewer writes, but it costs expressivity, so pin it.
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
    // The dot-star language contains the empty word, so `result` may concretely be `alias`, yet
    // `alias` is still reported as not writable.
    assert!(!is_writable(&graph, alias));
}

/// `Regex::extend` short-circuits on dot-star, so `x --.*--> y` extended by a label stays `.*`
/// rather than becoming `.*f`. The result over-approximates, since it now also matches the empty
/// word, which is why `x --> z` below carries both `.*` and the exact `a`.
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

/// `extend_by_dot_star_for_call` installs no edge at all between a mutable result and any other
/// result of the same call. Nothing in this crate checks that, so the graph is complete only
/// because the callee's own `Ret` check, `are_transferrable`, refuses to return a `&mut` that
/// overlaps any other returned reference.
///
/// This test exhibits the state that would exist if that guarantee were ever weakened. A mutable
/// result `m` sits at `root.a` and an immutable result `i` at `root.a.b`, with no edge
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
    let live = [
        LiveRef {
            r: m,
            is_mut: true,
            loc: ConcreteLocation {
                root: 0,
                path: vec!['a'],
            },
        },
        LiveRef {
            r: i,
            is_mut: false,
            loc: ConcreteLocation {
                root: 0,
                path: vec!['a', 'b'],
            },
        },
    ];
    let err = check_complete(&graph, &live)
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

    #[derive(Clone, Copy)]
    enum Step {
        Add,
        Clear,
        DropMax,
        DropMin,
        Minimize,
    }

    let mut g: GraphMap<u32, u32> = GraphMap::new(32);
    let mut live: BTreeSet<NodeIndex> = BTreeSet::new();
    let mut ever_issued: BTreeSet<NodeIndex> = BTreeSet::new();
    let mut counter = 0u32;
    // A fixed script that exercises the id-reuse window: fill, drop the top, minimize, refill.
    let script = [
        Step::Add,
        Step::Add,
        Step::Add,
        Step::Add,
        Step::DropMax,
        Step::DropMax,
        Step::Minimize,
        Step::Add,
        Step::Add,
        Step::DropMin,
        Step::Minimize,
        Step::Add,
        Step::Add,
        Step::Add,
        Step::Clear,
        Step::Add,
        Step::Add,
        Step::Minimize,
        Step::Add,
        Step::DropMax,
        Step::Minimize,
        Step::Add,
        Step::Add,
    ];
    for step in script {
        match step {
            Step::Add => {
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
            Step::DropMax | Step::DropMin => {
                let victim = match step {
                    Step::DropMax => live.iter().next_back().copied(),
                    Step::DropMin => live.iter().next().copied(),
                    _ => unreachable!(),
                };
                let Some(victim) = victim else { continue };
                g.remove_node(victim).unwrap();
                live.remove(&victim);
                assert!(!g.contains_node(victim));
            }
            Step::Minimize => g.minimize().unwrap(),
            Step::Clear => {
                g.clear().unwrap();
                for stale in &live {
                    assert!(!g.contains_node(*stale), "{stale:?} survived clear()");
                }
                live.clear();
            }
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

/// `minimize()` really does lower `next`, so the raw `id` is reused. The generation is what keeps
/// the indices apart. Drop the generation bump and this starts returning equal indices, and every
/// stale `NodeIndex` in the verifier would alias a live node.
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
