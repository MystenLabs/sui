// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::ast::{Exp, Module, ModuleRef, TypeRef};

use move_binary_format::normalized::ModuleId;
use move_symbol_pool::Symbol;

use std::collections::{BTreeMap, BTreeSet};

// -------------------------------------------------------------------------------------------------
// Module-level refinement
//
// Walk every function body in `module` and decide two things:
//   1. Which `ModuleId`s deserve a top-level `use 0xADDR::module;` declaration (module-level
//      aliasing).
//   2. Which `(ModuleId, type_name)` pairs deserve a top-level `use 0xADDR::module::Type;`
//      declaration (type-level aliasing).
//
// Type aliasing wins where both apply: a struct or enum imported directly renders as just
// `Type`, with no module prefix to alias separately. Modules then get aliased only for
// references that survive the type-aliasing pass (function calls, plus any types we didn't
// import directly).
//
// Both kinds of aliases share the same `used` set (locals + own declarations of the
// surrounding module) and the same picking algorithm: shortest non-conflicting alias, fall
// back to `x<prefix>_<name>` with the shortest disambiguating run of the stripped-leading-
// zeros hex address when collisions force it.

pub fn collect_uses(module: &mut Module, current_mid: ModuleId<Symbol>, used: &BTreeSet<Symbol>) {
    // Self-references render bare without a `use` declaration: structs and enums declared in
    // this module are already in scope under their bare names. Rewrite them first so the
    // counting/aliasing pass below only sees genuine external references.
    for fun in module.functions.values_mut() {
        rewrite_self_types(&mut fun.code, current_mid);
    }

    let type_counts = count_type_refs(module);
    let mut used_now = used.clone();
    let type_uses = pick_aliases(
        type_counts.iter().map(|(k, v)| (*k, *v)).collect(),
        |k: &(ModuleId<Symbol>, Symbol)| k.1,
        |k: &(ModuleId<Symbol>, Symbol)| k.0,
        &mut used_now,
    );

    let module_counts = count_module_refs(module, &type_uses);
    let uses = pick_aliases(
        module_counts.iter().map(|(k, v)| (*k, *v)).collect(),
        |mid: &ModuleId<Symbol>| mid.name,
        |mid: &ModuleId<Symbol>| *mid,
        &mut used_now,
    );

    if uses.is_empty() && type_uses.is_empty() {
        return;
    }

    for fun in module.functions.values_mut() {
        rewrite(&mut fun.code, &uses, &type_uses);
    }
    module.uses = uses;
    module.type_uses = type_uses;
}

/// Rewrite every `TypeRef::Qualified(_, name)` whose module is `current_mid` to
/// `TypeRef::Aliased(name)`. Self-declared structs and enums are already in scope under
/// their bare names; emitting a `use` declaration for them would be redundant (and, when the
/// name collides with the local declaration, generate a confusing alias).
fn rewrite_self_types(exp: &mut Exp, current_mid: ModuleId<Symbol>) {
    let unself = |t: &mut TypeRef| {
        if let TypeRef::Qualified(ModuleRef::Qualified(mid), name) = t
            && *mid == current_mid
        {
            *t = TypeRef::Aliased(*name);
        }
    };
    match exp {
        Exp::Switch(subject, t, arms) => {
            unself(t);
            rewrite_self_types(subject, current_mid);
            for (_, body) in arms {
                rewrite_self_types(body, current_mid);
            }
        }
        Exp::Match(subject, t, arms) => {
            unself(t);
            rewrite_self_types(subject, current_mid);
            for (_, _, body) in arms {
                rewrite_self_types(body, current_mid);
            }
        }
        Exp::Unpack(t, _, e) => {
            unself(t);
            rewrite_self_types(e, current_mid);
        }
        Exp::UnpackVariant(_, (t, _), _, e) => {
            unself(t);
            rewrite_self_types(e, current_mid);
        }
        Exp::Call(_, args) => {
            for a in args {
                rewrite_self_types(a, current_mid);
            }
        }
        Exp::Seq(items) | Exp::Return(items) => {
            for i in items {
                rewrite_self_types(i, current_mid);
            }
        }
        Exp::IfElse(c, t, alt) => {
            rewrite_self_types(c, current_mid);
            rewrite_self_types(t, current_mid);
            if let Some(a) = alt.as_mut().as_mut() {
                rewrite_self_types(a, current_mid);
            }
        }
        Exp::Loop(_, b) => rewrite_self_types(b, current_mid),
        Exp::While(_, c, b) => {
            rewrite_self_types(c, current_mid);
            rewrite_self_types(b, current_mid);
        }
        Exp::Assign(_, e)
        | Exp::LetBind(_, e)
        | Exp::Abort(e)
        | Exp::Borrow(_, e)
        | Exp::VecUnpack(_, e) => rewrite_self_types(e, current_mid),
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
            for a in args {
                rewrite_self_types(a, current_mid);
            }
        }
        Exp::Break(_)
        | Exp::Continue(_)
        | Exp::Declare(_)
        | Exp::Value(_)
        | Exp::Variable(_)
        | Exp::Constant(_)
        | Exp::Unstructured(_) => {}
    }
}

