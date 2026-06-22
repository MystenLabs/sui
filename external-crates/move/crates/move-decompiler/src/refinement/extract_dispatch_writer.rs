// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// -------------------------------------------------------------------------------------------------
// Dispatch-writer extraction from a fused loop body
// -------------------------------------------------------------------------------------------------
//
// When `structure_loop`'s multi-succ dispatch synthesizes a `match (sel)` after a loop,
// the loop body ends up with a conditional that writes `sel` and `break`s at every leaf:
//
//     loop {
//       iter_check
//       if (cond_1) { ... ; sel = 0; break }
//       else if (cond_2) { sel = 1; break }
//       ...
//       else { sel = K; break }
//     }
//     match (sel) { ... }
//
// The if/else nest at the loop's tail is the **dispatch-writer**: every leaf path is
// guaranteed to set `sel` and `break`. That's a property we can exploit - the entire
// conditional structurally belongs OUTSIDE the loop, because:
//
//   1. None of its leaves continue the loop. Every path exits.
//   2. Pulling it past the loop and dropping the `break`s gives equivalent semantics:
//      the prefix exits via an inserted explicit `break`, then control naturally flows
//      to the extracted dispatch-writer, which sets `sel` (without the now-stale `break`).
//
// **Detection.** A trailing slice `items[k..n]` of the loop body's `Seq` qualifies if
// every leaf-evaluation path of every item terminates (Break(L), Return, or Abort) - never
// falls through, never continues. Then `items[k..n]` is reached only via fall-through from
// the prefix.
//
// **Soundness check.** The prefix `items[0..k]` must contain no `Break(L)`. Such a break
// would otherwise be re-routed: in the original it exited to "after the loop" =
// POST_LOOP_CODE; after extraction the suffix sits between the loop and POST_LOOP_CODE, so
// the break would now execute the suffix on its way out.
//
// **Rewrite.**
//   - Take `items.split_off(k)` as the extracted suffix.
//   - Append `Break(L)` to the (shortened) `items` so fall-through paths in the prefix
//     that previously reached the suffix now exit the loop instead of re-iterating.
//   - In the extracted suffix, replace every `Break(L)` with an empty `Seq` (the loop no
//     longer encloses the suffix; the break has nothing to target). Other breaks/continues
//     targeting OUTER scopes are preserved.
//   - Wrap as `Seq[Loop(L, modified_body), suffix]`.
//
// After this runs, the loop's body looks like a normal single-exit `while`-equivalent
// (existing `introduce_while` will then collapse `loop { if (c) { ... continue } else { break } }`
// into `while (c) { ... }`), and the extracted dispatch-writer's `Assign(sel, K)` sites
// are visible to `fuse_let` + `hoist_arm_assignments`, which collapse them into a single
// `let sel = if (...) { K } else { ... }` expression. Combined with the cascade
// compression (`compress_dispatch_cascade.rs`), the final output reads as
//
//     while (...) { search-body }
//     let sel = if (...) { K } else { ... };
//     if (sel <= 0) { ... }; if (sel <= 1) { ... }; ...; last_body
//
// - close to what the original Move source looked like before the bytecode optimizer
// fused the post-loop code into the search loop.

use crate::ast::{Exp, Label};

pub fn refine(exp: &mut Exp) -> bool {
    let mut pass = Extract { changed: false };
    pass.walk(exp);
    pass.changed
}

struct Extract {
    changed: bool,
}

