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

    let mut module = Out::Module {
        name,
        functions,
        uses: BTreeMap::new(),
        type_uses: BTreeMap::new(),
    };

    let current_mid = resolved.id();
    let used = build_used(&module, &resolved);
    crate::refinement::collect_uses(&mut module, current_mid, &used);
    module
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
    // Look up the compiled function so we can name its parameters. The first `parameters.len()`
    // local indices are the parameters; the rest are body locals introduced by the bytecode.
    // term_reconstruction renders local id `i` as `l{i}`, so the parameter names are
    // `l0..l{param_count-1}`.
    let param_count = resolved_module
        .function(fun.name)
        .maybe_compiled()
        .map(|f| f.parameters.len())
        .unwrap_or(0);
    let params: Vec<String> = (0..param_count).map(|i| format!("l{i}")).collect();
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
    crate::structuring::hoist_declarations::hoist_declarations(&mut code, params);
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
            exps.push(Out::Exp::Switch(
                Box::new(cond),
                Out::TypeRef::Qualified(Out::ModuleRef::Qualified(enum_.0), enum_.1),
                cases,
            ));
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
