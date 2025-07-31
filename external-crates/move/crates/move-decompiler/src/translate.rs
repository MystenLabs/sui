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
    BTreeMap<D::Label, D::Block>,
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

fn generate_term_block(block: &S::BasicBlock) -> D::Block {
    D::Block{ body: vec![] }
}

fn extract_terms(block: &S::BasicBlock) -> Vec<Term> {
    let mut terms = Vec::new();
    for instr in &block.instructions {
        let term = extract_term(instr);
        terms.push(term);
    }
    terms
}

fn extract_term(instr: &S::Instruction) -> Term {
    // FIXME
    match instr {
        S::Instruction::AssignReg { lhs, rhs } => match rhs {
            S::RValue::Trivial(trivial) => Term::Assign {
                target: lhs.clone(),
                value: trivial.clone(),
            },
            S::RValue::Primitive { op, args } => Term::PrimitiveOp {
                op: format!("{:?}", op),
                args: args.clone(),
                result: lhs.clone(),
            },
            S::RValue::Data { op, args } => Term::DataOp {
                op: format!("{:?}", op),
                args: args.clone(),
                result: lhs.clone(),
            },
            S::RValue::Call { function, args } => Term::Call {
                function: *function,
                args: args.clone(),
                result: lhs.clone(),
            },
            S::RValue::Local { op, arg } => Term::LocalOp {
                op: format!("{:?}", op),
                loc: *arg,
                value: None,
                result: lhs.clone(),
            },
            S::RValue::Constant(const_ref) => Term::Constant {
                value: const_ref.data.clone(),
                result: lhs.clone(),
            },
        },
        S::Instruction::Drop(reg) => Term::Drop(*reg),
        S::Instruction::StoreLoc { loc, value } => Term::LocalOp {
            op: "store".to_string(),
            loc: *loc,
            value: Some(value.clone()),
            result: vec![],
        },
        S::Instruction::Abort(trivial) => Term::Abort(trivial.clone()),
        S::Instruction::Return(trivials) => Term::Return(trivials.clone()),
        S::Instruction::Nop => Term::Nop,
        S::Instruction::NotImplemented(msg) => Term::NotImplemented(msg.clone()),
        // Handle other instruction types
        S::Instruction::Jump(_)
        | S::Instruction::JumpIf {
            condition: _,
            then_label: _,
            else_label: _,
        }
        | S::Instruction::VariantSwitch { cases: _ } => {
            Term::NotImplemented("Control flow instruction not implemented".to_string())
        }
    }
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
    terms: BTreeMap<D::Label, D::Block>,
    structured: D::Structured,
) -> Exp {
    Exp::Continue
}
fn generate_output_old(terms: BTreeMap<D::Label, Term>, structured: D::Structured) -> Exp {
    // Convert the structured representation back to your Exp format
    convert_structured_to_exp(&terms, structured)
}

fn convert_structured_to_exp(terms: &BTreeMap<D::Label, Term>, structured: D::Structured) -> Exp {
    // FIXME
    match structured {
        D::Structured::Break => Exp::Break,
        D::Structured::Continue => Exp::Continue,
        D::Structured::Block(label) => {
            // You'll need to look up the block and convert its terms
            // For now, create a placeholder
            let blk = terms.get(&(label.0 as u32).into()).expect("Block not found");
            Exp::Block(blk.clone())
        }
        D::Structured::Loop(body) => Exp::Loop(Box::new(convert_structured_to_exp(terms, *body))),
        D::Structured::Seq(seq) => {
            Exp::Seq(seq.into_iter().map(|s| convert_structured_to_exp(terms, s)).collect())
        }
        D::Structured::While(cond, body) => {
            // You'll need to convert the condition to a Term
            let cond_term =
                Term::NotImplemented("Condition conversion not implemented".to_string());
            Exp::While(cond_term, Box::new(convert_structured_to_exp(terms, *body)))
        }
        D::Structured::IfElse(cond, then_branch, else_branch) => {
            let cond_term =
                Term::NotImplemented("Condition conversion not implemented".to_string());
            let else_exp = else_branch.map(|e| convert_structured_to_exp(terms, e));
            Exp::IfElse(
                cond_term,
                Box::new(convert_structured_to_exp(terms, *then_branch)),
                Box::new(else_exp),
            )
        }
        D::Structured::Switch(cond, cases) => {
            let cond_term =
                Term::NotImplemented("Switch condition conversion not implemented".to_string());
            let case_exps = cases.into_iter().map(|c| convert_structured_to_exp(terms, c)).collect();
            Exp::Switch(cond_term, case_exps)
        }
        _ => Exp::Block(Term::NotImplemented(format!(
            "Unhandled structured type: {:?}",
            structured
        ))),
    }
}

fn into_term(term: Term) -> Exp {
    // FIXME
    match term {
        Term::Assign { target, value } => Exp::Block(Term::Assign { target, value }),
        Term::PrimitiveOp { op, args, result } => {
            Exp::Block(Term::PrimitiveOp { op, args, result })
        }
        Term::DataOp { op, args, result } => Exp::Block(Term::DataOp { op, args, result }),
        Term::Call {
            function,
            args,
            result,
        } => Exp::Block(Term::Call {
            function,
            args,
            result,
        }),
        Term::LocalOp {
            op,
            loc,
            value,
            result,
        } => Exp::Block(Term::LocalOp {
            op,
            loc,
            value,
            result,
        }),
        Term::Drop(reg) => Exp::Block(Term::Drop(reg)),
        Term::Abort(trivial) => Exp::Block(Term::Abort(trivial)),
        Term::Return(trivials) => Exp::Block(Term::Return(trivials)),
        Term::Constant { value, result } => Exp::Block(Term::Constant { value, result }),
        Term::Nop => Exp::Block(Term::Nop),
        Term::NotImplemented(msg) => Exp::Block(Term::NotImplemented(msg)),
        Term::Untranslated(basic_block) => todo!(),
    }
}
