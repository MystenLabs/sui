// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// -------------------------------------------------------------------------------------------------
// Hoist a shared landing block through its goto LCA
// -------------------------------------------------------------------------------------------------
//
// `goto_to_break` only rewrites a `Goto(N)` whose target `Block(N, body)` lives as a SIBLING
// in the same `Seq` - it wraps the prefix and the labeled scope cleanly. When the dom-tree
// structurer instead inlined `Block(N, body)` inside a nested scope (a loop body, an IfElse
// arm, ...) and a `Goto(N)` survives in an OUTER `Seq` because the structurer couldn't reach
// the inlined position from there, we have a goto that no labeled-break can express: the
// labeled scope `'label_N` sits *inside* the loop the goto sits *outside* of, so
// `break 'label_N` from the outer position would target a scope that doesn't enclose it.
//
// This refinement handles that. When we find:
//
//     Seq[ ...prefix,
//          <item containing nested Block(N, body) somewhere>,
//          ...intermediate items not jumping to N,
//          Unstructured([Goto(N)]) ]
//
// and `body` always exits via `Return` / `Abort` (no fall-through past the inlined position),
// we hoist `body` past the goto by wrapping the range and rewriting the inner block:
//
//     Seq[ ...prefix,
//          Block(N, Seq[ <item with Block(N,body) -> Break(Some(N))>,
//                        ...intermediate items, ALL `Goto(N)` -> `Break(Some(N))` ]),
//          body ]
//
// Path equivalence:
//   - The original *inner* path through `Block(N, body)` executes `body` (which Returns).
//     After the rewrite it `Break`s the wrap, control falls through to `body`, which
//     still Returns - observationally identical.
//   - The original *outer* `Goto(N)` jumps to the inlined block and runs `body` (Returns).
//     After the rewrite it's replaced by `body` directly - same `Return`.
//
// Soundness:
//   - `body` must always terminate (Return / Abort). Otherwise the outer fall-through path
//     would land somewhere new and diverge from the original (which had no fall-through).
//   - The Block must be nested - *not* a direct sibling in the goto's Seq. The sibling case
//     is `goto_to_break`'s domain; firing here too would double-wrap.
//   - We require a *single* `Block(N, body)` in the rewrite region (no other definitions of
//     the same label). Multiple definitions would mean ambiguous landings; we bail.

use crate::{
    ast::{Exp, Label, UnstructuredNode},
    refinement::utils::{rewrite_gotos_as_breaks, walk_children, walk_children_mut},
};

pub fn refine(exp: &mut Exp) -> bool {
    let mut changed = false;
    walk(exp, &mut changed);
    changed
}

fn walk(exp: &mut Exp, changed: &mut bool) {
    // Post-order: recur into children first, then try the lift at this `Seq`. The
    // post-order matters when a lift would surface a new opportunity in an enclosing
    // scope - the outer `walk` runs after this returns and the next outer refinement
    // iteration picks it up.
    walk_children_mut(exp, &mut |c| walk(c, changed));
    if let Exp::Seq(items) = exp {
        while try_lift_in_seq(items) {
            *changed = true;
        }
    }
}

/// Returns `Some(N)` iff `exp` is `Unstructured` whose only content is `Goto(N)`.
fn singleton_goto(exp: &Exp) -> Option<Label> {
    let Exp::Unstructured(nodes) = exp else {
        return None;
    };
    if nodes.len() != 1 {
        return None;
    }
    match &nodes[0] {
        UnstructuredNode::Goto(l) => Some(*l),
        _ => None,
    }
}

fn try_lift_in_seq(items: &mut Vec<Exp>) -> bool {
    // Walk through items looking for a singleton goto at position `j` whose target `N` has
    // a nested `Block(N, body)` somewhere in `items[k]` for some `k < j`. Bail on:
    //  - Any sibling at this Seq level is already `Block(N, ...)` - that's `goto_to_break`'s
    //    shape and we don't want to compete with it.
    //  - `body` doesn't always terminate (would leave a fall-through diverging from goto).
    //  - Multiple `Block(N, ...)` candidates exist across `items[0..j]` (ambiguous landing).
    for j in 0..items.len() {
        let Some(target) = singleton_goto(&items[j]) else {
            continue;
        };

        if items
            .iter()
            .any(|it| matches!(it, Exp::Block(n, _) if *n == target))
        {
            continue;
        }

        let mut found_k: Option<usize> = None;
        let mut bail = false;
        for (k, item) in items[..j].iter().enumerate() {
            match nested_block_status(item, target) {
                BlockStatus::Terminating => {
                    if found_k.is_some() {
                        bail = true;
                        break;
                    }
                    found_k = Some(k);
                }
                BlockStatus::NonTerminating => {
                    bail = true;
                    break;
                }
                BlockStatus::Absent => {}
            }
        }
        if bail {
            continue;
        }
        let Some(k) = found_k else { continue };

        // Extract body, replace inner `Block(N, body)` with `Break(Some(N))`.
        let Some(body) = extract_block_replace_with_break(&mut items[k], target) else {
            continue;
        };

        // Sweep the wrap region for any other `Goto(N)`s and rewrite them as
        // `Break(Some(N))`. (Only after extracting above so we don't double-touch the
        // inner Block, which we already turned into a Break.)
        for item in items[k..j].iter_mut() {
            rewrite_gotos_as_breaks(item, target);
        }

        // Wrap items[k..j] in `Block(target, Seq[...])`; replace items[j] (the goto) with
        // the extracted body.
        let wrapped: Vec<Exp> = items.drain(k..j).collect();
        items.insert(k, Exp::Block(target, Box::new(Exp::Seq(wrapped))));
        // After the drain+insert, the original goto at position `j` is now at position
        // `k + 1` (the drain removed `j - k` items, and we re-inserted one wrap).
        items[k + 1] = body;
        return true;
    }
    false
}

