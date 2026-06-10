// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Flag elimination + `||`-fold. Recovers `if (stale_a || stale_b || …) { X }` from the
//! structured form the reaching-condition structurer leaves:
//!
//! ```text
//!   let f = true;
//!   <setup_a>
//!   if (cond_a) { f = false } else { <setup_b>; if (cond_b) { f = false } else { … } };
//!   if (f) { T } else { E }
//! ```
//!
//! `f` is a flag set `false` exactly on the "stale" paths and read once at the trailing
//! `if (f)`, so its final value is `¬(cond_a ∨ cond_b ∨ …)` and the test becomes
//! `if (cond_a ∨ cond_b ∨ …) { E } else { T }`. Each later condition's setup (reused
//! per-feed locals like `get_price_unsafe`) rides into its `||` operand as a block
//! expression, preserving the original short-circuit — a later feed's price is only fetched
//! when the earlier ones are fresh. The flag's declaration and assignments drop out.

use move_core_types::runtime_value::MoveValue as Value;
use move_stackless_bytecode_2::ast::PrimitiveOp;

use crate::{ast::Exp, refinement::Refine};

pub fn refine(exp: &mut Exp) -> bool {
    RecoverFlag.refine(exp)
}

struct RecoverFlag;

impl Refine for RecoverFlag {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::Seq(items) = exp else {
            return false;
        };
        try_recover(items)
    }
}

fn try_recover(items: &mut Vec<Exp>) -> bool {
    // Trailing `if (Variable(f)) { T } else { E }`, with the flag-setting structure as the
    // immediately preceding sibling and `let f = true` somewhere earlier in this Seq.
    for test_idx in 1..items.len() {
        let Some(flag) = flag_test_var(&items[test_idx]) else {
            continue;
        };
        let structure_idx = test_idx - 1;
        let Some(ops) = collect_operands(&items[structure_idx], &flag) else {
            continue;
        };
        let Some(init_idx) = (0..structure_idx).find(|&i| is_flag_init(&items[i], &flag)) else {
            continue;
        };

        // Pull the test apart: `f` false ⇒ run the else; `f` true ⇒ run the then.
        let Exp::IfElse(_, then_t, else_t) = items[test_idx].clone() else {
            continue;
        };
        let Some(f_false_branch) = *else_t else {
            continue;
        };
        let cond = or_fold(ops);
        if !flag_fully_consumed(
            items,
            &flag,
            init_idx,
            structure_idx,
            test_idx,
            &cond,
            &then_t,
            &f_false_branch,
        ) {
            continue;
        }
        let recovered = Exp::IfElse(
            Box::new(cond),
            Box::new(f_false_branch),     // f == false branch
            Box::new(non_empty(*then_t)), // f == true branch
        );

        // Replace [structure, test] with the recovered `if`, then drop the flag's declaration.
        items.splice(structure_idx..=test_idx, std::iter::once(recovered));
        items.remove(init_idx);
        return true;
    }
    false
}

/// True iff dropping the flag (the `let f = …` at `init_idx`) would not leave any
/// dangling read of `f`. Three places where a leftover read could hide:
///
///   1. The recovered condition `cond` (an op_setup that reused `f`).
///   2. The recovered then/else arms (an arm reads `f` after being set).
///   3. Any sibling of the canonical `init / structure / test` shape: items strictly
///      between `init_idx` and `structure_idx` (intervening blocks that touch `f`),
///      or items after `test_idx` (a later block reads `f` once the canonical chain
///      has run).
///
/// Each case is its own short-circuit so a failure is grep-able to the responsible shape.
fn flag_fully_consumed(
    items: &[Exp],
    flag: &str,
    init_idx: usize,
    structure_idx: usize,
    test_idx: usize,
    cond: &Exp,
    then_t: &Exp,
    f_false_branch: &Exp,
) -> bool {
    if mentions(cond, flag) || mentions(then_t, flag) || mentions(f_false_branch, flag) {
        return false;
    }
    let intervening = items[init_idx + 1..structure_idx]
        .iter()
        .any(|e| mentions(e, flag));
    if intervening {
        return false;
    }
    let trailing = items
        .get(test_idx + 1..)
        .into_iter()
        .flatten()
        .any(|e| mentions(e, flag));
    if trailing {
        return false;
    }
    true
}

/// `Some(f)` iff `exp` is `if (Variable(f)) { _ } else { _ }`.
fn flag_test_var(exp: &Exp) -> Option<String> {
    let Exp::IfElse(cond, _, alt) = exp else {
        return None;
    };
    alt.as_ref().as_ref()?;
    match &**cond {
        Exp::Variable(name) => Some(name.clone()),
        _ => None,
    }
}

