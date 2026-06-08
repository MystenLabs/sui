// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! A thin wrapper over `move-regex-borrow-graph` that mirrors the borrow tracking performed by the
//! bytecode verifier's `regex_reference_safety` pass. The generator threads one of these through
//! its `AbstractState` so that reference-using bytecode is memory-safe by construction.
//!
//! References are intra-block in the generator: a basic block is generated until its abstract stack
//! drains to empty, and no local/parameter/field is reference-typed (`references_allowed: false` in
//! module generation), so every reference lives transiently on the stack and is released before the
//! block ends. As a result we never canonicalize or join graphs across blocks (for now! this is
//! coming soon).

use move_binary_format::file_format::FieldHandleIndex;
use move_regex_borrow_graph::{collections::Graph, meter::DummyMeter};
use std::fmt;

pub use move_regex_borrow_graph::references::Ref;

pub type LocalIndex = usize;

/// A borrow-path label, mirroring `regex_reference_safety::Label`. `Local(i)` roots a borrow at
/// local `i`; `Field(f)` extends a struct reference by one field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Label {
    Local(LocalIndex),
    Field(FieldHandleIndex),
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Label::Local(i) => write!(f, "local#{i}"),
            Label::Field(i) => write!(f, "field#{}", i.0),
        }
    }
}

/// Upper bound (exclusive on the crate side) for the canonical-reference capacity passed to
/// `Graph::new`. `move-regex-borrow-graph`'s `GraphMap::new` asserts `capacity < 512`, so we stay one
/// below that. The capacity is only a sizing hint -- the graph grows past it as needed, and Phase A
/// never canonicalizes -- so clamping to this value is safe.
const MAX_CANONICAL_REFERENCE_CAPACITY: usize = 511;

/// The borrow graph for a single function body. `local_root` is the synthetic mutable reference all
/// local borrows extend from (as in the verifier).
#[derive(Clone, Debug)]
pub struct BorrowGraph {
    graph: Graph<(), Label>,
    local_root: Ref,
}

impl BorrowGraph {
    /// Construct an empty graph for a function with `num_locals` locals. No parameter references are
    /// seeded (Phase A has no reference parameters). The capacity is only a sizing hint for the
    /// underlying graph and is bounded by the crate's internal assertion.
    pub fn new(num_locals: usize) -> Self {
        // `+ 1` reserves a slot for the synthetic `local_root` reference created just below.
        let capacity = (num_locals + 1).min(MAX_CANONICAL_REFERENCE_CAPACITY);
        let (mut graph, _map) = Graph::new(capacity, std::iter::empty::<(usize, (), bool)>())
            .expect("empty borrow graph construction cannot fail");
        let local_root = graph
            .extend_by_epsilon(
                (),
                std::iter::empty::<Ref>(),
                /* is_mut */ true,
                &mut DummyMeter,
            )
            .expect("local root creation cannot fail");
        Self { graph, local_root }
    }

    /// A fresh, standalone reference borrowing nothing. Only supports the `push_fresh_reference`
    /// test helper; real borrows go through [`Self::borrow_loc`] etc.
    pub(crate) fn fresh_reference(&mut self, is_mut: bool) -> Result<Ref, String> {
        self.graph
            .extend_by_epsilon((), std::iter::empty::<Ref>(), is_mut, &mut DummyMeter)
            .map_err(|e| format!("fresh_reference failed: {e:?}"))
    }

    /// `ImmBorrowLoc`/`MutBorrowLoc`: a new reference rooted at `local`.
    pub fn borrow_loc(&mut self, local: LocalIndex, is_mut: bool) -> Result<Ref, String> {
        self.graph
            .extend_by_label(
                (),
                [self.local_root],
                is_mut,
                Label::Local(local),
                &mut DummyMeter,
            )
            .map_err(|e| format!("borrow_loc failed: {e:?}"))
    }

