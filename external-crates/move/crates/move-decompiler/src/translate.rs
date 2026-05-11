// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ast as Out,
    config::{Config, print_heading},
    structuring::{
        ast::{self as D},
        term_reconstruction,
    },
};

use crate::ast::Exp;
use move_model_2::{model::Model, source_kind::SourceKind};
use move_stackless_bytecode_2::ast as SB;
use move_symbol_pool::Symbol;
use std::collections::{BTreeMap, HashSet};

// -------------------------------------------------------------------------------------------------
// Entry
// -------------------------------------------------------------------------------------------------

pub fn model<S: SourceKind>(model: Model<S>) -> anyhow::Result<Out::Decompiled<S>> {
    let config = Config::default();
    model_with_config(&config, model)
}

pub fn model_with_config<S: SourceKind>(
    config: &Config,
    model: Model<S>,
) -> anyhow::Result<Out::Decompiled<S>> {
    // Don't optimize the stackless bytecode: we want to faithfully decompile what the
    // bytecode actually contains.
    let stackless = move_stackless_bytecode_2::from_model(&model, /* optimize */ false)?;
    let packages = packages(config, &model, stackless);
    Ok(Out::Decompiled { model, packages })
}

fn packages<S: SourceKind>(
    config: &Config,
    model: &Model<S>,
    stackless: SB::StacklessBytecode,
) -> Vec<Out::Package> {
    let SB::StacklessBytecode {
        packages: sb_packages,
    } = stackless;

    sb_packages
        .into_iter()
        .map(|pkg| package(config, model, pkg))
        .collect()
}

fn package<S: SourceKind>(config: &Config, _model: &Model<S>, sb_pkg: SB::Package) -> Out::Package {
    let SB::Package {
        name,
        address,
        modules,
    } = sb_pkg;
    let modules = modules
        .into_iter()
        .map(|(module_name, m)| {
            let decompiled_module = module(config, m);
            (module_name, decompiled_module)
        })
        .collect();
    Out::Package {
        name,
        address,
        modules,
    }
}

// -------------------------------------------------------------------------------------------------
// Module
// -------------------------------------------------------------------------------------------------

pub fn module(config: &Config, module: SB::Module) -> Out::Module {
    let SB::Module { name, functions } = module;

    let functions = functions
        .into_iter()
        .map(|(name, fun)| (name, function(config, fun)))
        .collect();

    Out::Module { name, functions }
}

// -------------------------------------------------------------------------------------------------
// Function
// -------------------------------------------------------------------------------------------------

fn function(config: &Config, fun: SB::Function) -> Out::Function {
    if config.debug_print.print_function_heading() {
        println!("DECOMPILING FUNCTION {}", fun.name);
    }
    if config.debug_print.stackless {
        print_heading("stackless bytecode");
        for (lbl, blk) in &fun.basic_blocks {
            println!("Block {}:\n{blk}", lbl);
        }
    }
    let (name, terms, input, entry) = make_input(fun);
    if config.debug_print.input {
        print_heading("input");
        println!("{input:?}");
    }
    let structured = crate::structuring::structure(config, input, entry);
    if config.debug_print.structured {
        print_heading("structured");
        println!("{}", structured.to_test_string());
    }
    let mut code = generate_output(terms, structured);
    let mut scope = HashSet::new();
    hoist_declarations(&mut code, &mut scope);
    crate::refinement::refine(&mut code);
    if config.debug_print.decompiled_code {
        print_heading("refined code");
        println!("{code}");
    }
    Out::Function { name, code }
}

