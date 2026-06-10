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
use indexmap::IndexMap;
use move_model_2::{
    model::{Model, Module as MModule},
    source_kind::SourceKind,
};
use move_stackless_bytecode_2::ast as SB;
use move_symbol_pool::Symbol;
use std::collections::{BTreeMap, BTreeSet, HashSet};

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

fn package<S: SourceKind>(config: &Config, model: &Model<S>, sb_pkg: SB::Package) -> Out::Package {
    let SB::Package {
        name,
        address,
        modules,
    } = sb_pkg;
    let pkg = model.package(&address);
    let modules = modules
        .into_iter()
        .map(|(module_name, m)| {
            let resolved = pkg.module(module_name);
            let decompiled_module = module(config, resolved, m);
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

pub fn module<S: SourceKind>(
    config: &Config,
    resolved: MModule<'_, S>,
    sb_module: SB::Module,
) -> Out::Module {
    let SB::Module { name, functions } = sb_module;

    let functions = functions
        .into_iter()
        .map(|(name, fun)| (name, function(config, resolved, fun)))
        .collect();

    let structs = collect_structs(&resolved);
    let enums = collect_enums(&resolved);

    let mut module = Out::Module {
        name,
        structs,
        enums,
        functions,
        uses: BTreeMap::new(),
        type_uses: BTreeMap::new(),
    };

    let current_mid = resolved.id();
    let used = build_used(&module, &resolved);
    crate::refinement::collect_uses(&mut module, current_mid, &used);
    module
}

/// Convert each compiled struct in `resolved` into our `ast::Struct`. Fields' types are
/// translated via `Type::from_normalized`, leaving every `Datatype` reference as `Qualified`
/// initially; `collect_uses` rewrites those to `Aliased` after counting.
fn collect_structs<S: SourceKind>(resolved: &MModule<'_, S>) -> IndexMap<Symbol, Out::Struct> {
    let mut out = IndexMap::new();
    for s in resolved.structs() {
        let compiled = s.compiled();
        let fields = compiled
            .fields
            .0
            .iter()
            .map(|(name, field)| (*name, Out::Type::from_normalized(&field.type_)))
            .collect();
        out.insert(
            s.name(),
            Out::Struct {
                name: s.name(),
                abilities: compiled.abilities,
                type_parameters: compiled.type_parameters.clone(),
                fields,
            },
        );
    }
    out
}

fn collect_enums<S: SourceKind>(resolved: &MModule<'_, S>) -> IndexMap<Symbol, Out::Enum> {
    let mut out = IndexMap::new();
    for e in resolved.enums() {
        let compiled = e.compiled();
        let variants = compiled
            .variants
            .iter()
            .map(|(vname, variant)| {
                let fields = variant
                    .fields
                    .0
                    .iter()
                    .map(|(name, field)| (*name, Out::Type::from_normalized(&field.type_)))
                    .collect();
                Out::Variant {
                    name: *vname,
                    fields,
                }
            })
            .collect();
        out.insert(
            e.name(),
            Out::Enum {
                name: e.name(),
                abilities: compiled.abilities,
                type_parameters: compiled.type_parameters.clone(),
                variants,
            },
        );
    }
    out
}

/// Names that no module alias may use: every function, struct, enum, variant, and field
/// name declared in this module, plus every local introduced or referenced in any function
/// body. Aliases shadowing these would produce ambiguous or wrong decompiled source.
fn build_used<S: SourceKind>(module: &Out::Module, resolved: &MModule<'_, S>) -> BTreeSet<Symbol> {
    let mut out = crate::refinement::collect_local_names(module);

    // Function names in this module.
    for name in module.functions.keys() {
        out.insert(*name);
    }

    // Struct, enum, variant, and field names declared in this module.
    for s in resolved.structs() {
        out.insert(s.name());
        for field in s.compiled().fields.0.keys() {
            out.insert(*field);
        }
    }
    for e in resolved.enums() {
        out.insert(e.name());
        for v in e.variants() {
            out.insert(v.name());
            for field in v.compiled().fields.0.keys() {
                out.insert(*field);
            }
        }
    }
    out
}

// -------------------------------------------------------------------------------------------------
// Function
// -------------------------------------------------------------------------------------------------

fn function<S: SourceKind>(
    config: &Config,
    resolved_module: MModule<'_, S>,
    fun: SB::Function,
) -> Out::Function {
    if config.debug_print.print_function_heading() {
        println!("DECOMPILING FUNCTION {}", fun.name);
    }
    if config.debug_print.stackless {
        print_heading("stackless bytecode");
        for (lbl, blk) in &fun.basic_blocks {
            println!("Block {}:\n{blk}", lbl);
        }
    }
    // Pull the compiled function's signature so parameter/return types end up in the AST,
    // available to `collect_uses` for alias rewriting. The first `parameters.len()` local
    // indices are the parameters; `term_reconstruction` renders local id `i` as `l{i}`, so
    // parameter names are `l0..l{N-1}` (we generate them at print time from this count).
    let model_fun = resolved_module.function(fun.name);
    let compiled = model_fun.maybe_compiled();
    let parameters: Vec<Out::Type> = compiled
        .map(|f| {
            f.parameters
                .iter()
                .map(|t| Out::Type::from_normalized(t))
                .collect()
        })
        .unwrap_or_default();
    let returns: Vec<Out::Type> = compiled
        .map(|f| {
            f.return_
                .iter()
                .map(|t| Out::Type::from_normalized(t))
                .collect()
        })
        .unwrap_or_default();
    let visibility = compiled.map(|f| f.visibility).unwrap_or_default();
    let is_entry = compiled.map(|f| f.is_entry).unwrap_or(false);
    let type_parameters = compiled
        .map(|f| f.type_parameters.clone())
        .unwrap_or_default();
    let param_names: Vec<String> = (0..parameters.len()).map(|i| format!("l{i}")).collect();
    let (name, terms, input, entry) = make_input(fun);
    if config.debug_print.input {
        print_heading("input");
        println!("{input:?}");
    }
    let (structured, unstructured_blocks) =
        crate::structuring::structure(config, input, entry, &terms);
    if config.debug_print.structured {
        print_heading("structured");
        println!("{}", structured.to_test_string());
    }
    let mut code = generate_output(terms, structured);
    // Block markers exist only to give surviving `Unstructured(Goto)`s something to point
    // at. Keep `Block(N, _)` iff some surviving goto targets `N`; strip the rest. Functions
    // with no surviving gotos shed every marker, restoring pre-Block adjacency for the
    // refinement pipeline.
    let targets = collect_goto_targets(&code);
    strip_untargeted_blocks(&mut code, &targets);
    crate::structuring::hoist_declarations::hoist_declarations(&mut code, param_names);
    crate::refinement::refine(&mut code);
    if config.debug_print.decompiled_code {
        print_heading("refined code");
        println!("{code}");
    }
    Out::Function {
        name,
        visibility,
        is_entry,
        type_parameters,
        parameters,
        returns,
        code,
        unstructured_blocks,
    }
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

/// Collect every label that a surviving `Unstructured(Goto(_))` targets. Block markers
/// whose ID is in this set are kept (for cross-referencing); the rest are stripped.
fn collect_goto_targets(exp: &Exp) -> HashSet<u64> {
    let mut out = HashSet::new();
    collect_goto_targets_into(exp, &mut out);
    out
}

fn collect_goto_targets_into(exp: &Exp, out: &mut HashSet<u64>) {
    use crate::ast::UnstructuredNode;
    match exp {
        Exp::Unstructured(nodes) => {
            for n in nodes {
                match n {
                    UnstructuredNode::Goto(label) => {
                        out.insert(*label);
                    }
                    UnstructuredNode::Labeled(_, body) | UnstructuredNode::Statement(body) => {
                        collect_goto_targets_into(body, out);
                    }
                }
            }
        }
        Exp::Block(_, body)
        | Exp::Loop(_, body)
        | Exp::Assign(_, body)
        | Exp::LetBind(_, body)
        | Exp::Abort(body)
        | Exp::Borrow(_, body)
        | Exp::Unpack(_, _, body)
        | Exp::UnpackVariant(_, _, _, body)
        | Exp::VecUnpack(_, body) => collect_goto_targets_into(body, out),
        Exp::While(_, c, b) => {
            collect_goto_targets_into(c, out);
            collect_goto_targets_into(b, out);
        }
        Exp::IfElse(c, t, alt) => {
            collect_goto_targets_into(c, out);
            collect_goto_targets_into(t, out);
            if let Some(a) = alt.as_ref().as_ref() {
                collect_goto_targets_into(a, out);
            }
        }
        Exp::Switch(c, _, arms) => {
            collect_goto_targets_into(c, out);
            for (_, e) in arms {
                collect_goto_targets_into(e, out);
            }
        }
        Exp::Match(c, _, arms) => {
            collect_goto_targets_into(c, out);
            for (_, _, e) in arms {
                collect_goto_targets_into(e, out);
            }
        }
        Exp::MatchLit(c, arms) => {
            collect_goto_targets_into(c, out);
            for (_, e) in arms {
                collect_goto_targets_into(e, out);
            }
        }
        Exp::Seq(es) | Exp::Return(es) | Exp::Call(_, es) => {
            for e in es {
                collect_goto_targets_into(e, out);
            }
        }
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
            for a in args {
                collect_goto_targets_into(a, out);
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

/// Recursively strip every `Exp::Block(id, body)` whose `id` is not in `targets`, leaving
/// targeted blocks intact. A block keeps its wrapper iff some surviving goto names its ID;
/// untargeted wrappers would only fragment the AST for refinements without aiding the
/// reader.
fn strip_untargeted_blocks(exp: &mut Exp, targets: &HashSet<u64>) {
    // First collapse this node if it's an un-targeted Block. The loop chases nested
    // un-targeted wrappers in a single pass.
    while let Exp::Block(id, body) = exp
        && !targets.contains(id)
    {
        let inner = std::mem::replace(body.as_mut(), Exp::Seq(vec![]));
        *exp = inner;
    }
    // Then recur into children, including the inside of any kept `Block`.
    match exp {
        Exp::Block(_, body)
        | Exp::Loop(_, body)
        | Exp::Assign(_, body)
        | Exp::LetBind(_, body)
        | Exp::Abort(body)
        | Exp::Borrow(_, body)
        | Exp::Unpack(_, _, body)
        | Exp::UnpackVariant(_, _, _, body)
        | Exp::VecUnpack(_, body) => strip_untargeted_blocks(body, targets),
        Exp::While(_, c, b) => {
            strip_untargeted_blocks(c, targets);
            strip_untargeted_blocks(b, targets);
        }
        Exp::IfElse(c, t, alt) => {
            strip_untargeted_blocks(c, targets);
            strip_untargeted_blocks(t, targets);
            if let Some(a) = alt.as_mut().as_mut() {
                strip_untargeted_blocks(a, targets);
            }
        }
        Exp::Switch(c, _, arms) => {
            strip_untargeted_blocks(c, targets);
            for (_, e) in arms {
                strip_untargeted_blocks(e, targets);
            }
        }
        Exp::Match(c, _, arms) => {
            strip_untargeted_blocks(c, targets);
            for (_, _, e) in arms {
                strip_untargeted_blocks(e, targets);
            }
        }
        Exp::MatchLit(c, arms) => {
            strip_untargeted_blocks(c, targets);
            for (_, e) in arms {
                strip_untargeted_blocks(e, targets);
            }
        }
        Exp::Seq(es) | Exp::Return(es) | Exp::Call(_, es) => {
            for e in es {
                strip_untargeted_blocks(e, targets);
            }
        }
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
            for a in args {
                strip_untargeted_blocks(a, targets);
            }
        }
        Exp::Unstructured(nodes) => {
            use crate::ast::UnstructuredNode;
            for n in nodes {
                if let UnstructuredNode::Labeled(_, body) | UnstructuredNode::Statement(body) = n {
                    strip_untargeted_blocks(body, targets);
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

/// Lower a recovered boolean [`Formula`](crate::structuring::reaching::Formula) to an `Exp`,
/// substituting each atom with its condition-block expression and mapping `And`/`Or`/`Not` to
/// the corresponding short-circuiting primitives.
fn formula_to_exp(
    formula: &crate::structuring::reaching::Formula,
    conds: &BTreeMap<D::Label, Exp>,
) -> Exp {
    use crate::structuring::reaching::FormulaTree as T;
    fn prim(op: SB::PrimitiveOp, a: Exp, b: Exp) -> Exp {
        Out::Exp::Primitive {
            op,
            args: vec![a, b],
        }
    }
    match &formula.0 {
        T::True => Out::Exp::Value(move_core_types::runtime_value::MoveValue::Bool(true)),
        T::False => Out::Exp::Value(move_core_types::runtime_value::MoveValue::Bool(false)),
        T::Atom(n) => conds
            .get(n)
            .cloned()
            .expect("CondIf atom missing its condition expression"),
        T::Not(inner) => Out::Exp::Primitive {
            op: SB::PrimitiveOp::Not,
            args: vec![formula_to_exp(inner, conds)],
        },
        T::And(fs) => fs
            .iter()
            .map(|x| formula_to_exp(x, conds))
            .reduce(|a, b| prim(SB::PrimitiveOp::And, a, b))
            .expect("non-empty And"),
        T::Or(fs) => fs
            .iter()
            .map(|x| formula_to_exp(x, conds))
            .reduce(|a, b| prim(SB::PrimitiveOp::Or, a, b))
            .expect("non-empty Or"),
    }
}

fn generate_output(mut terms: BTreeMap<D::Label, Out::Exp>, structured: D::Structured) -> Exp {
    match structured {
        D::Structured::Break(label) => Out::Exp::Break(Some(label.index() as u64)),
        D::Structured::Continue(label) => Out::Exp::Continue(Some(label.index() as u64)),
        D::Structured::Block(lbl) => {
            let term = terms.remove(&(lbl as u32).into()).unwrap();
            Out::Exp::Block(lbl, Box::new(term))
        }
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
        D::Structured::CondIf(formula, conseq, alt) => {
            // Pull each atom block's term apart: its trailing expr is the block's condition;
            // any leading statements are setup that must run before the (hoisted) `if`. Atoms
            // are taken in block-id order (≈ program order), so hoisted setups stay correctly
            // ordered.
            let mut setups: Vec<Exp> = Vec::new();
            let mut conds: BTreeMap<D::Label, Exp> = BTreeMap::new();
            for atom in formula.atoms() {
                let term = terms.remove(&atom).unwrap();
                let Exp::Seq(mut seq) = term else {
                    panic!("Expected Seq for CondIf atom block")
                };
                let cond = seq.pop().unwrap();
                setups.extend(seq);
                conds.insert(atom, cond);
            }
            let cond = formula_to_exp(&formula, &conds);
            let alt_exp = alt.and_then(|a| {
                let e = generate_output(terms.clone(), a);
                match &e {
                    Exp::Seq(items) if items.is_empty() => None,
                    _ => Some(e),
                }
            });
            setups.push(Out::Exp::IfElse(
                Box::new(cond),
                Box::new(generate_output(terms.clone(), *conseq)),
                Box::new(alt_exp),
            ));
            // Single-atom case is the migrated `IfElse(Code, ...)`. Wrap in a labeled `Block`
            // anchored at the condition block's label so any `Break(Some(lbl))` targeting it
            // (synthesized upstream by the structurer) still has its target. Compound formulas
            // come from the reaching structurer where no `Break` targets the synthesized guard.
            match formula.as_atom() {
                Some(n) => Out::Exp::Block(n.index() as u64, Box::new(Out::Exp::Seq(setups))),
                None => Out::Exp::Seq(setups),
            }
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
            exps.push(Out::Exp::Switch(
                Box::new(cond),
                Out::TypeRef::Qualified(Out::ModuleRef::Qualified(enum_.0), enum_.1),
                cases,
            ));
            Out::Exp::Block(lbl, Box::new(Out::Exp::Seq(exps)))
        }
        D::Structured::Jump(src, target) => {
            let label = target.index() as u64;
            // Surviving Jump becomes a real goto; tag the line with its creation site so
            // the corpus driver can break down residue by category.
            eprintln!("GOTO[{}] -> label_{}", src.as_tag(), label);
            Out::Exp::Unstructured(vec![Out::UnstructuredNode::Goto(label)])
        }
        D::Structured::JumpIf(src, code, then_target, else_target) => {
            let term = terms.remove(&(code as u32).into()).unwrap();
            let Exp::Seq(mut seq) = term else {
                panic!("Expected Seq for JumpIf condition")
            };
            let cond = seq.pop().unwrap();

            let then_label = then_target.index() as u64;
            let else_label = else_target.index() as u64;
            eprintln!(
                "GOTO[{}] -> label_{}/label_{}",
                src.as_tag(),
                then_label,
                else_label
            );

            seq.push(Out::Exp::IfElse(
                Box::new(cond),
                Box::new(Out::Exp::Unstructured(vec![Out::UnstructuredNode::Goto(
                    then_label,
                )])),
                Box::new(Some(Out::Exp::Unstructured(vec![
                    Out::UnstructuredNode::Goto(else_label),
                ]))),
            ));
            Out::Exp::Block(code, Box::new(Out::Exp::Seq(seq)))
        }
        D::Structured::Let(name) => Out::Exp::Declare(vec![name]),
        D::Structured::Assign(name, value) => Out::Exp::Assign(
            vec![name],
            Box::new(Out::Exp::Value(
                move_core_types::runtime_value::MoveValue::U32(value),
            )),
        ),
        D::Structured::SelectorMatch(name, arms) => {
            let translated_arms: Vec<(crate::ast::DispatchTag, Out::Exp)> = arms
                .into_iter()
                .map(|(tag, body)| (tag, generate_output(terms.clone(), body)))
                .collect();
            Out::Exp::MatchLit(Box::new(Out::Exp::Variable(name)), translated_arms)
        }
    }
}

// -------------------------------------------------------------------------------------------------