/// `Some(operands)` iff `exp` is the nested flag-setting structure: a chain of
/// `if (cond) { f = false } else { <setup>; <next> }`. Each operand is a condition, with any
/// preceding setup folded in as a leading block expression.
fn collect_operands(exp: &Exp, flag: &str) -> Option<Vec<Exp>> {
    let Exp::IfElse(cond, then, alt) = exp else {
        return None;
    };
    if !sets_flag_false(then, flag) {
        return None;
    }
    let mut ops = vec![(**cond).clone()];
    if let Some(else_branch) = alt.as_ref().as_ref() {
        ops.extend(operands_from_else(else_branch, flag)?);
    }
    Some(ops)
}

fn operands_from_else(exp: &Exp, flag: &str) -> Option<Vec<Exp>> {
    match exp {
        Exp::Seq(items) if items.is_empty() => Some(vec![]),
        Exp::Seq(items) => {
            let (last, setup) = items.split_last().unwrap();
            let inner = collect_operands(last, flag)?;
            let (first, rest) = inner.split_first()?;
            let first = if setup.is_empty() {
                first.clone()
            } else {
                let mut block: Vec<Exp> = setup.to_vec();
                block.push(first.clone());
                Exp::Seq(block)
            };
            Some(std::iter::once(first).chain(rest.iter().cloned()).collect())
        }
        Exp::IfElse(..) => collect_operands(exp, flag),
        // The fresh fall-through (no further flag assignment): no more operands.
        _ => Some(vec![]),
    }
}

/// True iff `exp`'s only observable effect is `flag = false` (modulo empty `Block`/`Seq`
/// wrappers the structurer may leave around the assignment).
fn sets_flag_false(exp: &Exp, flag: &str) -> bool {
    let mut effects = Vec::new();
    flatten_effects(exp, &mut effects);
    matches!(
        effects.as_slice(),
        [Exp::Assign(vars, val)]
            if vars.len() == 1 && vars[0] == flag && matches!(&**val, Exp::Value(Value::Bool(false)))
    )
}

fn flatten_effects<'a>(exp: &'a Exp, out: &mut Vec<&'a Exp>) {
    match exp {
        Exp::Seq(items) => items.iter().for_each(|i| flatten_effects(i, out)),
        Exp::Block(_, body) => flatten_effects(body, out),
        other => out.push(other),
    }
}

fn is_flag_init(exp: &Exp, flag: &str) -> bool {
    matches!(
        exp,
        Exp::LetBind(vars, val) | Exp::Assign(vars, val)
            if vars.len() == 1 && vars[0] == flag && matches!(&**val, Exp::Value(Value::Bool(true)))
    )
}

fn or_fold(ops: Vec<Exp>) -> Exp {
    ops.into_iter()
        .reduce(|a, b| Exp::Primitive {
            op: PrimitiveOp::Or,
            args: vec![a, b],
        })
        .expect("collect_operands yields at least one operand")
}

fn non_empty(exp: Exp) -> Option<Exp> {
    match exp {
        Exp::Seq(items) if items.is_empty() => None,
        other => Some(other),
    }
}

fn mentions(exp: &Exp, name: &str) -> bool {
    let mut found = false;
    walk(exp, &mut |e| {
        if let Exp::Variable(v) = e
            && v == name
        {
            found = true;
        }
    });
    found
}

fn walk(exp: &Exp, f: &mut impl FnMut(&Exp)) {
    use crate::ast::UnstructuredNode;
    f(exp);
    match exp {
        Exp::Loop(_, b)
        | Exp::Block(_, b)
        | Exp::Assign(_, b)
        | Exp::LetBind(_, b)
        | Exp::Abort(b)
        | Exp::Borrow(_, b)
        | Exp::Unpack(_, _, b)
        | Exp::UnpackVariant(_, _, _, b)
        | Exp::VecUnpack(_, b) => walk(b, f),
        Exp::While(_, c, b) => {
            walk(c, f);
            walk(b, f);
        }
        Exp::IfElse(c, t, alt) => {
            walk(c, f);
            walk(t, f);
            if let Some(a) = alt.as_ref().as_ref() {
                walk(a, f);
            }
        }
        Exp::Switch(c, _, arms) => {
            walk(c, f);
            arms.iter().for_each(|(_, e)| walk(e, f));
        }
        Exp::Match(c, _, arms) => {
            walk(c, f);
            arms.iter().for_each(|(_, _, e)| walk(e, f));
        }
        Exp::MatchLit(c, arms) => {
            walk(c, f);
            arms.iter().for_each(|(_, e)| walk(e, f));
        }
        Exp::Seq(es) | Exp::Return(es) | Exp::Call(_, es) => es.iter().for_each(|e| walk(e, f)),
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
            args.iter().for_each(|e| walk(e, f))
        }
        Exp::Unstructured(nodes) => {
            for node in nodes {
                if let UnstructuredNode::Labeled(_, b) | UnstructuredNode::Statement(b) = node {
                    walk(b, f);
                }
            }
        }
        Exp::Break(_)
        | Exp::Continue(_)
        | Exp::Declare(_)
        | Exp::Value(_)
        | Exp::Variable(_)
        | Exp::Constant(_) => {}
    }
}
