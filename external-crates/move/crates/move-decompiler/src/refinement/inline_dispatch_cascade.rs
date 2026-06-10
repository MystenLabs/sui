// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Inline the residue of `compress_dispatch_cascade` back into the if-tree that produced
// the selector, dissolving the selector variable entirely. The pattern after
// `compress_dispatch_cascade` looks like:
//
//   Seq[
//     LetBind(s, if_tree),         // if_tree's leaves are Value(U32(K_i))
//     ...setup statements...,
//     if (s <= 0) B_0,
//     if (s <= 1) B_1,
//     ...
//     if (s <= N-1) B_{N-1},
//     ...common tail...
//   ]
//
// When every leaf of `if_tree` is a U32 constant and `s` has no uses outside the cascade
// tests, we can:
//   * For each cascade arm K, find the LCA node in `if_tree` of the set of leaves with
//     value <= K — this must be an exact subtree (every leaf under the LCA has value <= K
//     and every leaf with value <= K is under the LCA). If no exact subtree exists for
//     some K, bail.
//   * Convert `if_tree` from a value-producing expression to a statement: drop the leaf
//     values, drop the `LetBind`, and at each LCA position insert the corresponding
//     `B_K` body in the enclosing scope.
//
// The result reproduces what the original Move source looked like before bytecode-optimizer
// fusion + dispatch-table synthesis: the if-tree carries the per-path code at the leaves
// and the shared post-actions at the LCA boundaries.

use crate::{ast::Exp, refinement::Refine};
use move_core_types::runtime_value::MoveValue;
use move_stackless_bytecode_2::ast::PrimitiveOp;

type Var = String;

pub fn refine(exp: &mut Exp) -> bool {
    Inliner { changed: false }.run(exp)
}

struct Inliner {
    changed: bool,
}

impl Inliner {
    fn run(mut self, exp: &mut Exp) -> bool {
        self.refine(exp);
        self.changed
    }
}

impl Refine for Inliner {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        if let Exp::Seq(items) = exp
            && try_inline(items)
        {
            self.changed = true;
            return true;
        }
        false
    }
}

/// Find a cascade pattern in `items` and inline it. Returns true if a transformation was
/// applied. Recurses into nested Seqs via the outer `Refine` walker; this function only
/// looks at the top-level items of `items`.
fn try_inline(items: &mut Vec<Exp>) -> bool {
    // Try every LetBind candidate — the dispatch selector binding is rarely at index 0.
    let candidates: Vec<(usize, Var)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            Exp::LetBind(names, val) if names.len() == 1 && is_value_if_tree(val) => {
                Some((i, names[0].clone()))
            }
            _ => None,
        })
        .collect();

    for (let_idx, sel_name) in candidates {
        if try_inline_at(items, let_idx, &sel_name) {
            return true;
        }
    }
    false
}

