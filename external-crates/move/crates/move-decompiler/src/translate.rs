// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ast as Out,
    structuring::{
        ast::{self as D, Label},
        graph::Graph,
    },
};

use crate::ast::{Exp, Term};
use move_stackless_bytecode_2::stackless::ast as S;
use move_symbol_pool::Symbol;
use std::collections::BTreeMap;

// -------------------------------------------------------------------------------------------------
// Module
// -------------------------------------------------------------------------------------------------

pub(crate) fn module(module: S::Module) -> Out::Module {
    let S::Module { name, functions } = module;

    let functions = functions
        .into_iter()
        .map(|(name, fun)| (name, function(fun)))
        .collect();

    Out::Module { name, functions }
}

// -------------------------------------------------------------------------------------------------
// Function
// -------------------------------------------------------------------------------------------------

fn function(fun: S::Function) -> Out::Function {
    println!("{}", fun);
    let (name, terms, input, entry) = make_input(fun);
    println!("Input: {input:#?}");
    let structured = crate::structuring::structure(input, entry);
    println!("{}", structured.to_test_string());
    let code = generate_output(terms, structured);

    Out::Function { name, code }
}

fn make_input(
    fun: S::Function,
) -> (
    Symbol,
    BTreeMap<D::Label, Out::Exp>,
    BTreeMap<D::Label, D::Input>,
    D::Label,
) {
    let S::Function {
        name,
        entry_label,
        basic_blocks,
    } = fun;

    let mut terms = BTreeMap::new();
    let mut input = BTreeMap::new();

    let mut blocks_iter = basic_blocks.iter();
    let mut next_blocks_iter = basic_blocks.iter().skip(1);

    while let Some((lbl, block)) = blocks_iter.next() {
        let label = lbl;
        assert!(*label == block.label, "Block label mismatch: {label} != {}", block.label);
        let next_block_label = if let Some((nxt_lbl, _)) = next_blocks_iter.next() {
            Some(*nxt_lbl)
        } else {
            None
        };
        // Extract terms and input for the block
        // TODO extract terms to be impmlemented
        let blk_terms = generate_term_block(block);
        let blk_input = extract_input(&block, next_block_label);

        // Insert into the maps

        terms.insert((*label as u32).into(), blk_terms);
        input.insert((*label as u32).into(), blk_input);
    }
    (name, terms, input, (entry_label as u32).into())
}

fn generate_term_block(block: &S::BasicBlock) -> Out::Exp {
    Out::Exp::Block(Term::Untranslated(block.clone()))
}

fn extract_input(block: &S::BasicBlock, next_block_label: Option<S::Label>) -> D::Input {
    // Look at the last instruction to determine control flow
    if let Some(last_instr) = block.instructions.last() {
        match last_instr {
            S::Instruction::Jump(label) => {
                return D::Input::Code(
                    (block.label as u32).into(),
                    ((block.label as u32).into(), false),
                    Some((*label as u32).into()),
                );
            }
            S::Instruction::JumpIf {
                condition: _,
                then_label,
                else_label,
            } => {
                return D::Input::Condition(
                    (block.label as u32).into(),
                    ((block.label as u32).into(), false),
                    (*then_label as u32).into(),
                    (*else_label as u32).into(),
                );
            }
            S::Instruction::VariantSwitch { cases } => {
                return D::Input::Variants(
                    (block.label as u32).into(),
                    ((block.label as u32).into(), false),
                    cases
                        .into_iter()
                        .map(|case| (*case as u32).into())
                        .collect(),
                );
            }
            S::Instruction::Return(_) => {
                return D::Input::Code(
                    (block.label as u32).into(),
                    ((block.label as u32).into(), false),
                    None,
                );
            }
            S::Instruction::AssignReg { lhs: _, rhs: _ }
            | S::Instruction::StoreLoc { loc: _, value: _ }
            | S::Instruction::Abort(_)
            | S::Instruction::Nop
            | S::Instruction::Drop(_)
            | S::Instruction::NotImplemented(_) => {
                return D::Input::Code(
                    (block.label as u32).into(),
                    ((block.label as u32).into(), false),
                    next_block_label.map(|lbl| (lbl as u32).into()),
                );
            }
        }
    } else {
        unreachable!("Block should not be empty");
    }
}

fn generate_output(
    mut terms: BTreeMap<D::Label, Out::Exp>,
    structured: D::Structured,
) -> Exp {
    match structured {
        D::Structured::Break => Out::Exp::Break,
        D::Structured::Continue => Out::Exp::Continue,
        D::Structured::Block((lbl, _invert)) => {
            terms.remove(&(lbl as u32).into()).unwrap()
        },
        D::Structured::Loop(body) => Out::Exp::Loop(Box::new(generate_output(terms, *body))),
        D::Structured::Seq(seq) => {
            let seq = seq.into_iter().map(|s| generate_output(terms.clone(), s)).collect();
            Out::Exp::Seq(seq)
        }
        D::Structured::While((lbl, _invert), body) => {
            let term = terms.remove(&(lbl as u32).into()).unwrap();
            Out::Exp::While(Box::new(term), Box::new(generate_output(terms, *body)))
        }
        D::Structured::IfElse((lbl, _invert), conseq, alt) => {
            let term = terms.remove(&(lbl as u32).into()).unwrap();
            let alt_exp = alt.map(|a| generate_output(terms.clone(), a));
            Out::Exp::IfElse(Box::new(term), Box::new(generate_output(terms.clone(), *conseq)), Box::new(alt_exp))
        }
        D::Structured::Switch((lbl, _invert), cases) => {
            let term = terms.remove(&(lbl as u32).into()).unwrap();
            let cases = cases.into_iter().map(|c| generate_output(terms.clone(), c)).collect();
            Out::Exp::Switch(Box::new(term), cases)
        }
        D::Structured::Jump(_) 
        | D::Structured::JumpIf { .. } => unreachable!("Jump nodes should not be present in the final output")

    }
}