/// What we found when scanning a subtree for `Block(target, ...)`. The three cases are
/// genuinely distinct decisions for the caller - `Absent` is a soft "skip this sibling,
/// keep looking", `NonTerminating` is "bail the whole lift", `Terminating` is the only
/// fireable case. Encoding them as an enum reads more clearly at the call site than a
/// `Option<bool>` where each variant's meaning was non-obvious.
enum BlockStatus {
    /// No `Block(target, ...)` lives in this subtree.
    Absent,
    /// Exactly one `Block(target, ...)` lives in this subtree, and its body always exits
    /// (return/abort/labeled-break to some non-`target` label). Safe to hoist.
    Terminating,
    /// Either the found block's body might fall through, or there are multiple
    /// `Block(target, ...)` nested in this subtree (ambiguous landing). Bail the lift.
    NonTerminating,
}

fn nested_block_status(exp: &Exp, target: Label) -> BlockStatus {
    if let Exp::Block(n, body) = exp
        && *n == target
    {
        return if all_paths_exit(body, target) {
            BlockStatus::Terminating
        } else {
            BlockStatus::NonTerminating
        };
    }
    let mut result = BlockStatus::Absent;
    walk_children(exp, &mut |c| match nested_block_status(c, target) {
        BlockStatus::Absent => {}
        BlockStatus::NonTerminating => result = BlockStatus::NonTerminating,
        BlockStatus::Terminating => match result {
            BlockStatus::Absent => result = BlockStatus::Terminating,
            // Two Terminating blocks at sibling positions in the same subtree are still
            // ambiguous from the caller's perspective. Bail.
            BlockStatus::Terminating => result = BlockStatus::NonTerminating,
            BlockStatus::NonTerminating => {}
        },
    });
    result
}

/// True iff every path through `exp` ends in `Return`, `Abort`, or a `Break`/`Continue` to
/// a label *other* than `self_label`. A `Break(Some(self_label))` is a self-loop on our own
/// landing - not a real exit, refuses the lift.
fn all_paths_exit(exp: &Exp, self_label: Label) -> bool {
    use Exp as E;
    match exp {
        E::Return(_) | E::Abort(_) => true,
        E::Break(Some(l)) | E::Continue(Some(l)) => *l != self_label,
        // Unlabeled break/continue targets an enclosing loop (which is NOT our wrap).
        // From the wrap's perspective, an unlabeled break exits some inner loop and then
        // execution continues in the wrap. That's NOT an exit out of the wrap, so it
        // doesn't satisfy "always exit". Conservative: refuse.
        E::Break(None) | E::Continue(None) => false,
        E::Seq(items) => items.last().is_some_and(|l| all_paths_exit(l, self_label)),
        E::IfElse(_, t, alt) => match alt.as_ref().as_ref() {
            Some(a) => all_paths_exit(t, self_label) && all_paths_exit(a, self_label),
            None => false,
        },
        E::Switch(_, _, arms) => arms.iter().all(|(_, b)| all_paths_exit(b, self_label)),
        E::Match(_, _, arms) => arms.iter().all(|(_, _, b)| all_paths_exit(b, self_label)),
        E::MatchLit(_, arms) => arms.iter().all(|(_, b)| all_paths_exit(b, self_label)),
        E::Block(_, body) => all_paths_exit(body, self_label),
        _ => false,
    }
}

/// Find a `Block(target, body)` in `exp`'s subtree, replace it with `Break(Some(target))`,
/// and return the body. Returns `None` if no such block exists in this subtree. Replaces
/// the *first* such block encountered (DFS), which is the one detected by
/// `nested_block_status` above.
fn extract_block_replace_with_break(exp: &mut Exp, target: Label) -> Option<Exp> {
    if let Exp::Block(n, body) = exp
        && *n == target
    {
        let taken = std::mem::replace(body.as_mut(), Exp::Seq(vec![]));
        *exp = Exp::Break(Some(target));
        return Some(taken);
    }
    let mut result: Option<Exp> = None;
    walk_children_mut(exp, &mut |c| {
        if result.is_none() {
            result = extract_block_replace_with_break(c, target);
        }
    });
    result
}