    /// `ImmBorrowField`/`MutBorrowField`: extend `r` by `field`, consuming `r`.
    pub fn borrow_field(
        &mut self,
        r: Ref,
        is_mut: bool,
        field: FieldHandleIndex,
    ) -> Result<Ref, String> {
        let new_r = self
            .graph
            .extend_by_label((), [r], is_mut, Label::Field(field), &mut DummyMeter)
            .map_err(|e| format!("borrow_field failed: {e:?}"))?;
        self.release(r)?;
        Ok(new_r)
    }

    /// `FreezeRef`: a new immutable reference aliasing the mutable `r`, consuming `r`.
    pub fn freeze(&mut self, r: Ref) -> Result<Ref, String> {
        let frozen = self
            .graph
            .extend_by_epsilon((), [r], /* is_mut */ false, &mut DummyMeter)
            .map_err(|e| format!("freeze failed: {e:?}"))?;
        self.release(r)?;
        Ok(frozen)
    }

    /// Release a reference (`Pop`, `ReadRef`, `WriteRef`, and the consumed source of a field
    /// borrow/freeze).
    pub fn release(&mut self, r: Ref) -> Result<(), String> {
        self.graph
            .release(r, &mut DummyMeter)
            .map_err(|e| format!("release failed: {e:?}"))
    }

    pub fn is_mutable(&self, r: Ref) -> bool {
        self.graph.is_mutable(r).unwrap_or(false)
    }

    /// `WriteRef` precondition: `r` is mutable and has no outstanding non-epsilon borrowers.
    pub fn is_writable(&self, r: Ref) -> bool {
        self.is_mutable(r)
            && match self.graph.borrowed_by(r, &mut DummyMeter) {
                Ok(borrowers) => borrowers
                    .values()
                    .all(|paths| paths.iter().all(|p| p.is_epsilon())),
                Err(_) => false,
            }
    }

    /// Whether `local` is borrowed. With `exclude_alias` only proper extensions (e.g. field borrows)
    /// count, matching the verifier's `StLoc` check; otherwise any borrow counts (`MoveLoc`).
    pub fn is_local_borrowed(&self, local: LocalIndex, exclude_alias: bool) -> bool {
        let lbl = Label::Local(local);
        match self.graph.borrowed_by(self.local_root, &mut DummyMeter) {
            Ok(borrowers) => borrowers.values().flatten().any(|p| {
                if exclude_alias {
                    p.starts_with(&lbl) && !p.is_label(&lbl)
                } else {
                    p.starts_with(&lbl)
                }
            }),
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_mutable_reference_is_writable_immutable_is_not() {
        let mut g = BorrowGraph::new(1);
        let m = g.borrow_loc(0, true).unwrap();
        assert!(g.is_writable(m), "a fresh mutable borrow has no borrowers");
        let i = g.borrow_loc(0, false).unwrap();
        assert!(!g.is_writable(i), "immutable references are never writable");
    }

    #[test]
    fn borrowing_a_local_blocks_moving_but_a_direct_alias_does_not_block_overwriting() {
        let mut g = BorrowGraph::new(1);
        assert!(!g.is_local_borrowed(0, false));
        let r = g.borrow_loc(0, true).unwrap();
        // A direct `&local` borrow blocks `MoveLoc` (any borrow) but not `StLoc` (proper
        // extensions only).
        assert!(g.is_local_borrowed(0, false));
        assert!(!g.is_local_borrowed(0, true));
        g.release(r).unwrap();
        assert!(!g.is_local_borrowed(0, false));
    }

    #[test]
    fn a_field_borrow_blocks_overwriting_the_local() {
        let mut g = BorrowGraph::new(1);
        let r = g.borrow_loc(0, true).unwrap();
        // `borrow_field` consumes `r`; the resulting field reference still extends `local#0` via a
        // proper (`Local . Field`) path, so it blocks both `MoveLoc` and `StLoc`.
        let _f = g.borrow_field(r, true, FieldHandleIndex(0)).unwrap();
        assert!(g.is_local_borrowed(0, false));
        assert!(g.is_local_borrowed(0, true));
    }
}
