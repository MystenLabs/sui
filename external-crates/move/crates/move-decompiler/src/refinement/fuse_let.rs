// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use crate::{ast::Exp, refinement::Refine};

pub fn refine(exp: &mut Exp) -> bool {
    FuseLet.refine(exp)
}

// -------------------------------------------------------------------------------------------------
// Refinement
//
// `hoist_declarations` and `hoist_arm_assignments` together produce sequences of the shape
//
//     let X;
//     let Y = some_unrelated_thing;
//     ...other items that don't touch X...
//     X = e;
//
// (where `let X;` is `Declare([X])` and `X = e;` is `Assign([X], e)` later in the same `Seq`).
// We want to recover the source-form `let X = e;`. This pass walks each `Declare` and, looking
// forward in its enclosing `Seq`, tries to find the assignment that initializes each declared
// name. When the *first* item that touches a name `X` is exactly the `Assign` that initializes
// it — i.e., `Assign(targets, rhs)` whose `targets` are a subset of the declared names and
// whose `rhs` doesn't read any of them — we rewrite that `Assign` to a `LetBind` and drop the
// fused names from the `Declare`. If any pending name is touched in some other way first
// (read, used inside a nested expression, multi-target assign with non-pending targets, etc.)
// it stays in the `Declare` — fusing in that case would either reorder a read past an unbound
// use or change scoping.
//
// The pass is forward-scanning per `Declare` and conservative: a name that's blocked stays
// declared, and we never fuse across an item that reads a still-pending name.

struct FuseLet;

impl Refine for FuseLet {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::Seq(items) = exp else {
            return false;
        };
        fuse_seq(items)
    }
}

// -------------------------------------------------------------------------------------------------
// Helpers

fn fuse_seq(items: &mut Vec<Exp>) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < items.len() {
        let Exp::Declare(declared) = &items[i] else {
            i += 1;
            continue;
        };
        let declared: Vec<String> = declared.clone();
        let mut pending: BTreeSet<String> = declared.iter().cloned().collect();
        let mut fused: BTreeSet<String> = BTreeSet::new();
        let mut fusion_positions: Vec<usize> = Vec::new();

        for (j, item) in items.iter().enumerate().skip(i + 1) {
            if pending.is_empty() {
                break;
            }
            classify_item(item, &mut pending, &mut fused, &mut fusion_positions, j);
        }

        if fused.is_empty() {
            i += 1;
            continue;
        }

        // Apply fusions in reverse so positional indices remain valid as we mutate in place.
        for &j in fusion_positions.iter().rev() {
            let item = std::mem::replace(&mut items[j], Exp::Seq(vec![]));
            let Exp::Assign(targets, rhs) = item else {
                unreachable!("fusion_positions only records Assign indices");
            };
            items[j] = Exp::LetBind(targets, rhs);
        }

        // Drop the fused names from the Declare; remove the Declare if nothing remains.
        let remaining: Vec<String> = declared
            .into_iter()
            .filter(|n| !fused.contains(n))
            .collect();
        changed = true;
        if remaining.is_empty() {
            items.remove(i);
            // Don't advance i — what was at i+1 is now at i, and we want to keep walking.
        } else {
            items[i] = Exp::Declare(remaining);
            i += 1;
        }
    }
    changed
}

/// Inspect `item` at position `j` and update the fusion state.
///
/// - If `item` is an `Assign(targets, rhs)` whose `targets` are *all* still pending and whose
///   `rhs` doesn't read any pending name, mark those targets fused and record `j` for rewriting.
/// - Otherwise, every pending name that `item` touches (writes or reads, anywhere in the
///   subtree) is "blocked": removed from `pending` without being added to `fused`, so the
///   `Declare` keeps it.
fn classify_item(
    item: &Exp,
    pending: &mut BTreeSet<String>,
    fused: &mut BTreeSet<String>,
    fusion_positions: &mut Vec<usize>,
    j: usize,
) {
    if let Exp::Assign(targets, rhs) = item {
        let rhs_refs = rhs.referenced_names();
        let all_targets_pending = targets.iter().all(|t| pending.contains(t));
        let rhs_safe = !rhs_refs.iter().any(|n| pending.contains(n));
        if all_targets_pending && rhs_safe && !targets.is_empty() {
            for t in targets {
                pending.remove(t);
                fused.insert(t.clone());
            }
            fusion_positions.push(j);
            return;
        }
        // Otherwise: block any pending names this Assign touches (its targets and any name
        // its rhs references).
        for t in targets {
            pending.remove(t);
        }
        for n in &rhs_refs {
            pending.remove(n);
        }
        return;
    }

    // Any other item: collect every name it touches and block them.
    for n in &item.referenced_names() {
        pending.remove(n);
    }
}