impl Extract {
    fn walk(&mut self, exp: &mut Exp) {
        use Exp as E;
        // Recurse into children first (post-order), then attempt extraction at THIS node if
        // it's a `Loop`. The post-order means nested loops get their tails pulled out
        // before we look at this loop's body.
        match exp {
            E::Loop(_, body) | E::Block(_, body) => self.walk(body),
            E::While(_, c, b) => {
                self.walk(c);
                self.walk(b);
            }
            E::Seq(items) | E::Return(items) | E::Call(_, items) => {
                for i in items.iter_mut() {
                    self.walk(i);
                }
            }
            E::IfElse(c, t, alt) => {
                self.walk(c);
                self.walk(t);
                if let Some(a) = alt.as_mut().as_mut() {
                    self.walk(a);
                }
            }
            E::Switch(c, _, arms) => {
                self.walk(c);
                for (_, b) in arms.iter_mut() {
                    self.walk(b);
                }
            }
            E::Match(c, _, arms) => {
                self.walk(c);
                for (_, _, b) in arms.iter_mut() {
                    self.walk(b);
                }
            }
            E::MatchLit(s, arms) => {
                self.walk(s);
                for (_, b) in arms.iter_mut() {
                    self.walk(b);
                }
            }
            E::Primitive { args, .. } | E::Data { args, .. } => {
                for a in args.iter_mut() {
                    self.walk(a);
                }
            }
            E::Assign(_, e)
            | E::LetBind(_, e)
            | E::Abort(e)
            | E::Borrow(_, e)
            | E::Unpack(_, _, e)
            | E::UnpackVariant(_, _, _, e)
            | E::VecUnpack(_, e) => self.walk(e),
            E::Unstructured(nodes) => {
                use crate::ast::UnstructuredNode;
                for n in nodes.iter_mut() {
                    match n {
                        UnstructuredNode::Labeled(_, body) | UnstructuredNode::Statement(body) => {
                            self.walk(body);
                        }
                        UnstructuredNode::Goto(_) => {}
                    }
                }
            }
            E::Break(_)
            | E::Continue(_)
            | E::Declare(_)
            | E::Value(_)
            | E::Variable(_)
            | E::Constant(_) => {}
        }
        // After children are refined, attempt the extraction if this is a labeled loop.
        if matches!(exp, Exp::Loop(Some(_), _)) {
            let placeholder = Exp::Seq(vec![]);
            let taken = std::mem::replace(exp, placeholder);
            match try_extract(taken) {
                Ok(new_exp) => {
                    *exp = new_exp;
                    self.changed = true;
                }
                Err(original) => {
                    *exp = original;
                }
            }
        }
    }
}

/// Attempt extraction. On success returns `Ok(Seq[Loop(label, prefix_with_break),
/// suffix])`. On failure returns `Err(original_loop)` so the caller can restore it. The
/// failure modes are: non-Seq body, no qualifying trailing slice, or prefix contains
/// `Break(label)`.
fn try_extract(exp: Exp) -> Result<Exp, Exp> {
    let Exp::Loop(label_opt, body) = exp else {
        return Err(exp);
    };
    let Some(label) = label_opt else {
        return Err(Exp::Loop(label_opt, body));
    };
    let Exp::Seq(mut items) = *body else {
        return Err(Exp::Loop(Some(label), body));
    };
    if items.len() < 2 {
        return Err(Exp::Loop(Some(label), Box::new(Exp::Seq(items))));
    }
    let mut k = items.len();
    while k > 0 && all_paths_terminate(&items[k - 1], label) {
        k -= 1;
    }
    if k == items.len() {
        return Err(Exp::Loop(Some(label), Box::new(Exp::Seq(items))));
    }
    if items[..k].iter().any(|p| contains_break_for(p, label)) {
        return Err(Exp::Loop(Some(label), Box::new(Exp::Seq(items))));
    }
    // A `Continue(Some(label))` buried in the soon-to-be-extracted suffix would target a
    // loop we're about to remove. `all_paths_terminate` only checks tail positions, so an
    // early Continue in a Seq prefix passes the tail check and would dangle once extracted.
    // Bail rather than try to rewrite it; the dispatch-writer shape this fires on always
    // writes-then-breaks and doesn't construct such suffixes in practice.
    if items[k..].iter().any(|s| contains_continue_for(s, label)) {
        return Err(Exp::Loop(Some(label), Box::new(Exp::Seq(items))));
    }

    let suffix_items: Vec<Exp> = items.split_off(k);
    items.push(Exp::Break(Some(label)));

    let mut suffix = if suffix_items.len() == 1 {
        suffix_items.into_iter().next().unwrap()
    } else {
        Exp::Seq(suffix_items)
    };
    strip_break_for(&mut suffix, label);

    Ok(Exp::Seq(vec![
        Exp::Loop(Some(label), Box::new(Exp::Seq(items))),
        suffix,
    ]))
}

