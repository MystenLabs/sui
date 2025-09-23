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
use move_stackless_bytecode_2::stackless::ast as S;
use move_symbol_pool::Symbol;
use std::collections::{BTreeMap, HashSet};

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
    println!("Decompiling function {}", fun.name);
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

fn generate_term_block(block: &S::BasicBlock, let_binds: &mut HashSet<S::RegId>) -> Out::Exp {
    // remove the last jump / replace the conditional with just the "triv" in it
    term_reconstruction::exp(block.clone(), let_binds)
}

fn extract_input(block: &S::BasicBlock, next_block_label: Option<S::Label>) -> D::Input {
    // Look at the last instruction to determine control flow
    if let Some(last_instr) = block.instructions.last() {
        match last_instr {
            S::Instruction::Jump(label) => D::Input::Code(
                (block.label as u32).into(),
                ((block.label as u32).into(), false),
                Some((*label as u32).into()),
            ),
            S::Instruction::JumpIf {
                condition: _,
                then_label,
                else_label,
            } => D::Input::Condition(
                (block.label as u32).into(),
                ((block.label as u32).into(), false),
                (*then_label as u32).into(),
                (*else_label as u32).into(),
            ),
            S::Instruction::VariantSwitch {
                condition: _,
                labels,
                variants: _,
            } => D::Input::Variants(
                (block.label as u32).into(),
                ((block.label as u32).into(), false),
                labels.iter().map(|label| (*label as u32).into()).collect(),
            ),
            S::Instruction::Return(_) => D::Input::Code(
                (block.label as u32).into(),
                ((block.label as u32).into(), false),
                None,
            ),
            S::Instruction::AssignReg { lhs: _, rhs: _ }
            | S::Instruction::StoreLoc { loc: _, value: _ }
            | S::Instruction::Abort(_)
            | S::Instruction::Nop
            | S::Instruction::Drop(_)
            | S::Instruction::NotImplemented(_) => D::Input::Code(
                (block.label as u32).into(),
                ((block.label as u32).into(), false),
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
        D::Structured::Block((lbl, _invert)) => terms.remove(&(lbl as u32).into()).unwrap(),
        D::Structured::Loop(body) => Out::Exp::Loop(Box::new(generate_output(terms, *body))),
        D::Structured::Seq(seq) => {
            let seq = seq
                .into_iter()
                .map(|s| generate_output(terms.clone(), s))
                .collect();
            Out::Exp::Seq(seq)
        }
        D::Structured::IfElse((lbl, _invert), conseq, alt) => {
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
        D::Structured::Switch((lbl, _invert), cases) => {
            let term = terms.remove(&(lbl as u32).into()).unwrap();
            let Exp::Seq(mut seq) = term else {
                panic!("A seq espected")
            };
            let (cond, mut exps) = (seq.pop().unwrap(), seq);

            let cases = cases
                .into_iter()
                .map(|c| generate_output(terms.clone(), c))
                .collect();
            exps.push(Out::Exp::Switch(Box::new(cond), cases));
            Out::Exp::Seq(exps)
        }
        D::Structured::Jump(_) | D::Structured::JumpIf { .. } => {
            unreachable!("Jump nodes should not be present in the final output")
        }
    }
}