fn try_inline_at(items: &mut Vec<Exp>, let_idx: usize, sel_name: &Var) -> bool {
    // Collect cascade tests `if (sel <= K) body` after the LetBind.
    let cascades = collect_cascades(items, let_idx, sel_name);
    if cascades.is_empty() {
        return false;
    }

    // Ensure sel has no uses outside (a) the LetBind itself and (b) the cascade tests.
    let cascade_idxs: Vec<usize> = cascades.iter().map(|c| c.idx).collect();
    if has_other_uses(items, let_idx, &cascade_idxs, sel_name) {
        return false;
    }

    // Extract the if_tree from the LetBind.
    let if_tree = match &items[let_idx] {
        Exp::LetBind(_, body) => (**body).clone(),
        _ => unreachable!(),
    };

    // For each cascade arm, compute the LCA path in the if_tree of `{ leaves with value <= K }`.
    let mut placements: Vec<(Vec<bool>, Exp)> = Vec::with_capacity(cascades.len());
    for c in &cascades {
        let path = match find_exact_subtree_path(&if_tree, c.bound) {
            Some(p) => p,
            None => return false,
        };
        placements.push((path, c.body.clone()));
    }

    // Apply transformation:
    //  1. Build the new if_tree-as-statement: first strip leaf U32 values (we don't need
    //     them anymore — the LCA paths are already computed), then insert each B_K at its
    //     LCA position. Stripping first ensures inserts don't leave the leaf's `K` value
    //     sandwiched between setup and the inserted body.
    //  2. Replace LetBind at let_idx with the transformed if_tree.
    //  3. Drop cascade entries.
    //  4. Pull up any Declares whose targets are assigned inside the inlined bodies: those
    //     Declares were positioned after the LetBind by `hoist_declarations` because the
    //     assignment sites were originally cascade arms (post-let). Now the assignments
    //     live deeper inside the if-tree (earlier in flow); the Declares must precede the
    //     if-tree to keep use-before-declare from sneaking in.
    let mut new_tree = if_tree;
    strip_leaf_values(&mut new_tree);
    for (path, body) in &placements {
        insert_at_path(&mut new_tree, path, body.clone());
    }

    // Collect names assigned inside any cascade body — these are the variables that moved
    // from post-let positions into the if-tree, and whose Declares may need hoisting.
    let mut assigned_in_bodies: std::collections::HashSet<Var> = std::collections::HashSet::new();
    for c in &cascades {
        collect_assigned_targets(&c.body, &mut assigned_in_bodies);
    }
    let cascade_set: std::collections::HashSet<usize> = cascade_idxs.iter().copied().collect();

    // Build new items vector: hoisted Declares first, then surviving items with the
    // LetBind replaced by the transformed if-tree.
    let mut hoisted: Vec<Exp> = Vec::new();
    let mut new_items: Vec<Exp> = Vec::with_capacity(items.len());
    for (i, item) in items.drain(..).enumerate() {
        if i == let_idx {
            // Insert any hoisted Declares right before the transformed if-tree.
            new_items.append(&mut hoisted);
            new_items.push(new_tree.clone());
            continue;
        }
        if cascade_set.contains(&i) {
            continue;
        }
        // If this is a Declare for any of the assigned-in-bodies variables that lives
        // AFTER let_idx, split it into the hoisted portion and the rest.
        if i > let_idx
            && let Exp::Declare(names) = &item
        {
            let (to_hoist, to_keep): (Vec<_>, Vec<_>) = names
                .iter()
                .cloned()
                .partition(|n| assigned_in_bodies.contains(n));
            if !to_hoist.is_empty() {
                // Hoisted Declares need to be inserted before the new_tree slot — but
                // we've already passed let_idx. Insert into new_items right before the
                // new_tree we just pushed.
                let pos = new_items.len() - 1; // index of new_tree
                new_items.insert(pos, Exp::Declare(to_hoist));
                if !to_keep.is_empty() {
                    new_items.push(Exp::Declare(to_keep));
                }
                continue;
            }
        }
        new_items.push(item);
    }
    *items = new_items;
    true
}

/// Collect names that appear as Assign targets anywhere within `exp`.
fn collect_assigned_targets(exp: &Exp, out: &mut std::collections::HashSet<Var>) {
    match exp {
        Exp::Assign(names, val) => {
            for n in names {
                out.insert(n.clone());
            }
            collect_assigned_targets(val, out);
        }
        Exp::LetBind(_, val) => collect_assigned_targets(val, out),
        Exp::Seq(items) | Exp::Return(items) | Exp::Call(_, items) => {
            for i in items {
                collect_assigned_targets(i, out);
            }
        }
        Exp::IfElse(c, t, a) => {
            collect_assigned_targets(c, out);
            collect_assigned_targets(t, out);
            if let Some(alt) = a.as_ref().as_ref() {
                collect_assigned_targets(alt, out);
            }
        }
        Exp::While(_, c, b) => {
            collect_assigned_targets(c, out);
            collect_assigned_targets(b, out);
        }
        Exp::Loop(_, b) | Exp::Block(_, b) => collect_assigned_targets(b, out),
        Exp::Switch(c, _, arms) => {
            collect_assigned_targets(c, out);
            for (_, b) in arms {
                collect_assigned_targets(b, out);
            }
        }
        Exp::Match(c, _, arms) => {
            collect_assigned_targets(c, out);
            for (_, _, b) in arms {
                collect_assigned_targets(b, out);
            }
        }
        Exp::MatchLit(c, arms) => {
            collect_assigned_targets(c, out);
            for (_, b) in arms {
                collect_assigned_targets(b, out);
            }
        }
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
            for a in args {
                collect_assigned_targets(a, out);
            }
        }
        Exp::Abort(e)
        | Exp::Borrow(_, e)
        | Exp::Unpack(_, _, e)
        | Exp::UnpackVariant(_, _, _, e)
        | Exp::VecUnpack(_, e) => collect_assigned_targets(e, out),
        Exp::Unstructured(nodes) => {
            use crate::ast::UnstructuredNode;
            for n in nodes {
                if let UnstructuredNode::Labeled(_, b) | UnstructuredNode::Statement(b) = n {
                    collect_assigned_targets(b, out);
                }
            }
        }
        Exp::Declare(_)
        | Exp::Break(_)
        | Exp::Continue(_)
        | Exp::Value(_)
        | Exp::Variable(_)
        | Exp::Constant(_) => {}
    }
}

