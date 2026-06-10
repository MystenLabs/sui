// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// -------------------------------------------------------------------------------------------------
// Cascade compression for dispatch tables
// -------------------------------------------------------------------------------------------------
//
// `structure_loop`'s multi-succ dispatch mode emits a `match (sel) { 0 => A_0, 1 => A_1,
// ..., (N-1) => A_{N-1} }` where each arm A_k is the cascade tail starting at succ k. When
// the loop's exits form a linear cascade in the CFG, the arms have a very specific shape:
// each A_k is the concatenation `unique_prefix_k :: A_{k+1}` — i.e., each arm starts with
// some code unique to that entry and then duplicates everything that subsequent arms also
// do. This is sound (it's literally NMG's "duplicated dispatch" step) but verbose.
//
// This refinement recognizes that shape and compresses it into the equivalent fall-through
// form:
//
//     if (sel <= 0) { unique_prefix_0 };
//     if (sel <= 1) { unique_prefix_1 };
//     ...
//     if (sel <= N-2) { unique_prefix_{N-2} };
//     A_{N-1}
//
// Semantics: each `if (sel <= K)` runs unique_prefix_K only when we entered at or before
// tag K, which is the same condition under which arm K's cascade included unique_prefix_K.
// The final arm A_{N-1} runs unconditionally (every entry point ≤ N-1).
//
// **Detection.** For each `MatchLit(scrutinee, arms)` with arms in tag order:
//   1. Treat each arm body as a flat list of items (flatten if it's a `Seq`).
//   2. For each adjacent pair (arm_k, arm_{k+1}), check arm_{k+1}'s items appear verbatim
//      at the tail of arm_k's items (structural equality, using Debug-format compare).
//   3. If every pair matches and at least one non-empty unique prefix exists, compress.
//
// Equality is by Debug-formatted string. The arms came from cloning the same `Structured`
// nodes, so their `Exp` shapes are identical modulo refinement transformations applied to
// each arm. Refinement passes run identically over each arm body, so Debug-strings stay
// in sync.

use crate::ast::{Exp, exp_eq::exp_struct_eq};

use move_stackless_bytecode_2::ast::PrimitiveOp;

pub fn refine(exp: &mut Exp) -> bool {
    let mut pass = Compress { changed: false };
    pass.walk(exp);
    pass.changed
}

struct Compress {
    changed: bool,
}

impl Compress {
    /// Recursively walks `exp` post-order, applying the compression at every `MatchLit`.
    /// Inner constructs get refined first; the outer compression sees their final shape.
    fn walk(&mut self, exp: &mut Exp) {
        use Exp as E;
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
                for (_, body) in arms.iter_mut() {
                    self.walk(body);
                }
            }
            E::Match(c, _, arms) => {
                self.walk(c);
                for (_, _, body) in arms.iter_mut() {
                    self.walk(body);
                }
            }
            E::MatchLit(scrutinee, arms) => {
                self.walk(scrutinee);
                for (_, body) in arms.iter_mut() {
                    self.walk(body);
                }
                // After children are refined, try the compression on this MatchLit.
                if let Some(replacement) = try_compress(scrutinee, arms) {
                    *exp = replacement;
                    self.changed = true;
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
    }
}

/// Try to compress a `MatchLit` whose arms form a strict suffix cascade. Returns the
/// replacement `Exp` on success (`Seq[if (sel <= 0) prefix_0; ...; final_arm]`), or `None`
/// if the pattern doesn't fit.
fn try_compress(scrutinee: &Exp, arms: &[(u32, Exp)]) -> Option<Exp> {
    if arms.len() < 2 {
        return None;
    }
    // Tags should be the dense range 0..N (structuring emits them in tag order). If they
    // skip values or aren't sorted, bail — the `<=` cascade only makes sense for a dense
    // ordered tag space.
    for (i, (tag, _)) in arms.iter().enumerate() {
        if *tag != i as u32 {
            return None;
        }
    }
    let arm_items_by_idx: Vec<Vec<&Exp>> = arms.iter().map(|(_, body)| flatten_seq(body)).collect();
    // Check each consecutive pair: arm_{k+1} must appear as a suffix of arm_k.
    let mut prefix_lens: Vec<usize> = Vec::with_capacity(arms.len() - 1);
    for k in 0..arms.len() - 1 {
        let longer = &arm_items_by_idx[k];
        let shorter = &arm_items_by_idx[k + 1];
        if longer.len() < shorter.len() {
            return None;
        }
        let prefix_len = longer.len() - shorter.len();
        for j in 0..shorter.len() {
            if !exp_struct_eq(longer[prefix_len + j], shorter[j]) {
                return None;
            }
        }
        prefix_lens.push(prefix_len);
    }
    // Require at least one non-empty unique prefix; otherwise every arm is identical and
    // there's nothing to compress (we'd just emit the unconditional body, which is fine
    // but redundant — could happen if structure_loop produces a degenerate cascade).
    if prefix_lens.iter().all(|&n| n == 0) {
        return None;
    }
    // Build the output Seq: `if (sel <= k) { prefix_k };` for each k, then the final arm
    // unconditionally.
    let mut out: Vec<Exp> = Vec::with_capacity(arms.len());
    for (k, &prefix_len) in prefix_lens.iter().enumerate() {
        if prefix_len == 0 {
            continue;
        }
        let prefix_items: Vec<Exp> = arm_items_by_idx[k]
            .iter()
            .take(prefix_len)
            .map(|e| (*e).clone())
            .collect();
        let prefix_body = if prefix_items.len() == 1 {
            prefix_items.into_iter().next().unwrap()
        } else {
            Exp::Seq(prefix_items)
        };
        let cond = Exp::Primitive {
            op: PrimitiveOp::LessThanOrEqual,
            args: vec![
                scrutinee.clone(),
                Exp::Value(move_core_types::runtime_value::MoveValue::U32(k as u32)),
            ],
        };
        out.push(Exp::IfElse(
            Box::new(cond),
            Box::new(prefix_body),
            Box::new(None),
        ));
    }
    // Final arm: emit its items inline (no `if`).
    let last_items: Vec<Exp> = arm_items_by_idx
        .last()
        .unwrap()
        .iter()
        .map(|e| (*e).clone())
        .collect();
    let last = if last_items.len() == 1 {
        last_items.into_iter().next().unwrap()
    } else {
        Exp::Seq(last_items)
    };
    out.push(last);
    if out.len() == 1 {
        Some(out.into_iter().next().unwrap())
    } else {
        Some(Exp::Seq(out))
    }
}

/// Flatten a `Seq` into its items; non-Seq expressions become a single-item slice. We use
/// this so `Exp::Seq([a, b])` and a hypothetical `Exp::Seq([Exp::Seq([a]), b])` compare
/// equal — earlier refinements may have left nested Seqs that `flatten_seq` would later
/// collapse, but we don't want to depend on its order vs. ours.
fn flatten_seq(exp: &Exp) -> Vec<&Exp> {
    fn go<'a>(exp: &'a Exp, out: &mut Vec<&'a Exp>) {
        match exp {
            Exp::Seq(items) => {
                for i in items {
                    go(i, out);
                }
            }
            other => out.push(other),
        }
    }
    let mut out = Vec::new();
    go(exp, &mut out);
    out
}
