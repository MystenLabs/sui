// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! The `CombinableBool` detects and warns about boolean conditions in Move code that can be simplified.
//! It identifies comparisons that are logically equivalent and suggests more concise alternatives.
//! This rule focuses on simplifying expressions involving `==`, `<`, `>`, and `!=` operators to improve code readability.

use crate::{
    cfgir::visitor::{same_value_exp, simple_visitor},
    diag,
    hlir::ast::{self as H, UnannotatedExp_},
    linters::StyleCodes,
    parser::ast::{BinOp, BinOp_},
};
use move_ir_types::location::*;

#[derive(Debug, Clone, Copy)]
enum Simplification {
    Reducible(InnerOp_),
    AlwaysTrue,
    AlwaysFalse,
}

// impl Simplification {
//     fn message(&self) -> &'static str {
//         match self {
//             Simplification::SameOp
//             Simplification::Contradiction => {
//                 "This is always contradictory and can be simplified to false"
//             }
//             Simplification::UseComparison => "Consider simplifying to `<=` or `>=` respectively.",
//             Simplification::UseEquality => "Consider simplifying to `==`.",
//         }
//     }
// }

#[derive(Debug, Clone, Copy)]
enum OuterOp_ {
    And,
    Or,
}
type OuterOp = Spanned<OuterOp_>;

#[derive(Debug, Clone, Copy)]
enum InnerOp_ {
    Eq,
    Neq,
    Lt,
    Gt,
    Le,
    Ge,
}
type InnerOp = Spanned<InnerOp_>;

simple_visitor!(
    CombinableBoolConditionsVisitor,
    fn visit_exp_custom(&mut self, exp: &H::Exp) -> bool {
        use H::UnannotatedExp_ as E;
        let E::BinopExp(outer_l, outer_bop, outer_r) = &exp.exp.value else {
            return false;
        };
        let E::BinopExp(l1, op_l, r1) = &outer_l.exp.value else {
            return false;
        };
        let E::BinopExp(l2, op_r, r2) = &outer_r.exp.value else {
            return false;
        };
        let Some((outer, inner_l, inner_r)) = binop_case(outer_bop, l1, op_l, r1, l2, op_r, r2)
        else {
            return false;
        };
        let simplification = match outer.value {
            OuterOp_::And => simplify_and(inner_l, inner_r),
            OuterOp_::Or => simplify_or(inner_l, inner_r),
        };
        let msg = match simplification {
            Simplification::Reducible(inner_op) => {
                format!("to just the operation '{}'", inner_op)
            }
            Simplification::AlwaysTrue => "is always 'true'".to_string(),
            Simplification::AlwaysFalse => "is always 'false'".to_string(),
        };
        self.reporter.add_diag(diag!(
            StyleCodes::CombinableBoolConditions.diag_info(),
            (exp.exp.loc, format!("This comparison {msg}")),
        ));

        false
    }
);

fn simplify_and(op1: InnerOp, op2: InnerOp) -> Simplification {
    use InnerOp_ as I;
    Simplification::Reducible(match (op1.value, op2.value) {
        // same operation
        (I::Eq, I::Eq)
        | (I::Neq, I::Neq)
        | (I::Ge, I::Ge)
        | (I::Le, I::Le)
        | (I::Lt, I::Lt)
        | (I::Gt, I::Gt) => op1.value,

        // contradiction
        (I::Lt, I::Gt)
        | (I::Gt, I::Lt)
        | (I::Lt, I::Ge)
        | (I::Ge, I::Lt)
        | (I::Le, I::Gt)
        | (I::Gt, I::Le)
        | (I::Eq, I::Lt)
        | (I::Lt, I::Eq)
        | (I::Eq, I::Gt)
        | (I::Gt, I::Eq)
        | (I::Neq, I::Eq)
        | (I::Eq, I::Neq) => return Simplification::AlwaysFalse,

        // ==
        (I::Le, I::Ge)
        | (I::Ge, I::Le)
        | (I::Ge, I::Eq)
        | (I::Eq, I::Ge)
        | (I::Le, I::Eq)
        | (I::Eq, I::Le) => I::Eq,

        // <
        (I::Lt, I::Le)
        | (I::Le, I::Lt)
        | (I::Lt, I::Neq)
        | (I::Neq, I::Lt)
        | (I::Le, I::Neq)
        | (I::Neq, I::Le) => I::Lt,
        // >
        (I::Gt, I::Ge)
        | (I::Ge, I::Gt)
        | (I::Gt, I::Neq)
        | (I::Neq, I::Gt)
        | (I::Ge, I::Neq)
        | (I::Neq, I::Ge) => I::Gt,
    })
}

