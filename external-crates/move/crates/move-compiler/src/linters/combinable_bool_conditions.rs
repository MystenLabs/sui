// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! The `CombinableBool` detects and warns about boolean conditions in Move code that can be simplified.
//! It identifies comparisons that are logically equivalent and suggests more concise alternatives.
//! This rule focuses on simplifying expressions involving `==`, `<`, `>`, and `!=` operators to improve code readability.

use crate::{
    diag,
    linters::StyleCodes,
    parser::ast::BinOp_,
    typing::{
        ast::{self as T, UnannotatedExp_},
        visitor::simple_visitor,
    },
};
use lazy_static::lazy_static;
use move_ir_types::location::Loc;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
enum Simplification {
    Contradiction,
    UseComparison,
    UseEquality,
}

impl Simplification {
    fn message(&self) -> &'static str {
        match self {
            Simplification::Contradiction => {
                "This is always contradictory and can be simplified to false"
            }
            Simplification::UseComparison => "Consider simplifying to `<=` or `>=` respectively.",
            Simplification::UseEquality => "Consider simplifying to `==`.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Operator {
    Eq,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

impl From<&BinOp_> for Operator {
    fn from(op: &BinOp_) -> Self {
        match op {
            BinOp_::Eq => Operator::Eq,
            BinOp_::Lt => Operator::Lt,
            BinOp_::Gt => Operator::Gt,
            BinOp_::Le => Operator::Le,
            BinOp_::Ge => Operator::Ge,
            BinOp_::And => Operator::And,
            BinOp_::Or => Operator::Or,
            _ => panic!("Unexpected operator"),
        }
    }
}

lazy_static! {
    static ref OPERATOR_COMBINATIONS: HashMap<(Operator, Operator, Operator), Simplification> = {
        let mut m = HashMap::new();
        // Contradictions
        for ops in [(Operator::Eq, Operator::Lt), (Operator::Eq, Operator::Gt)] {
            m.insert((ops.0, ops.1, Operator::And), Simplification::Contradiction);
            m.insert((ops.1, ops.0, Operator::And), Simplification::Contradiction);
        }
        // Use comparison operators
        for ops in [(Operator::Eq, Operator::Lt), (Operator::Eq, Operator::Gt)] {
            m.insert((ops.0, ops.1, Operator::Or), Simplification::UseComparison);
            m.insert((ops.1, ops.0, Operator::Or), Simplification::UseComparison);
        }
        // Use equality
        for ops in [(Operator::Ge, Operator::Eq), (Operator::Le, Operator::Eq)] {
            m.insert((ops.0, ops.1, Operator::And), Simplification::UseEquality);
            m.insert((ops.1, ops.0, Operator::And), Simplification::UseEquality);
        }
        m
    };
}

simple_visitor!(
    CombinableBoolConditionsVisitor,
    fn visit_exp_custom(&mut self, exp: &T::Exp) -> bool {
        if let UnannotatedExp_::BinopExp(e1, op, _, e2) = &exp.exp.value {
            if let (
                UnannotatedExp_::BinopExp(lhs1, op1, _, rhs1),
                UnannotatedExp_::BinopExp(lhs2, op2, _, rhs2),
            ) = (&e1.exp.value, &e2.exp.value)
            {
                check_combinable_conditions(
                    self,
                    exp.exp.loc,
                    lhs1,
                    rhs1,
                    lhs2,
                    rhs2,
                    &op1.value,
                    &op2.value,
                    &op.value,
                );
            }
        }

        false
    }
);

fn check_combinable_conditions(
    context: &mut Context,
    loc: Loc,
    lhs1: &T::Exp,
    rhs1: &T::Exp,
    lhs2: &T::Exp,
    rhs2: &T::Exp,
    op1: &BinOp_,
    op2: &BinOp_,
    parent_op: &BinOp_,
) {
    if lhs1 == lhs2 && rhs1 == rhs2 && !is_module_call(lhs1) && !is_module_call(rhs1) {
        let key = (
            Operator::from(op1),
            Operator::from(op2),
            Operator::from(parent_op),
        );
        if let Some(simplification) = OPERATOR_COMBINATIONS.get(&key) {
            let diagnostic = diag!(
                StyleCodes::CombinableBoolConditions.diag_info(),
                (loc, simplification.message())
            );
            context.add_diag(diagnostic); // Using context instead of self
        }
    }
}

fn is_module_call(exp: &T::Exp) -> bool {
    matches!(exp.exp.value, UnannotatedExp_::ModuleCall(_))
}
