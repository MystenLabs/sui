// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! The `CombinableBool` detects and warns about boolean conditions in Move code that can be simplified.
//! It identifies comparisons that are logically equivalent and suggests more concise alternatives.
//! This rule focuses on simplifying expressions involving `==`, `<`, `>`, and `!=` operators to improve code readability.

use crate::{
    cfgir::visitor::{same_value_exp, simple_visitor},
    diag,
    hlir::ast::{self as H},
    linters::StyleCodes,
    parser::ast::{BinOp, BinOp_},
};

#[derive(Debug, Clone, Copy)]
enum Simplification {
    Reducible(CmpOp),
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
enum BoolOp {
    And,
    Or,
}

#[derive(Debug, Clone, Copy)]
enum CmpOp {
    Eq,
    Neq,
    Lt,
    Gt,
    Le,
    Ge,
}

simple_visitor!(
    CombinableComparisons,
    fn visit_exp_custom(&mut self, exp: &H::Exp) -> bool {
        use H::UnannotatedExp_ as E;
        let E::BinopExp(outer_l, outer_bop, outer_r) = &exp.exp.value else {
            return false;
        };
        // TODO handle negation
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
        let simplification = match outer {
            BoolOp::And => simplify_and(inner_l, inner_r),
            BoolOp::Or => simplify_or(inner_l, inner_r),
        };
        let msg = match simplification {
            Simplification::Reducible(inner_op) => {
                format!("simplifies to the operation '{}'", inner_op)
            }
            Simplification::AlwaysTrue => "is always 'true'".to_string(),
            Simplification::AlwaysFalse => "is always 'false'".to_string(),
        };
        self.reporter.add_diag(diag!(
            StyleCodes::CombinableComparisons.diag_info(),
            (exp.exp.loc, format!("This comparison {msg}")),
        ));

        false
    }
);

fn simplify_and(op1: CmpOp, op2: CmpOp) -> Simplification {
    use CmpOp as C;
    Simplification::Reducible(match (op1, op2) {
        // same operation
        (C::Eq, C::Eq)
        | (C::Neq, C::Neq)
        | (C::Ge, C::Ge)
        | (C::Le, C::Le)
        | (C::Lt, C::Lt)
        | (C::Gt, C::Gt) => op1,

        // contradiction
        (C::Lt, C::Gt)
        | (C::Gt, C::Lt)
        | (C::Lt, C::Ge)
        | (C::Ge, C::Lt)
        | (C::Le, C::Gt)
        | (C::Gt, C::Le)
        | (C::Eq, C::Lt)
        | (C::Lt, C::Eq)
        | (C::Eq, C::Gt)
        | (C::Gt, C::Eq)
        | (C::Neq, C::Eq)
        | (C::Eq, C::Neq) => return Simplification::AlwaysFalse,

        // ==
        (C::Le, C::Ge)
        | (C::Ge, C::Le)
        | (C::Ge, C::Eq)
        | (C::Eq, C::Ge)
        | (C::Le, C::Eq)
        | (C::Eq, C::Le) => C::Eq,

        // <
        (C::Lt, C::Le)
        | (C::Le, C::Lt)
        | (C::Lt, C::Neq)
        | (C::Neq, C::Lt)
        | (C::Le, C::Neq)
        | (C::Neq, C::Le) => C::Lt,
        // >
        (C::Gt, C::Ge)
        | (C::Ge, C::Gt)
        | (C::Gt, C::Neq)
        | (C::Neq, C::Gt)
        | (C::Ge, C::Neq)
        | (C::Neq, C::Ge) => C::Gt,
    })
}

fn simplify_or(op1: CmpOp, op2: CmpOp) -> Simplification {
    use CmpOp as C;
    Simplification::Reducible(match (op1, op2) {
        // same operation
        (C::Eq, C::Eq)
        | (C::Neq, C::Neq)
        | (C::Ge, C::Ge)
        | (C::Le, C::Le)
        | (C::Lt, C::Lt)
        | (C::Gt, C::Gt) => op1,

        // tautology
        (C::Neq, C::Le)
        | (C::Neq, C::Ge)
        | (C::Le, C::Neq)
        | (C::Ge, C::Neq)
        | (C::Gt, C::Le)
        | (C::Le, C::Gt)
        | (C::Lt, C::Ge)
        | (C::Ge, C::Lt)
        | (C::Ge, C::Le)
        | (C::Le, C::Ge)
        | (C::Neq, C::Eq)
        | (C::Eq, C::Neq) => return Simplification::AlwaysTrue,

        // !=
        (C::Neq, C::Lt)
        | (C::Neq, C::Gt)
        | (C::Lt, C::Neq)
        | (C::Gt, C::Neq)
        | (C::Lt, C::Gt)
        | (C::Gt, C::Lt) => C::Neq,

        // <=
        (C::Lt, C::Le)
        | (C::Le, C::Lt)
        | (C::Eq, C::Lt)
        | (C::Lt, C::Eq)
        | (C::Eq, C::Le)
        | (C::Le, C::Eq) => C::Le,
        // >=
        (C::Gt, C::Ge)
        | (C::Ge, C::Gt)
        | (C::Eq, C::Gt)
        | (C::Eq, C::Ge)
        | (C::Gt, C::Eq)
        | (C::Ge, C::Eq) => C::Ge,
    })
}

fn bool_op(sp!(_, bop_): &BinOp) -> Option<BoolOp> {
    Some(match bop_ {
        BinOp_::And => BoolOp::And,
        BinOp_::Or => BoolOp::Or,
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
    })
}

fn cmp_op(sp!(_, bop_): &BinOp) -> Option<CmpOp> {
    Some(match bop_ {
        BinOp_::Eq => CmpOp::Eq,
        BinOp_::Neq => CmpOp::Neq,
        BinOp_::Lt => CmpOp::Lt,
        BinOp_::Gt => CmpOp::Gt,
        BinOp_::Le => CmpOp::Le,
        BinOp_::Ge => CmpOp::Ge,

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
    })
}

fn flip(op: CmpOp) -> CmpOp {
    match op {
        CmpOp::Eq => CmpOp::Eq,
        CmpOp::Neq => CmpOp::Neq,
        CmpOp::Lt => CmpOp::Gt,
        CmpOp::Gt => CmpOp::Lt,
        CmpOp::Le => CmpOp::Ge,
        CmpOp::Ge => CmpOp::Le,
    }
}

fn binop_case(
    outer_bop: &BinOp,
    l1: &H::Exp,
    op_l: &BinOp,
    r1: &H::Exp,
    l2: &H::Exp,
    op_r: &BinOp,
    r2: &H::Exp,
) -> Option<(BoolOp, CmpOp, CmpOp)> {
    let outer = bool_op(outer_bop)?;
    let inner_l = cmp_op(op_l)?;
    let inner_r = cmp_op(op_r)?;
    let (inner_l, inner_r) = operand_case(l1, inner_l, r1, l2, inner_r, r2)?;
    Some((outer, inner_l, inner_r))
}

fn operand_case(
    l1: &H::Exp,
    op1: CmpOp,
    r1: &H::Exp,
    l2: &H::Exp,
    op2: CmpOp,
    r2: &H::Exp,
) -> Option<(CmpOp, CmpOp)> {
    if same_value_exp(l1, l2) && same_value_exp(r1, r2) {
        Some((op1, op2))
    } else if same_value_exp(l1, r2) && same_value_exp(r1, l2) {
        Some((op1, flip(op2)))
    } else {
        None
    }
}

impl std::fmt::Display for CmpOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                CmpOp::Eq => "==",
                CmpOp::Neq => "!=",
                CmpOp::Lt => "<",
                CmpOp::Gt => ">",
                CmpOp::Le => "<=",
                CmpOp::Ge => ">=",
            }
        )
    }
}