struct CascadeArm {
    idx: usize,
    bound: u32,
    body: Exp,
}

/// Collect cascade arms `if (sel <= K) body` after `let_idx` (the cascade may be
/// interleaved with other items). Returned sorted by K. The cascade tests must form a
/// contiguous K range starting at 0 with no gaps.
fn collect_cascades(items: &[Exp], let_idx: usize, sel: &Var) -> Vec<CascadeArm> {
    let mut arms = vec![];
    for (i, e) in items.iter().enumerate().skip(let_idx + 1) {
        if let Some((k, body)) = match_cascade_test(e, sel) {
            arms.push(CascadeArm {
                idx: i,
                bound: k,
                body: body.clone(),
            });
        }
    }
    arms.sort_by_key(|a| a.bound);
    // Require K range to be 0..N contiguous (matching the form `compress_dispatch_cascade`
    // emits). Anything sparser means the cascade has been further transformed by some
    // other pass — bail rather than partially fire.
    for (i, a) in arms.iter().enumerate() {
        if a.bound != i as u32 {
            return vec![];
        }
    }
    arms
}

/// Pattern: `IfElse(Primitive(LessThanOrEqual, [Variable(sel), Value(U32(K))]), body, None)`.
fn match_cascade_test<'a>(e: &'a Exp, sel: &Var) -> Option<(u32, &'a Exp)> {
    let Exp::IfElse(cond, body, alt) = e else {
        return None;
    };
    if alt.as_ref().is_some() {
        return None;
    }
    let Exp::Primitive { op, args } = &**cond else {
        return None;
    };
    if *op != PrimitiveOp::LessThanOrEqual || args.len() != 2 {
        return None;
    }
    let Exp::Variable(v) = &args[0] else {
        return None;
    };
    if v != sel {
        return None;
    }
    let k = match &args[1] {
        Exp::Value(MoveValue::U32(k)) => *k,
        _ => return None,
    };
    Some((k, body))
}

/// True if any item in `items` (other than the LetBind itself and the cascade entries)
/// references `sel`. Recurses through expression structure.
fn has_other_uses(items: &[Exp], let_idx: usize, cascade_idxs: &[usize], sel: &Var) -> bool {
    let cascade_set: std::collections::HashSet<usize> = cascade_idxs.iter().copied().collect();
    for (i, e) in items.iter().enumerate() {
        if i == let_idx || cascade_set.contains(&i) {
            continue;
        }
        if contains_var(e, sel) {
            return true;
        }
    }
    false
}