fn make_input(
    fun: SB::Function,
) -> (
    Symbol,
    BTreeMap<D::Label, Out::Exp>,
    BTreeMap<D::Label, D::Input>,
    D::Label,
) {
    let SB::Function {
        name,
        entry_label,
        basic_blocks,
    } = fun;

    let mut terms = BTreeMap::new();
    let mut input = BTreeMap::new();

    let blocks_iter = basic_blocks.iter();
    let mut next_blocks_iter = basic_blocks.iter().skip(1);

    for (lbl, block) in blocks_iter {
        let label = lbl;
        assert!(
            *label == block.label,
            "Block label mismatch: {label} != {}",
            block.label
        );
        let next_block_label = if let Some((nxt_lbl, _)) = next_blocks_iter.next() {
            Some(*nxt_lbl)
        } else {
            None
        };
        // Per-block: the block's first StoreLoc of each local emits `let X = e`, the rest
        // `X = e`. Cross-block coordination — hoisting `let X;` out of arm scopes when X is
        // shared with siblings or seen by later items — happens later in `hoist_declarations`.
        let mut let_binds: HashSet<SB::RegId> = HashSet::new();
        let blk_terms = generate_term_block(block, &mut let_binds);
        let blk_input = extract_input(block, next_block_label);

        terms.insert((*label as u32).into(), blk_terms);
        input.insert((*label as u32).into(), blk_input);
    }
    (name, terms, input, (entry_label as u32).into())
}

fn generate_term_block(block: &SB::BasicBlock, let_binds: &mut HashSet<SB::RegId>) -> Out::Exp {
    // remove the last jump / replace the conditional with just the "triv" in it
    term_reconstruction::exp(block.clone(), let_binds)
}

fn extract_input(block: &SB::BasicBlock, next_block_label: Option<SB::Label>) -> D::Input {
    use D::Input as DI;
    use SB::Instruction as SI;

    // Look at the last instruction to determine control flow
    if let Some(last_instr) = block.instructions.last() {
        match last_instr {
            SI::Jump(label) => DI::Code(
                (block.label as u32).into(),
                (block.label as u32).into(),
                Some((*label as u32).into()),
            ),
            SI::JumpIf {
                condition: _,
                then_label,
                else_label,
            } => {
                // A degenerate JumpIf whose two arms target the same label is just an
                // unconditional jump with a dead condition. The compiler can leave these
                // around when an arm body is empty (`if (c) {}`) and the optimizer forwards
                // the empty arm to the join. Lower it as `Code` so structuring isn't asked
                // to handle a Condition whose successors aren't dominated by it.
                if then_label == else_label {
                    DI::Code(
                        (block.label as u32).into(),
                        (block.label as u32).into(),
                        Some((*then_label as u32).into()),
                    )
                } else {
                    DI::Condition(
                        (block.label as u32).into(),
                        (block.label as u32).into(),
                        (*then_label as u32).into(),
                        (*else_label as u32).into(),
                    )
                }
            }
            SI::VariantSwitch {
                condition: _,
                enum_,
                variants,
                labels,
            } => {
                assert!(variants.len() == labels.len());
                DI::Variants(
                    (block.label as u32).into(),
                    (block.label as u32).into(),
                    *enum_,
                    variants
                        .iter()
                        .zip(labels.iter())
                        .map(|(variant, label)| (*variant, (*label as u32).into()))
                        .collect(),
                )
            }
            SI::Abort(_) | SI::Return(_) => DI::Code(
                (block.label as u32).into(),
                (block.label as u32).into(),
                None,
            ),
            SI::AssignReg { lhs: _, rhs: _ }
            | SI::StoreLoc { loc: _, value: _ }
            | SI::Nop
            | SI::Drop(_)
            | SI::NotImplemented(_) => DI::Code(
                (block.label as u32).into(),
                (block.label as u32).into(),
                next_block_label.map(|lbl| (lbl as u32).into()),
            ),
        }
    } else {
        unreachable!("Block should not be empty");
    }
}