/// True iff every leaf evaluation path of `exp` terminates (Break(label), Return, or
/// Abort). A path that falls through, continues, or breaks to a different label doesn't
/// count.
///
/// Conservative on a few shapes:
/// - `Loop` / `While` return `false` unconditionally, even when their body unconditionally
///   returns/aborts. Recognizing that would require a recursive structural analysis that
///   detected "no escaping break path"; not worth the complexity until the corpus exercises
///   it.
/// - `Switch` / `Match` / `MatchLit` rely on the arm list being exhaustive over the
///   scrutinee. Move's enum dispatch is exhaustive at the type level, so post-structuring
///   shapes meet this. If a future refinement drops an arm during folding, this check is
///   over-optimistic - flag the change with a regression fixture.
fn all_paths_terminate(exp: &Exp, label: Label) -> bool {
    use Exp as E;
    match exp {
        E::Break(Some(l)) if *l == label => true,
        E::Return(_) | E::Abort(_) => true,
        E::Seq(items) => items
            .last()
            .map(|last| all_paths_terminate(last, label))
            .unwrap_or(false),
        E::IfElse(_, conseq, alt) => match alt.as_ref().as_ref() {
            Some(a) => all_paths_terminate(conseq, label) && all_paths_terminate(a, label),
            None => false,
        },
        E::Switch(_, _, arms) => arms.iter().all(|(_, b)| all_paths_terminate(b, label)),
        E::Match(_, _, arms) => arms.iter().all(|(_, _, b)| all_paths_terminate(b, label)),
        E::MatchLit(_, arms) => arms.iter().all(|(_, b)| all_paths_terminate(b, label)),
        // Everything else either falls through (Assign, Call, Primitive, ...) or has its
        // own loop scope that doesn't represent "this loop's exit" (Loop, While).
        // Unlabeled Break inside a nested loop targets that loop, not ours.
        _ => false,
    }
}

/// True iff `exp` syntactically contains `Break(Some(label))`. Descends through nested
/// constructs - a labeled break from inside an inner loop still exits OUR loop.
fn contains_break_for(exp: &Exp, label: Label) -> bool {
    use Exp as E;
    match exp {
        E::Break(Some(l)) => *l == label,
        E::Seq(items) | E::Return(items) | E::Call(_, items) => {
            items.iter().any(|i| contains_break_for(i, label))
        }
        E::IfElse(c, t, alt) => {
            contains_break_for(c, label)
                || contains_break_for(t, label)
                || alt
                    .as_ref()
                    .as_ref()
                    .is_some_and(|a| contains_break_for(a, label))
        }
        E::Switch(c, _, arms) => {
            contains_break_for(c, label) || arms.iter().any(|(_, b)| contains_break_for(b, label))
        }
        E::Match(c, _, arms) => {
            contains_break_for(c, label)
                || arms.iter().any(|(_, _, b)| contains_break_for(b, label))
        }
        E::MatchLit(c, arms) => {
            contains_break_for(c, label) || arms.iter().any(|(_, b)| contains_break_for(b, label))
        }
        E::Loop(_, b) | E::Block(_, b) => contains_break_for(b, label),
        E::While(_, c, b) => contains_break_for(c, label) || contains_break_for(b, label),
        E::Primitive { args, .. } | E::Data { args, .. } => {
            args.iter().any(|a| contains_break_for(a, label))
        }
        E::Assign(_, e)
        | E::LetBind(_, e)
        | E::Abort(e)
        | E::Borrow(_, e)
        | E::Unpack(_, _, e)
        | E::UnpackVariant(_, _, _, e)
        | E::VecUnpack(_, e) => contains_break_for(e, label),
        E::Unstructured(nodes) => {
            use crate::ast::UnstructuredNode;
            nodes.iter().any(|n| match n {
                UnstructuredNode::Labeled(_, body) | UnstructuredNode::Statement(body) => {
                    contains_break_for(body, label)
                }
                UnstructuredNode::Goto(_) => false,
            })
        }
        E::Break(None)
        | E::Continue(_)
        | E::Declare(_)
        | E::Value(_)
        | E::Variable(_)
        | E::Constant(_) => false,
    }
}

