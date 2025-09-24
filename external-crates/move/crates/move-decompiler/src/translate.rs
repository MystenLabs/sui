// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ast as Out,
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
    let stackless = move_stackless_bytecode_2::from_model(&model, /* optimize */ true)?;
    let packages = packages(&model, stackless);
    Ok(Out::Decompiled { model, packages })
}

fn packages<S: SourceKind>(
    model: &Model<S>,
    stackless: SB::StacklessBytecode,
) -> Vec<Out::Package> {
    let SB::StacklessBytecode {
        packages: sb_packages,
    } = stackless;

    sb_packages
        .into_iter()
        .map(|pkg| package(model, pkg))
        .collect()
}

fn package<S: SourceKind>(_model: &Model<S>, sb_pkg: SB::Package) -> Out::Package {
    let SB::Package {
        name,
        address,
        modules,
    } = sb_pkg;
    let modules = modules
        .into_iter()
        .map(|(module_name, m)| {
            let decompiled_module = module(m);
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

pub fn module(module: SB::Module) -> Out::Module {
    let SB::Module { name, functions } = module;

    let functions = functions
        .into_iter()
        .map(|(name, fun)| (name, function(fun)))
        .collect();

    Out::Module { name, functions }
}

// -------------------------------------------------------------------------------------------------
// Function
// -------------------------------------------------------------------------------------------------

fn function(fun: SB::Function) -> Out::Function {
    println!("Decompiling function {}", fun.name);
    println!("-- stackless bytecode ------------------");
    for (lbl, blk) in &fun.basic_blocks {
        println!("Block {}:\n{blk}", lbl);
    }
    println!("----------------------------------------");
    let (name, terms, input, entry) = make_input(fun);
    println!("Input: {input:?}");
    let structured = crate::structuring::structure(input, entry);
    // println!("{}", structured.to_test_string());
    let mut code = generate_output(terms, structured);
    crate::refinement::refine(&mut code);
    // println!("Function {name}:\n{code}");
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
    let mut let_binds = HashSet::new();

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
        // Extract terms and input for the block
        let blk_terms = generate_term_block(block, &mut let_binds);
        let blk_input = extract_input(block, next_block_label);

        // Insert into the maps

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
            } => DI::Condition(
                (block.label as u32).into(),
                (block.label as u32).into(),
                (*then_label as u32).into(),
                (*else_label as u32).into(),
            ),
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
        D::Structured::Break => Out::Exp::Break,
        D::Structured::Continue => Out::Exp::Continue,
        D::Structured::Block(lbl) => terms.remove(&(lbl as u32).into()).unwrap(),
        D::Structured::Loop(body) => Out::Exp::Loop(Box::new(generate_output(terms, *body))),
        D::Structured::Seq(seq) => {
            let seq = seq
                .into_iter()
                .map(|s| generate_output(terms.clone(), s))
                .collect();
            Out::Exp::Seq(seq)
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
        D::Structured::Jump(_) | D::Structured::JumpIf { .. } => {
            unreachable!("Jump nodes should not be present in the final output")
        }
    }
}