fn generate_output(mut terms: BTreeMap<D::Label, Out::Exp>, structured: D::Structured) -> Exp {
    match structured {
        D::Structured::Break(label) => Out::Exp::Break(Some(label.index() as u64)),
        D::Structured::Continue(label) => Out::Exp::Continue(Some(label.index() as u64)),
        D::Structured::Block(lbl) => terms.remove(&(lbl as u32).into()).unwrap(),
        D::Structured::Loop(label, body) => Out::Exp::Loop(
            Some(label.index() as u64),
            Box::new(generate_output(terms, *body)),
        ),
        D::Structured::Seq(seq) => {
            let items = seq
                .into_iter()
                .map(|s| generate_output(terms.clone(), s))
                .collect();
            Out::Exp::Seq(items)
        }
        D::Structured::IfElse(lbl, conseq, alt) => {
            let term = terms.remove(&(lbl as u32).into()).unwrap();
            // TODO create helper function to extract last exp from term that works with whatever Exp, not just Seq
            let Exp::Seq(mut seq) = term else {
                panic!("A seq espected")
            };
            // When a cond block has list of exp before the jump, we need to pop the conditional statement and
            let (cond, mut exps) = (seq.pop().unwrap(), seq);
            let alt_exp = alt.map(|a| generate_output(terms.clone(), a));
            exps.push(Out::Exp::IfElse(
                Box::new(cond),
                Box::new(generate_output(terms.clone(), *conseq)),
                Box::new(alt_exp),
            ));
            Out::Exp::Seq(exps)
        }
        D::Structured::Switch(lbl, enum_, cases) => {
            let term = terms.remove(&(lbl as u32).into()).unwrap();
            let Exp::Seq(mut seq) = term else {
                panic!("A seq espected")
            };
            let (cond, mut exps) = (seq.pop().unwrap(), seq);

            let cases = cases
                .into_iter()
                .map(|(v, c)| (v, generate_output(terms.clone(), c)))
                .collect();
            exps.push(Out::Exp::Switch(Box::new(cond), enum_, cases));
            Out::Exp::Seq(exps)
        }
        D::Structured::Jump(target) => {
            let label = target.index() as u64;
            Out::Exp::Unstructured(vec![Out::UnstructuredNode::Goto(label)])
        }
        D::Structured::JumpIf(code, then_target, else_target) => {
            let term = terms.remove(&(code as u32).into()).unwrap();
            let Exp::Seq(mut seq) = term else {
                panic!("Expected Seq for JumpIf condition")
            };
            let cond = seq.pop().unwrap();

            let then_label = then_target.index() as u64;
            let else_label = else_target.index() as u64;

            seq.push(Out::Exp::IfElse(
                Box::new(cond),
                Box::new(Out::Exp::Unstructured(vec![Out::UnstructuredNode::Goto(
                    then_label,
                )])),
                Box::new(Some(Out::Exp::Unstructured(vec![
                    Out::UnstructuredNode::Goto(else_label),
                ]))),
            ));
            Out::Exp::Seq(seq)
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Compositional `let X;` hoisting
// -------------------------------------------------------------------------------------------------
//
// Per-block term reconstruction emits `LetBind` for each local's first StoreLoc, but blocks have
// no view of the surrounding scope. An arm-scope `let X` doesn't outlive the arm, so when those
// per-block LetBinds end up inside an IfElse/Switch arm or a Loop/While body, anything reading
// `X` from outside the arm either fails to resolve or shadows a still-live outer binding.
//
// This pass walks the Exp bottom-up. At each Seq it goes item-by-item with an in-scope set
// (inherited from the enclosing scope, plus earlier items in this Seq). Items that are
// IfElse/Switch/Loop/While get their arm/body top-level intros classified: if the name is
// already in scope, appears in a sibling arm, or is referenced later in this Seq, it's demoted
// to `Assign` and (unless already in scope) a fresh `Declare([X])` is prepended before the item.
// Hoists surface as new top-level intros of their containing Seq, so an inner hoist becomes
// visible to the next outer Seq and can bubble further up the same way.

fn hoist_declarations(exp: &mut Exp, scope: &mut HashSet<String>) {
    recurse_children(exp, scope);
    if let Exp::Seq(items) = exp {
        stitch_seq(items, scope);
    }
}

/// Recurse into `exp`'s children. Control-flow boundaries (Loop, While, IfElse, Switch arms)
/// each get a cloned scope: outer bindings are visible inside, but inner intros don't escape
/// except via the explicit Declares that `stitch_seq` may insert.
fn recurse_children(exp: &mut Exp, scope: &mut HashSet<String>) {
    match exp {
        Exp::Seq(items) => {
            for item in items.iter_mut() {
                hoist_declarations(item, scope);
                for n in top_level_arm_intros(item) {
                    scope.insert(n);
                }
            }
        }
        Exp::Loop(_, body) => {
            let mut inner = scope.clone();
            hoist_declarations(body, &mut inner);
        }
        Exp::While(_, cond, body) => {
            hoist_declarations(cond, scope);
            let mut inner = scope.clone();
            hoist_declarations(body, &mut inner);
        }
        Exp::IfElse(cond, conseq, alt) => {
            hoist_declarations(cond, scope);
            let mut inner = scope.clone();
            hoist_declarations(conseq, &mut inner);
            if let Some(a) = alt.as_mut() {
                let mut inner = scope.clone();
                hoist_declarations(a, &mut inner);
            }
        }
        Exp::Switch(cond, _, cases) => {
            hoist_declarations(cond, scope);
            for (_, arm) in cases.iter_mut() {
                let mut inner = scope.clone();
                hoist_declarations(arm, &mut inner);
            }
        }
        Exp::Assign(_, value)
        | Exp::LetBind(_, value)
        | Exp::Abort(value)
        | Exp::Borrow(_, value)
        | Exp::Unpack(_, _, value)
        | Exp::UnpackVariant(_, _, _, value)
        | Exp::VecUnpack(_, value) => {
            hoist_declarations(value, scope);
        }
        Exp::Return(items) | Exp::Call(_, items) => {
            for item in items.iter_mut() {
                hoist_declarations(item, scope);
            }
        }
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
            for arg in args.iter_mut() {
                hoist_declarations(arg, scope);
            }
        }
        Exp::Break(_)
        | Exp::Continue(_)
        | Exp::Value(_)
        | Exp::Variable(_)
        | Exp::Constant(_)
        | Exp::Declare(_)
        | Exp::Unstructured(_) => {}
    }
}

/// One forward pass over a Seq. For each item, ask `hoist_arm_intros` which names need to lift
/// out to this Seq's scope; prepend the new Declares, then advance `scope` with whatever the
/// (possibly demoted) item now introduces at top level.
fn stitch_seq(items: &mut Vec<Exp>, scope: &mut HashSet<String>) {
    if items.is_empty() {
        return;
    }

    // `item_refs[i]` is every name referenced anywhere inside `items[i]`; suffix-union gives
    // "referenced anywhere after position i" in O(1) per lookup once it's built.
    let item_refs: Vec<HashSet<String>> = items.iter().map(collect_references).collect();

    let mut out: Vec<Exp> = Vec::with_capacity(items.len());
    for (i, item) in std::mem::take(items).into_iter().enumerate() {
        let later_refs: HashSet<String> = item_refs[i + 1..]
            .iter()
            .flat_map(|s| s.iter().cloned())
            .collect();

        let (needs_declare, rebuilt) = hoist_arm_intros(item, scope, &later_refs);
        if !needs_declare.is_empty() {
            let mut names: Vec<String> = needs_declare.into_iter().collect();
            names.sort();
            for n in &names {
                scope.insert(n.clone());
            }
            out.push(Exp::Declare(names));
        }
        for n in top_level_arm_intros(&rebuilt) {
            scope.insert(n);
        }
        out.push(rebuilt);
    }
    *items = out;
}

/// Inspect one Seq item. If it's a multi-arm construct (IfElse/Switch) or a single-body one
/// (Loop/While), return the names that need a fresh outer `Declare` and the item with its
/// arm-level LetBinds demoted to Assigns. Other items pass through unchanged.
fn hoist_arm_intros(
    item: Exp,
    earlier_scope: &HashSet<String>,
    later_refs: &HashSet<String>,
) -> (HashSet<String>, Exp) {
    match item {
        Exp::IfElse(cond, conseq, alt) => {
            let alt_opt: Option<Exp> = *alt;
            let arm_refs: Vec<HashSet<String>> = std::iter::once(&*conseq)
                .chain(alt_opt.iter())
                .map(collect_references)
                .collect();
            let arm_intros: Vec<HashSet<String>> = std::iter::once(&*conseq)
                .chain(alt_opt.iter())
                .map(top_level_arm_intros)
                .collect();

            let to_demote = decide_demotions(&arm_intros, &arm_refs, earlier_scope, later_refs);
            let needs_declare: HashSet<String> =
                to_demote.difference(earlier_scope).cloned().collect();

            let conseq = Box::new(demote_top_level_intros(*conseq, &to_demote));
            let alt = Box::new(alt_opt.map(|a| demote_top_level_intros(a, &to_demote)));
            (needs_declare, Exp::IfElse(cond, conseq, alt))
        }
        Exp::Switch(cond, enum_, arms) => {
            let arm_exps: Vec<&Exp> = arms.iter().map(|(_, e)| e).collect();
            let arm_refs: Vec<HashSet<String>> =
                arm_exps.iter().map(|e| collect_references(e)).collect();
            let arm_intros: Vec<HashSet<String>> =
                arm_exps.iter().map(|e| top_level_arm_intros(e)).collect();

            let to_demote = decide_demotions(&arm_intros, &arm_refs, earlier_scope, later_refs);
            let needs_declare: HashSet<String> =
                to_demote.difference(earlier_scope).cloned().collect();

            let arms = arms
                .into_iter()
                .map(|(v, e)| (v, demote_top_level_intros(e, &to_demote)))
                .collect();
            (needs_declare, Exp::Switch(cond, enum_, arms))
        }
        Exp::Loop(label, body) => {
            let (to_demote, needs_declare) =
                single_body_demotions(&body, earlier_scope, later_refs);
            let body = Box::new(demote_top_level_intros(*body, &to_demote));
            (needs_declare, Exp::Loop(label, body))
        }
        Exp::While(label, cond, body) => {
            let (to_demote, needs_declare) =
                single_body_demotions(&body, earlier_scope, later_refs);
            let body = Box::new(demote_top_level_intros(*body, &to_demote));
            (needs_declare, Exp::While(label, cond, body))
        }
        other => (HashSet::new(), other),
    }
}

/// Loop/While reduction of `decide_demotions`: no sibling arms, so the only triggers are
/// already-in-scope (shadow) and referenced-after (forward use).
fn single_body_demotions(
    body: &Exp,
    earlier_scope: &HashSet<String>,
    later_refs: &HashSet<String>,
) -> (HashSet<String>, HashSet<String>) {
    let intros = top_level_arm_intros(body);
    let to_demote: HashSet<String> = intros
        .into_iter()
        .filter(|n| earlier_scope.contains(n) || later_refs.contains(n))
        .collect();
    let needs_declare = to_demote.difference(earlier_scope).cloned().collect();
    (to_demote, needs_declare)
}

/// A name introduced at the top of an arm needs to lift out iff at least one of:
///   - it's already in scope above this Seq item (the arm-level `let` would shadow);
///   - another arm of the same item also introduces or references it;
///   - some later sibling in this Seq references it.
fn decide_demotions(
    arm_intros: &[HashSet<String>],
    arm_refs: &[HashSet<String>],
    earlier_scope: &HashSet<String>,
    later_refs: &HashSet<String>,
) -> HashSet<String> {
    let mut out = HashSet::new();
    for (i, intros) in arm_intros.iter().enumerate() {
        for name in intros {
            if out.contains(name) {
                continue;
            }
            let already_in_scope = earlier_scope.contains(name);
            let used_later = later_refs.contains(name);
            let other_arm_touches = arm_intros
                .iter()
                .enumerate()
                .any(|(j, other)| j != i && other.contains(name))
                || arm_refs
                    .iter()
                    .enumerate()
                    .any(|(j, other)| j != i && other.contains(name));
            if already_in_scope || used_later || other_arm_touches {
                out.insert(name.clone());
            }
        }
    }
    out
}

/// Names introduced at the top of `exp`, descending only through `Seq`. Nested
/// IfElse/Switch/Loop/While arms have their own scopes — anything introduced inside them was
/// already given a hoist opportunity by their own Seqs.
fn top_level_arm_intros(exp: &Exp) -> HashSet<String> {
    let mut out = HashSet::new();
    top_level_arm_intros_into(exp, &mut out);
    out
}

fn top_level_arm_intros_into(exp: &Exp, out: &mut HashSet<String>) {
    match exp {
        Exp::LetBind(names, _) | Exp::Declare(names) => {
            for n in names {
                out.insert(n.clone());
            }
        }
        Exp::Seq(items) => {
            for item in items {
                top_level_arm_intros_into(item, out);
            }
        }
        _ => {}
    }
}

/// Top-level rewrite for the names in `targets`: `LetBind([X], e)` → `Assign([X], e)`, and X
/// drops out of any top-level `Declare`. Descends only through `Seq` — see `top_level_arm_intros`.
fn demote_top_level_intros(exp: Exp, targets: &HashSet<String>) -> Exp {
    if targets.is_empty() {
        return exp;
    }
    match exp {
        Exp::LetBind(names, value) => {
            if names.len() == 1 && targets.contains(&names[0]) {
                Exp::Assign(names, value)
            } else {
                Exp::LetBind(names, value)
            }
        }
        Exp::Declare(names) => {
            let kept: Vec<String> = names.into_iter().filter(|n| !targets.contains(n)).collect();
            if kept.is_empty() {
                // Empty Seq is dropped by flatten_seq later.
                Exp::Seq(vec![])
            } else {
                Exp::Declare(kept)
            }
        }
        Exp::Seq(items) => {
            let items = items
                .into_iter()
                .map(|item| demote_top_level_intros(item, targets))
                .collect();
            Exp::Seq(items)
        }
        other => other,
    }
}

/// Every name read, assigned, or otherwise mentioned anywhere in `exp`. Used as the
/// "referenced" set for hoist decisions; over-approximating just causes spurious Declares,
/// never missing ones.
fn collect_references(exp: &Exp) -> HashSet<String> {
    let mut out = HashSet::new();
    collect_references_into(exp, &mut out);
    out
}

fn collect_references_into(exp: &Exp, out: &mut HashSet<String>) {
    match exp {
        Exp::Variable(name) => {
            out.insert(name.clone());
        }
        Exp::Assign(names, value) => {
            for n in names {
                out.insert(n.clone());
            }
            collect_references_into(value, out);
        }
        Exp::LetBind(_, value) => {
            collect_references_into(value, out);
        }
        Exp::Declare(_) => {}
        Exp::Seq(items) => {
            for item in items {
                collect_references_into(item, out);
            }
        }
        Exp::IfElse(cond, conseq, alt) => {
            collect_references_into(cond, out);
            collect_references_into(conseq, out);
            if let Some(alt) = alt.as_ref() {
                collect_references_into(alt, out);
            }
        }
        Exp::Switch(cond, _, cases) => {
            collect_references_into(cond, out);
            for (_, body) in cases {
                collect_references_into(body, out);
            }
        }
        Exp::Loop(_, body) => collect_references_into(body, out),
        Exp::While(_, cond, body) => {
            collect_references_into(cond, out);
            collect_references_into(body, out);
        }
        Exp::Return(items) => {
            for item in items {
                collect_references_into(item, out);
            }
        }
        Exp::Call(_, args) => {
            for a in args {
                collect_references_into(a, out);
            }
        }
        Exp::Abort(e) => collect_references_into(e, out),
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
            for a in args {
                collect_references_into(a, out);
            }
        }
        Exp::Borrow(_, e) => collect_references_into(e, out),
        Exp::Unpack(_, _, e) => collect_references_into(e, out),
        Exp::UnpackVariant(_, _, _, e) => collect_references_into(e, out),
        Exp::VecUnpack(_, e) => collect_references_into(e, out),
        Exp::Break(_) | Exp::Continue(_) | Exp::Value(_) | Exp::Constant(_) => {}
        Exp::Unstructured(nodes) => {
            for node in nodes {
                match node {
                    Out::UnstructuredNode::Labeled(_, body)
                    | Out::UnstructuredNode::Statement(body) => {
                        collect_references_into(body, out);
                    }
                    Out::UnstructuredNode::Goto(_) => {}
                }
            }
        }
    }
}