fn simplify_or(op1: InnerOp, op2: InnerOp) -> Simplification {
    use InnerOp_ as I;
    Simplification::Reducible(match (op1.value, op2.value) {
        // same operation
        (I::Eq, I::Eq)
        | (I::Neq, I::Neq)
        | (I::Ge, I::Ge)
        | (I::Le, I::Le)
        | (I::Lt, I::Lt)
        | (I::Gt, I::Gt) => op1.value,

        // tautology
        (I::Neq, I::Le)
        | (I::Neq, I::Ge)
        | (I::Le, I::Neq)
        | (I::Ge, I::Neq)
        | (I::Gt, I::Le)
        | (I::Le, I::Gt)
        | (I::Lt, I::Ge)
        | (I::Ge, I::Lt)
        | (I::Ge, I::Le)
        | (I::Le, I::Ge)
        | (I::Neq, I::Eq)
        | (I::Eq, I::Neq) => return Simplification::AlwaysTrue,

        // !=
        (I::Neq, I::Lt)
        | (I::Neq, I::Gt)
        | (I::Lt, I::Neq)
        | (I::Gt, I::Neq)
        | (I::Lt, I::Gt)
        | (I::Gt, I::Lt) => I::Neq,

        // <=
        (I::Lt, I::Le)
        | (I::Le, I::Lt)
        | (I::Eq, I::Lt)
        | (I::Lt, I::Eq)
        | (I::Eq, I::Le)
        | (I::Le, I::Eq) => I::Le,
        // >=
        (I::Gt, I::Ge)
        | (I::Ge, I::Gt)
        | (I::Eq, I::Gt)
        | (I::Eq, I::Ge)
        | (I::Gt, I::Eq)
        | (I::Ge, I::Eq) => I::Ge,
    })
}

fn outer(sp!(loc, bop_): &BinOp) -> Option<OuterOp> {
    let op_ = match bop_ {
        BinOp_::And => OuterOp_::And,
        BinOp_::Or => OuterOp_::Or,
        BinOp_::Eq
        | BinOp_::Lt
        | BinOp_::Gt
        | BinOp_::Le
        | BinOp_::Ge
        | BinOp_::Add
        | BinOp_::Sub
        | BinOp_::Mul
        | BinOp_::Mod
        | BinOp_::Div
        | BinOp_::BitOr
        | BinOp_::BitAnd
        | BinOp_::Xor
        | BinOp_::Shl
        | BinOp_::Shr
        | BinOp_::Range
        | BinOp_::Implies
        | BinOp_::Iff
        | BinOp_::Neq => return None,
    };
    Some(sp(*loc, op_))
}

fn inner(sp!(loc, bop_): &BinOp) -> Option<InnerOp> {
    let op_ = match bop_ {
        BinOp_::Eq => InnerOp_::Eq,
        BinOp_::Neq => InnerOp_::Neq,
        BinOp_::Lt => InnerOp_::Lt,
        BinOp_::Gt => InnerOp_::Gt,
        BinOp_::Le => InnerOp_::Le,
        BinOp_::Ge => InnerOp_::Ge,

        BinOp_::Add
        | BinOp_::Sub
        | BinOp_::Mul
        | BinOp_::Mod
        | BinOp_::Div
        | BinOp_::BitOr
        | BinOp_::BitAnd
        | BinOp_::Xor
        | BinOp_::Shl
        | BinOp_::Shr
        | BinOp_::Range
        | BinOp_::Implies
        | BinOp_::Iff
        | BinOp_::And
        | BinOp_::Or => return None,
    };
    Some(sp(*loc, op_))
}

fn flip(sp!(loc, op_): InnerOp) -> InnerOp {
    sp(
        loc,
        match op_ {
            InnerOp_::Eq => InnerOp_::Eq,
            InnerOp_::Neq => InnerOp_::Neq,
            InnerOp_::Lt => InnerOp_::Gt,
            InnerOp_::Gt => InnerOp_::Lt,
            InnerOp_::Le => InnerOp_::Ge,
            InnerOp_::Ge => InnerOp_::Le,
        },
    )
}

fn binop_case(
    outer_bop: &BinOp,
    l1: &H::Exp,
    op_l: &BinOp,
    r1: &H::Exp,
    l2: &H::Exp,
    op_r: &BinOp,
    r2: &H::Exp,
) -> Option<(OuterOp, InnerOp, InnerOp)> {
    let outer = outer(outer_bop)?;
    let inner_l = inner(op_l)?;
    let inner_r = inner(op_r)?;
    let (inner_l, inner_r) = operand_case(l1, inner_l, r1, l2, inner_r, r2)?;
    Some((outer, inner_l, inner_r))
}

fn operand_case(
    l1: &H::Exp,
    op1: InnerOp,
    r1: &H::Exp,
    l2: &H::Exp,
    op2: InnerOp,
    r2: &H::Exp,
) -> Option<(InnerOp, InnerOp)> {
    if same_value_exp(l1, l2) && same_value_exp(r1, r2) {
        // a1 := l1
        // a2 := l2
        // b1 := r1
        // b2 := r2
        if same_value_exp(l1, r1) {
            // Covered by EqualOperands
            None
        } else {
            Some((op1, op2))
        }
    } else if same_value_exp(l1, r2) && same_value_exp(r1, l2) {
        // a1 := l1
        // a2 := r2
        // b1 := r1
        // b2 := l2
        Some((op1, flip(op2)))
    } else {
        None
    }
}

impl std::fmt::Display for InnerOp_ {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use InnerOp_ as I;
        write!(
            f,
            "{}",
            match self {
                I::Eq => "==",
                I::Neq => "!=",
                I::Lt => "<",
                I::Gt => ">",
                I::Le => "<=",
                I::Ge => ">=",
            }
        )
    }
}