fn contains_var(e: &Exp, sel: &Var) -> bool {
    match e {
        Exp::Variable(v) => v == sel,
        Exp::Seq(items) | Exp::Return(items) | Exp::Call(_, items) => {
            items.iter().any(|i| contains_var(i, sel))
        }
        Exp::IfElse(c, t, a) => {
            contains_var(c, sel)
                || contains_var(t, sel)
                || a.as_ref()
                    .as_ref()
                    .is_some_and(|alt| contains_var(alt, sel))
        }
        Exp::While(_, c, b) => contains_var(c, sel) || contains_var(b, sel),
        Exp::Loop(_, b) | Exp::Block(_, b) => contains_var(b, sel),
        Exp::Switch(c, _, arms) => {
            contains_var(c, sel) || arms.iter().any(|(_, b)| contains_var(b, sel))
        }
        Exp::Match(c, _, arms) => {
            contains_var(c, sel) || arms.iter().any(|(_, _, b)| contains_var(b, sel))
        }
        Exp::MatchLit(c, arms) => {
            contains_var(c, sel) || arms.iter().any(|(_, b)| contains_var(b, sel))
        }
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
            args.iter().any(|a| contains_var(a, sel))
        }
        Exp::Assign(targets, val) => targets.iter().any(|t| t == sel) || contains_var(val, sel),
        Exp::LetBind(targets, val) => targets.iter().any(|t| t == sel) || contains_var(val, sel),
        Exp::Declare(targets) => targets.iter().any(|t| t == sel),
        Exp::Abort(e)
        | Exp::Borrow(_, e)
        | Exp::Unpack(_, _, e)
        | Exp::UnpackVariant(_, _, _, e)
        | Exp::VecUnpack(_, e) => contains_var(e, sel),
        Exp::Unstructured(nodes) => {
            use crate::ast::UnstructuredNode;
            nodes.iter().any(|n| match n {
                UnstructuredNode::Labeled(_, b) | UnstructuredNode::Statement(b) => {
                    contains_var(b, sel)
                }
                UnstructuredNode::Goto(_) => false,
            })
        }
        Exp::Break(_) | Exp::Continue(_) | Exp::Value(_) | Exp::Constant(_) => false,
    }
}

/// True if `exp` is an if-tree whose every leaf (the value-producing tail of each arm) is
/// `Value(U32(_))`. Recurses through Seq wrappers in arms — an arm can be either a direct
/// IfElse, a direct U32, or a `Seq[…setup…, IfElse-or-U32]`.
fn is_value_if_tree(exp: &Exp) -> bool {
    if leaf_value(exp).is_some() {
        return true;
    }
    match peek_tail(exp) {
        Exp::IfElse(_, t, a) => {
            let Some(alt) = a.as_ref().as_ref() else {
                return false;
            };
            is_value_if_tree(t) && is_value_if_tree(alt)
        }
        _ => false,
    }
}

/// Return the trailing expression of a Seq (recursively for nested Seqs), or the expression
/// itself if it isn't a Seq.
fn peek_tail(exp: &Exp) -> &Exp {
    match exp {
        Exp::Seq(items) => items.last().map_or(exp, peek_tail),
        _ => exp,
    }
}

/// If `exp` is a direct U32 leaf (possibly preceded by setup in a Seq), return its value.
fn leaf_value(exp: &Exp) -> Option<u32> {
    match exp {
        Exp::Value(MoveValue::U32(k)) => Some(*k),
        Exp::Seq(items) => items.last().and_then(leaf_value),
        _ => None,
    }
}

/// Walk every leaf in `exp` (an if-tree) and call `f` with its value. Order is left-to-right.
/// Peeks through Seq wrappers — an arm `Seq[setup, IfElse]` recurses into the IfElse.
fn for_each_leaf(exp: &Exp, f: &mut impl FnMut(u32)) {
    if let Some(v) = leaf_value(exp) {
        f(v);
        return;
    }
    if let Exp::IfElse(_, t, a) = peek_tail(exp) {
        for_each_leaf(t, f);
        if let Some(alt) = a.as_ref().as_ref() {
            for_each_leaf(alt, f);
        }
    }
}

/// For an if-tree node, find the path (sequence of `bool`s: false=then, true=else)
/// from `exp` to the deepest IfElse node N such that the leaves under N are exactly
/// `{ l | l.value <= bound }`. If no such N exists, returns None. An empty path means
/// the root itself is the LCA.
fn find_exact_subtree_path(exp: &Exp, bound: u32) -> Option<Vec<bool>> {
    // Special case: if every leaf in `exp` has value <= bound, the root is the answer.
    let mut all_leq = true;
    let mut any_leq = false;
    for_each_leaf(exp, &mut |v| {
        if v <= bound {
            any_leq = true;
        } else {
            all_leq = false;
        }
    });
    if !any_leq {
        return None;
    }
    if all_leq {
        return Some(vec![]);
    }
    // Mixed at root: descend. Peek past Seq wrappers to reach the IfElse.
    let Exp::IfElse(_, then_b, alt) = peek_tail(exp) else {
        return None;
    };
    let Some(else_b) = alt.as_ref().as_ref() else {
        return None;
    };
    let then_has = has_leq(then_b, bound);
    let else_has = has_leq(else_b, bound);
    match (then_has, else_has) {
        (true, false) => {
            let mut sub = find_exact_subtree_path(then_b, bound)?;
            sub.insert(0, false);
            Some(sub)
        }
        (false, true) => {
            let mut sub = find_exact_subtree_path(else_b, bound)?;
            sub.insert(0, true);
            Some(sub)
        }
        _ => None, // both sides have matching leaves but root isn't exact — not a subtree
    }
}