// -------------------------------------------------------------------------------------------------
// Alias selection

/// Pick a short alias per key in `counts`, grouping by `group_key` (the alias's short name),
/// breaking ties by reference count desc then by `mid_for(...)` address. `used` grows with
/// each pick so successor calls (e.g. modules-after-types) honor every alias already taken.
fn pick_aliases<K, FGroup, FMid>(
    counts: Vec<(K, usize)>,
    group_key: FGroup,
    mid_for: FMid,
    used: &mut BTreeSet<Symbol>,
) -> BTreeMap<K, Symbol>
where
    K: Copy + Ord,
    FGroup: Fn(&K) -> Symbol,
    FMid: Fn(&K) -> ModuleId<Symbol>,
{
    let mut groups: BTreeMap<Symbol, Vec<(K, usize)>> = BTreeMap::new();
    for (k, n) in counts {
        groups.entry(group_key(&k)).or_default().push((k, n));
    }
    let mut out: BTreeMap<K, Symbol> = BTreeMap::new();
    for (name, mut members) in groups {
        members.sort_by(|(a, na), (b, nb)| {
            nb.cmp(na)
                .then_with(|| mid_for(a).address.cmp(&mid_for(b).address))
        });
        let stripped: Vec<String> = members
            .iter()
            .map(|(k, _)| strip_leading_zeros_hex(&mid_for(k)))
            .collect();
        for (i, (k, _)) in members.iter().enumerate() {
            if let Some(alias) = shortest_alias(name, i, &stripped, used) {
                out.insert(*k, alias);
                used.insert(alias);
            }
        }
    }
    out
}