/// True iff `exp` syntactically contains `Continue(Some(label))`. Mirrors
/// `contains_break_for`; used to bail extraction when the suffix would dangle a continue
/// at the now-removed loop's label.
fn contains_continue_for(exp: &Exp, label: Label) -> bool {
    use Exp as E;
    match exp {
        E::Continue(Some(l)) => *l == label,
        E::Seq(items) | E::Return(items) | E::Call(_, items) => {
            items.iter().any(|i| contains_continue_for(i, label))
        }
        E::IfElse(c, t, alt) => {
            contains_continue_for(c, label)
                || contains_continue_for(t, label)
                || alt
                    .as_ref()
                    .as_ref()
                    .is_some_and(|a| contains_continue_for(a, label))
        }
        E::Switch(c, _, arms) => {
            contains_continue_for(c, label)
                || arms.iter().any(|(_, b)| contains_continue_for(b, label))
        }
        E::Match(c, _, arms) => {
            contains_continue_for(c, label)
                || arms.iter().any(|(_, _, b)| contains_continue_for(b, label))
        }
        E::MatchLit(c, arms) => {
            contains_continue_for(c, label)
                || arms.iter().any(|(_, b)| contains_continue_for(b, label))
        }
        E::Loop(_, b) | E::Block(_, b) => contains_continue_for(b, label),
        E::While(_, c, b) => contains_continue_for(c, label) || contains_continue_for(b, label),
        E::Primitive { args, .. } | E::Data { args, .. } => {
            args.iter().any(|a| contains_continue_for(a, label))
        }
        E::Assign(_, e)
        | E::LetBind(_, e)
        | E::Abort(e)
        | E::Borrow(_, e)
        | E::Unpack(_, _, e)
        | E::UnpackVariant(_, _, _, e)
        | E::VecUnpack(_, e) => contains_continue_for(e, label),
        E::Unstructured(nodes) => {
            use crate::ast::UnstructuredNode;
            nodes.iter().any(|n| match n {
                UnstructuredNode::Labeled(_, body) | UnstructuredNode::Statement(body) => {
                    contains_continue_for(body, label)
                }
                UnstructuredNode::Goto(_) => false,
            })
        }
        E::Continue(None)
        | E::Break(_)
        | E::Declare(_)
        | E::Value(_)
        | E::Variable(_)
        | E::Constant(_) => false,
    }
}

/// Replace each `Break(Some(label))` in `exp` with an empty `Seq` (a no-op that
/// `flatten_seq` will drop). Used on the extracted suffix to neutralize breaks that
/// targeted the loop we just pulled out of.
fn strip_break_for(exp: &mut Exp, label: Label) {
    use Exp as E;
    match exp {
        E::Break(Some(l)) if *l == label => {
            *exp = E::Seq(vec![]);
        }
        E::Seq(items) | E::Return(items) | E::Call(_, items) => {
            for i in items.iter_mut() {
                strip_break_for(i, label);
            }
        }
        E::IfElse(c, t, alt) => {
            strip_break_for(c, label);
            strip_break_for(t, label);
            if let Some(a) = alt.as_mut().as_mut() {
                strip_break_for(a, label);
            }
        }
        E::Switch(c, _, arms) => {
            strip_break_for(c, label);
            for (_, b) in arms.iter_mut() {
                strip_break_for(b, label);
            }
        }
        E::Match(c, _, arms) => {
            strip_break_for(c, label);
            for (_, _, b) in arms.iter_mut() {
                strip_break_for(b, label);
            }
        }
        E::MatchLit(c, arms) => {
            strip_break_for(c, label);
            for (_, b) in arms.iter_mut() {
                strip_break_for(b, label);
            }
        }
        E::Loop(_, b) | E::Block(_, b) => strip_break_for(b, label),
        E::While(_, c, b) => {
            strip_break_for(c, label);
            strip_break_for(b, label);
        }
        E::Primitive { args, .. } | E::Data { args, .. } => {
            for a in args.iter_mut() {
                strip_break_for(a, label);
            }
        }
        E::Assign(_, e)
        | E::LetBind(_, e)
        | E::Abort(e)
        | E::Borrow(_, e)
        | E::Unpack(_, _, e)
        | E::UnpackVariant(_, _, _, e)
        | E::VecUnpack(_, e) => strip_break_for(e, label),
        E::Unstructured(nodes) => {
            use crate::ast::UnstructuredNode;
            for n in nodes.iter_mut() {
                match n {
                    UnstructuredNode::Labeled(_, body) | UnstructuredNode::Statement(body) => {
                        strip_break_for(body, label);
                    }
                    UnstructuredNode::Goto(_) => {}
                }
            }
        }
        E::Break(_)
        | E::Continue(_)
        | E::Declare(_)
        | E::Value(_)
        | E::Variable(_)
        | E::Constant(_) => {}
    }
}