fn has_leq(exp: &Exp, bound: u32) -> bool {
    let mut found = false;
    for_each_leaf(exp, &mut |v| {
        if v <= bound {
            found = true;
        }
    });
    found
}

/// Navigate to the IfElse node at `path` in `exp` and append `body` to its enclosing scope
/// so `body` executes right after the IfElse completes. If `path` is empty, wrap `exp`
/// itself.
///
/// The IfElse may sit inside a `Seq[…setup…, IfElse]` arm; in that case we append to that
/// Seq rather than wrapping the IfElse alone (preserves the setup statements as siblings).
fn insert_at_path(exp: &mut Exp, path: &[bool], body: Exp) {
    if path.is_empty() {
        append_after_tail(exp, body);
        return;
    }
    // Walk into the IfElse arm. Peek past Seq wrappers to find the IfElse node.
    let inner = inner_ifelse_mut(exp);
    let Exp::IfElse(_, then_b, alt) = inner else {
        return;
    };
    let target = if path[0] {
        match alt.as_mut().as_mut() {
            Some(a) => a,
            None => return,
        }
    } else {
        &mut **then_b
    };
    insert_at_path(target, &path[1..], body);
}

/// Drill through Seq wrappers (`Seq[…setup…, X]`) to reach the trailing expression `X`
/// mutably. Returns `exp` itself if not a Seq.
fn inner_ifelse_mut(exp: &mut Exp) -> &mut Exp {
    if matches!(exp, Exp::Seq(items) if matches!(items.last(), Some(Exp::IfElse(_, _, _)))) {
        let Exp::Seq(items) = exp else { unreachable!() };
        return inner_ifelse_mut(items.last_mut().unwrap());
    }
    exp
}

/// Append `body` so it runs right after the trailing expression of `exp`. If `exp` is a
/// Seq, push `body` onto the end. Otherwise wrap `*exp = Seq[exp_taken, body]`.
fn append_after_tail(exp: &mut Exp, body: Exp) {
    if let Exp::Seq(items) = exp {
        items.push(body);
        return;
    }
    let taken = std::mem::replace(exp, Exp::Seq(vec![]));
    *exp = Exp::Seq(vec![taken, body]);
}

/// Replace every U32 leaf in `exp` (the if-tree) with `Seq[setup_if_any]` — i.e., drop
/// the value but keep any setup statements. After this `exp` no longer produces a U32.
fn strip_leaf_values(exp: &mut Exp) {
    if leaf_value(exp).is_some() {
        strip_trailing_value(exp);
        return;
    }
    match exp {
        Exp::IfElse(_, t, a) => {
            strip_leaf_values(t);
            if let Some(alt) = a.as_mut().as_mut() {
                strip_leaf_values(alt);
            }
        }
        Exp::Seq(items) => {
            // Setup + nested-IfElse arm: recurse on the trailing item.
            if let Some(last) = items.last_mut() {
                strip_leaf_values(last);
            }
        }
        _ => {}
    }
}

/// Replace `Value(U32(_))` with `Seq[]` (a no-op). For Seqs, drop the trailing U32 value.
fn strip_trailing_value(exp: &mut Exp) {
    match exp {
        Exp::Value(MoveValue::U32(_)) => {
            *exp = Exp::Seq(vec![]);
        }
        Exp::Seq(items) => {
            if matches!(items.last(), Some(Exp::Value(MoveValue::U32(_)))) {
                items.pop();
            } else if let Some(last) = items.last_mut() {
                strip_trailing_value(last);
            }
        }
        _ => {}
    }
}