/// Try the bare `name` first; fall back to `x<prefix>_<name>` with the shortest
/// disambiguating prefix of `stripped[i]`. Extend the prefix until clear of `used`. Returns
/// `None` only when no candidate fits (pathological, unreachable for compiled bytecode).
fn shortest_alias(
    name: Symbol,
    i: usize,
    stripped: &[String],
    used: &BTreeSet<Symbol>,
) -> Option<Symbol> {
    if !used.contains(&name) {
        return Some(name);
    }
    let my = stripped[i].as_str();
    let min_len = min_distinguishing_prefix_len(i, stripped).max(1);
    for len in min_len..=my.len() {
        let candidate: Symbol = format!("x{}_{}", &my[..len], name).into();
        if !used.contains(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// The smallest k such that `stripped[i][..k]` differs from every other element of
/// `stripped`. Computes per-other-pair divergence and takes the max.
fn min_distinguishing_prefix_len(i: usize, stripped: &[String]) -> usize {
    let me = stripped[i].as_str();
    let mut min_len = 0;
    for (j, other) in stripped.iter().enumerate() {
        if i == j {
            continue;
        }
        min_len = min_len.max(distinguishing_prefix_len(me, other));
    }
    min_len
}

/// Smallest k such that `me[..k] != other[..k]` (with appropriate handling when one is a
/// prefix of the other). Distinct strings always have such a k.
fn distinguishing_prefix_len(me: &str, other: &str) -> usize {
    let me_b = me.as_bytes();
    let other_b = other.as_bytes();
    for i in 0..me_b.len().min(other_b.len()) {
        if me_b[i] != other_b[i] {
            return i + 1;
        }
    }
    if me_b.len() <= other_b.len() {
        me_b.len()
    } else {
        other_b.len() + 1
    }
}

/// Render `mid.address` as lowercase hex with leading zeros stripped. The all-zeros address
/// (theoretical only) maps to `"0"` so callers always have a non-empty prefix string.
fn strip_leading_zeros_hex(mid: &ModuleId<Symbol>) -> String {
    let s = format!("{:x}", mid.address);
    let trimmed = s.trim_start_matches('0');
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

// -------------------------------------------------------------------------------------------------
// Local-name collection (taboo input to `collect_uses`)

/// Names introduced inside `module`'s function bodies: every `LetBind`, `Declare`, `Assign`
/// LHS, `Unpack`/`UnpackVariant`/`VecUnpack` binder, and `Variable` read. Conservative -
/// over-approximation is harmless (an alias we could have used stays fully qualified, never
/// the other way around).
pub fn collect_local_names(module: &Module) -> BTreeSet<Symbol> {
    let mut out = BTreeSet::new();
    for fun in module.functions.values() {
        collect_local_names_exp(&fun.code, &mut out);
    }
    out
}

fn collect_local_names_exp(exp: &Exp, out: &mut BTreeSet<Symbol>) {
    match exp {
        Exp::LetBind(names, e) | Exp::Assign(names, e) => {
            for n in names {
                out.insert(Symbol::from(n.as_str()));
            }
            collect_local_names_exp(e, out);
        }
        Exp::Declare(names) => {
            for n in names {
                out.insert(Symbol::from(n.as_str()));
            }
        }
        Exp::Variable(n) => {
            out.insert(Symbol::from(n.as_str()));
        }
        Exp::Unpack(_, fields, e) | Exp::UnpackVariant(_, _, fields, e) => {
            for (_, n) in fields {
                out.insert(Symbol::from(n.as_str()));
            }
            collect_local_names_exp(e, out);
        }
        Exp::VecUnpack(names, e) => {
            for n in names {
                out.insert(Symbol::from(n.as_str()));
            }
            collect_local_names_exp(e, out);
        }
        Exp::Seq(items) | Exp::Return(items) | Exp::Call(_, items) => {
            for i in items {
                collect_local_names_exp(i, out);
            }
        }
        Exp::IfElse(c, t, alt) => {
            collect_local_names_exp(c, out);
            collect_local_names_exp(t, out);
            if let Some(a) = alt.as_ref().as_ref() {
                collect_local_names_exp(a, out);
            }
        }
        Exp::Switch(c, _, arms) => {
            collect_local_names_exp(c, out);
            for (_, body) in arms {
                collect_local_names_exp(body, out);
            }
        }
        Exp::Match(c, _, arms) => {
            collect_local_names_exp(c, out);
            for (_, fields, body) in arms {
                for (_, n) in fields {
                    out.insert(Symbol::from(n.as_str()));
                }
                collect_local_names_exp(body, out);
            }
        }
        Exp::Loop(_, b) => collect_local_names_exp(b, out),
        Exp::While(_, c, b) => {
            collect_local_names_exp(c, out);
            collect_local_names_exp(b, out);
        }
        Exp::Abort(e) | Exp::Borrow(_, e) => collect_local_names_exp(e, out),
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
            for a in args {
                collect_local_names_exp(a, out);
            }
        }
        Exp::Break(_) | Exp::Continue(_) | Exp::Value(_) | Exp::Constant(_) => {}
        Exp::Unstructured(nodes) => {
            for node in nodes {
                match node {
                    crate::ast::UnstructuredNode::Labeled(_, body)
                    | crate::ast::UnstructuredNode::Statement(body) => {
                        collect_local_names_exp(body, out);
                    }
                    crate::ast::UnstructuredNode::Goto(_) => {}
                }
            }
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Reference counting

/// Pull `(ModuleId, type_name)` out of a `TypeRef`, ignoring already-aliased ones.
fn type_qualified(t: &TypeRef) -> Option<(ModuleId<Symbol>, Symbol)> {
    match t {
        TypeRef::Qualified(ModuleRef::Qualified(mid), name) => Some((*mid, *name)),
        _ => None,
    }
}

/// Count references to each `(ModuleId, type_name)` across every function body.
fn count_type_refs(module: &Module) -> BTreeMap<(ModuleId<Symbol>, Symbol), usize> {
    let mut out = BTreeMap::new();
    for fun in module.functions.values() {
        count_type_refs_exp(&fun.code, &mut out);
    }
    out
}

fn count_type_refs_exp(exp: &Exp, out: &mut BTreeMap<(ModuleId<Symbol>, Symbol), usize>) {
    let note = |t: &TypeRef, out: &mut BTreeMap<_, _>| {
        if let Some(k) = type_qualified(t) {
            *out.entry(k).or_insert(0) += 1;
        }
    };
    match exp {
        Exp::Switch(subject, t, arms) => {
            note(t, out);
            count_type_refs_exp(subject, out);
            for (_, body) in arms {
                count_type_refs_exp(body, out);
            }
        }
        Exp::Match(subject, t, arms) => {
            note(t, out);
            count_type_refs_exp(subject, out);
            for (_, _, body) in arms {
                count_type_refs_exp(body, out);
            }
        }
        Exp::Unpack(t, _, e) => {
            note(t, out);
            count_type_refs_exp(e, out);
        }
        Exp::UnpackVariant(_, (t, _), _, e) => {
            note(t, out);
            count_type_refs_exp(e, out);
        }
        Exp::Call(_, args) => {
            for a in args {
                count_type_refs_exp(a, out);
            }
        }
        Exp::Seq(items) | Exp::Return(items) => {
            for i in items {
                count_type_refs_exp(i, out);
            }
        }
        Exp::IfElse(c, t, alt) => {
            count_type_refs_exp(c, out);
            count_type_refs_exp(t, out);
            if let Some(a) = alt.as_ref().as_ref() {
                count_type_refs_exp(a, out);
            }
        }
        Exp::Loop(_, b) => count_type_refs_exp(b, out),
        Exp::While(_, c, b) => {
            count_type_refs_exp(c, out);
            count_type_refs_exp(b, out);
        }
        Exp::Assign(_, e)
        | Exp::LetBind(_, e)
        | Exp::Abort(e)
        | Exp::Borrow(_, e)
        | Exp::VecUnpack(_, e) => count_type_refs_exp(e, out),
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
            for a in args {
                count_type_refs_exp(a, out);
            }
        }
        Exp::Break(_)
        | Exp::Continue(_)
        | Exp::Declare(_)
        | Exp::Value(_)
        | Exp::Variable(_)
        | Exp::Constant(_)
        | Exp::Unstructured(_) => {}
    }
}

/// Count references to each `ModuleId` across every function body, *excluding* references
/// that are about to disappear because their containing `(mid, type_name)` is in `type_uses`
/// (after type-aliasing, those sites don't render the module prefix at all).
fn count_module_refs(
    module: &Module,
    type_uses: &BTreeMap<(ModuleId<Symbol>, Symbol), Symbol>,
) -> BTreeMap<ModuleId<Symbol>, usize> {
    let mut out = BTreeMap::new();
    for fun in module.functions.values() {
        count_module_refs_exp(&fun.code, type_uses, &mut out);
    }
    out
}

fn count_module_refs_exp(
    exp: &Exp,
    type_uses: &BTreeMap<(ModuleId<Symbol>, Symbol), Symbol>,
    out: &mut BTreeMap<ModuleId<Symbol>, usize>,
) {
    let note_module = |m: &ModuleRef, out: &mut BTreeMap<_, _>| {
        if let ModuleRef::Qualified(mid) = m {
            *out.entry(*mid).or_insert(0) += 1;
        }
    };
    let note_type_module = |t: &TypeRef, out: &mut BTreeMap<_, _>| {
        if let Some(k) = type_qualified(t)
            && !type_uses.contains_key(&k)
        {
            *out.entry(k.0).or_insert(0) += 1;
        }
    };
    match exp {
        Exp::Call((m, _), args) => {
            note_module(m, out);
            for a in args {
                count_module_refs_exp(a, type_uses, out);
            }
        }
        Exp::Switch(subject, t, arms) => {
            note_type_module(t, out);
            count_module_refs_exp(subject, type_uses, out);
            for (_, body) in arms {
                count_module_refs_exp(body, type_uses, out);
            }
        }
        Exp::Match(subject, t, arms) => {
            note_type_module(t, out);
            count_module_refs_exp(subject, type_uses, out);
            for (_, _, body) in arms {
                count_module_refs_exp(body, type_uses, out);
            }
        }
        Exp::Unpack(t, _, e) => {
            note_type_module(t, out);
            count_module_refs_exp(e, type_uses, out);
        }
        Exp::UnpackVariant(_, (t, _), _, e) => {
            note_type_module(t, out);
            count_module_refs_exp(e, type_uses, out);
        }
        Exp::Seq(items) | Exp::Return(items) => {
            for i in items {
                count_module_refs_exp(i, type_uses, out);
            }
        }
        Exp::IfElse(c, t, alt) => {
            count_module_refs_exp(c, type_uses, out);
            count_module_refs_exp(t, type_uses, out);
            if let Some(a) = alt.as_ref().as_ref() {
                count_module_refs_exp(a, type_uses, out);
            }
        }
        Exp::Loop(_, b) => count_module_refs_exp(b, type_uses, out),
        Exp::While(_, c, b) => {
            count_module_refs_exp(c, type_uses, out);
            count_module_refs_exp(b, type_uses, out);
        }
        Exp::Assign(_, e)
        | Exp::LetBind(_, e)
        | Exp::Abort(e)
        | Exp::Borrow(_, e)
        | Exp::VecUnpack(_, e) => count_module_refs_exp(e, type_uses, out),
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
            for a in args {
                count_module_refs_exp(a, type_uses, out);
            }
        }
        Exp::Break(_)
        | Exp::Continue(_)
        | Exp::Declare(_)
        | Exp::Value(_)
        | Exp::Variable(_)
        | Exp::Constant(_)
        | Exp::Unstructured(_) => {}
    }
}

// -------------------------------------------------------------------------------------------------
// Rewrite

/// Rewrite each `TypeRef::Qualified(_, name)` whose `(mid, name)` is in `type_uses` to
/// `TypeRef::Aliased(...)`, and each `ModuleRef::Qualified(mid)` whose `mid` is in `uses` to
/// `ModuleRef::Aliased(...)`. Type-aliasing takes precedence: a type-aliased site never
/// renders the module prefix, so the inner `ModuleRef` becomes irrelevant.
fn rewrite(
    exp: &mut Exp,
    uses: &BTreeMap<ModuleId<Symbol>, Symbol>,
    type_uses: &BTreeMap<(ModuleId<Symbol>, Symbol), Symbol>,
) {
    match exp {
        Exp::Call((m, _), args) => {
            alias_module(m, uses);
            for a in args {
                rewrite(a, uses, type_uses);
            }
        }
        Exp::Switch(subject, t, arms) => {
            alias_type(t, uses, type_uses);
            rewrite(subject, uses, type_uses);
            for (_, body) in arms {
                rewrite(body, uses, type_uses);
            }
        }
        Exp::Match(subject, t, arms) => {
            alias_type(t, uses, type_uses);
            rewrite(subject, uses, type_uses);
            for (_, _, body) in arms {
                rewrite(body, uses, type_uses);
            }
        }
        Exp::Unpack(t, _, e) => {
            alias_type(t, uses, type_uses);
            rewrite(e, uses, type_uses);
        }
        Exp::UnpackVariant(_, (t, _), _, e) => {
            alias_type(t, uses, type_uses);
            rewrite(e, uses, type_uses);
        }
        Exp::Seq(items) | Exp::Return(items) => {
            for i in items {
                rewrite(i, uses, type_uses);
            }
        }
        Exp::IfElse(c, t, alt) => {
            rewrite(c, uses, type_uses);
            rewrite(t, uses, type_uses);
            if let Some(a) = alt.as_mut().as_mut() {
                rewrite(a, uses, type_uses);
            }
        }
        Exp::Loop(_, b) => rewrite(b, uses, type_uses),
        Exp::While(_, c, b) => {
            rewrite(c, uses, type_uses);
            rewrite(b, uses, type_uses);
        }
        Exp::Assign(_, e)
        | Exp::LetBind(_, e)
        | Exp::Abort(e)
        | Exp::Borrow(_, e)
        | Exp::VecUnpack(_, e) => rewrite(e, uses, type_uses),
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
            for a in args {
                rewrite(a, uses, type_uses);
            }
        }
        Exp::Break(_)
        | Exp::Continue(_)
        | Exp::Declare(_)
        | Exp::Value(_)
        | Exp::Variable(_)
        | Exp::Constant(_)
        | Exp::Unstructured(_) => {}
    }
}

fn alias_module(m: &mut ModuleRef, uses: &BTreeMap<ModuleId<Symbol>, Symbol>) {
    if let ModuleRef::Qualified(mid) = m
        && let Some(name) = uses.get(mid)
    {
        *m = ModuleRef::Aliased(*name);
    }
}

fn alias_type(
    t: &mut TypeRef,
    uses: &BTreeMap<ModuleId<Symbol>, Symbol>,
    type_uses: &BTreeMap<(ModuleId<Symbol>, Symbol), Symbol>,
) {
    if let Some(k) = type_qualified(t)
        && let Some(alias) = type_uses.get(&k)
    {
        *t = TypeRef::Aliased(*alias);
        return;
    }
    if let TypeRef::Qualified(m, _) = t {
        alias_module(m, uses);
    }
}
