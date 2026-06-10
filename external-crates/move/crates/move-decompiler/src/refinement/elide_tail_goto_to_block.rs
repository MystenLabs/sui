// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// -------------------------------------------------------------------------------------------------
// Tail-goto-to-immediate-block elision
// -------------------------------------------------------------------------------------------------
//
// The dom-tree acyclic structurer used to elide tail `Jump`s targeting orphan-hoisted
// siblings via `convergence_ok` + `elide_tail_jump_to`; `-1.4` deleted that machinery
// because reaching covers most acyclic shapes goto-free. For the shapes reaching doesn't
// fold the dom-tree path now surfaces those tail jumps as `Unstructured(Goto)` in the
// output. The dominant pattern is:
//
//     if (cond) {
//         body_t;
//         unstructured { goto 'label_N; }
//     } else {
//         body_e;
//         unstructured { goto 'label_N; }
//     };
//     /* block N */;  // <- Exp::Block(N, ...)
//     post_action
//
// Fall-through from the if-else's natural exit lands at the immediately-following
// `Block(N, _)`, which is what the gotos target. So the gotos are redundant: stripping
// them changes nothing semantically and renders cleanly:
//
//     if (cond) { body_t } else { body_e };
//     /* block N */;
//     post_action
//
// This refinement does exactly that. It looks at every `Seq` and, for each adjacent pair
// `(items[i], items[i+1])` where `items[i+1]` is `Block(N, _)`, walks the tail positions
// of `items[i]` (last item of a `Seq`, both arms of `IfElse`, every arm of `Switch`/
// `Match`/`MatchLit`) and pops any `Unstructured(…, Goto(N))` it finds. Empty
// `Unstructured` after the pop becomes an empty `Seq` (later refinements drop it).
//
// Conservative: only strips when `Block(N, _)` is the *immediately* next sibling. A goto
// to a later sibling would skip everything between, which is a different execution path,
// so we leave those for the labeled-break fallback (when/if `-3`'s deletion path is taken).

use crate::ast::{Exp, UnstructuredNode};

pub fn refine(exp: &mut Exp) -> bool {
    let mut changed = false;
    walk(exp, &mut changed);
    changed
}

fn walk(exp: &mut Exp, changed: &mut bool) {
    use Exp as E;
    match exp {
        E::Seq(items) => {
            for item in items.iter_mut() {
                walk(item, changed);
            }
            for i in 0..items.len().saturating_sub(1) {
                // Look past any side-effect-free declarations (`hoist_declarations` often
                // floats a bare `let l;` between the if-else and the labeled block whose body
                // uses it) to find the eligible `Block(N, _)`. Statements like `Declare(_)`
                // don't change control flow, so the goto's natural fall-through still lands
                // on `Block(N)` after we skip them.
                let mut j = i + 1;
                while j < items.len() && matches!(&items[j], E::Declare(_)) {
                    j += 1;
                }
                let target = match items.get(j) {
                    Some(E::Block(n, _)) => *n,
                    _ => continue,
                };
                if strip_tail_goto_to(&mut items[i], target) {
                    *changed = true;
                }
            }
        }
        E::Loop(_, body) | E::Block(_, body) => walk(body, changed),
        E::While(_, c, b) => {
            walk(c, changed);
            walk(b, changed);
        }
        E::IfElse(c, t, alt) => {
            walk(c, changed);
            walk(t, changed);
            if let Some(a) = alt.as_mut().as_mut() {
                walk(a, changed);
            }
        }
        E::Switch(c, _, arms) => {
            walk(c, changed);
            for (_, b) in arms.iter_mut() {
                walk(b, changed);
            }
        }
        E::Match(c, _, arms) => {
            walk(c, changed);
            for (_, _, b) in arms.iter_mut() {
                walk(b, changed);
            }
        }
        E::MatchLit(s, arms) => {
            walk(s, changed);
            for (_, b) in arms.iter_mut() {
                walk(b, changed);
            }
        }
        E::Return(items) | E::Call(_, items) => {
            for item in items.iter_mut() {
                walk(item, changed);
            }
        }
        E::Primitive { args, .. } | E::Data { args, .. } => {
            for a in args.iter_mut() {
                walk(a, changed);
            }
        }
        E::Assign(_, e)
        | E::LetBind(_, e)
        | E::Abort(e)
        | E::Borrow(_, e)
        | E::Unpack(_, _, e)
        | E::UnpackVariant(_, _, _, e)
        | E::VecUnpack(_, e) => walk(e, changed),
        E::Unstructured(nodes) => {
            for n in nodes.iter_mut() {
                match n {
                    UnstructuredNode::Labeled(_, body) | UnstructuredNode::Statement(body) => {
                        walk(body, changed);
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

/// Pop a trailing `Unstructured(Goto(target))` from `exp`'s tail positions. Returns `true`
/// if anything was popped. Drops empty `Unstructured` so flatten/refinements can clean up.
fn strip_tail_goto_to(exp: &mut Exp, target: u64) -> bool {
    use Exp as E;
    match exp {
        E::Unstructured(nodes) => {
            if matches!(nodes.last(), Some(UnstructuredNode::Goto(n)) if *n == target) {
                nodes.pop();
                if nodes.is_empty() {
                    *exp = E::Seq(vec![]);
                }
                true
            } else {
                false
            }
        }
        E::Seq(items) => {
            let Some(last) = items.last_mut() else {
                return false;
            };
            let popped = strip_tail_goto_to(last, target);
            if popped && matches!(items.last(), Some(E::Seq(v)) if v.is_empty()) {
                items.pop();
            }
            popped
        }
        E::IfElse(_, t, alt) => {
            let ct = strip_tail_goto_to(t, target);
            let ce = alt
                .as_mut()
                .as_mut()
                .is_some_and(|a| strip_tail_goto_to(a, target));
            ct || ce
        }
        E::Switch(_, _, arms) => {
            let mut popped = false;
            for (_, body) in arms.iter_mut() {
                popped |= strip_tail_goto_to(body, target);
            }
            popped
        }
        E::Match(_, _, arms) => {
            let mut popped = false;
            for (_, _, body) in arms.iter_mut() {
                popped |= strip_tail_goto_to(body, target);
            }
            popped
        }
        E::MatchLit(_, arms) => {
            let mut popped = false;
            for (_, body) in arms.iter_mut() {
                popped |= strip_tail_goto_to(body, target);
            }
            popped
        }
        E::Block(_, body) => strip_tail_goto_to(body, target),
        _ => false,
    }
}
