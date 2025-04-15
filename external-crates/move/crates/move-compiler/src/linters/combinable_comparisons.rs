// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! The `CombinableBool` detects and warns about boolean conditions in Move code that can be simplified.
//! It identifies comparisons that are logically equivalent and suggests more concise alternatives.
//! This rule focuses on simplifying expressions involving `==`, `<`, `>`, and `!=` operators to improve code readability.

use crate::{
    diag,
    linters::StyleCodes,
    parser::ast::{BinOp, BinOp_},
    typing::{
        ast::{self as T},
        visitor::{same_value_exp, simple_visitor},
    },
};

#[derive(Debug, Clone, Copy)]
enum Simplification {
    Reducible(CmpOp),
    AlwaysTrue,
    AlwaysFalse,
}

#[derive(Debug, Clone, Copy)]
enum BoolOp {
    And,
    Or,
}

/// See `simplify` for how these values are used.
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
enum CmpOp {
    Lt = LT,
    Eq = EQ,
    Le = LE,
    Gt = GT,
    Neq = NEQ,
    Ge = GE,
}

// See `simplify` for how these values are used.
const FALSE: u8 = 0b000;
const LT: u8 = 0b001;
const EQ: u8 = 0b010;
const LE: u8 = 0b011;
const GT: u8 = 0b100;
const NEQ: u8 = 0b101;
const GE: u8 = 0b110;
const TRUE: u8 = 0b111;

simple_visitor!(
    CombinableComparisons,
    fn visit_exp_custom(&mut self, exp: &T::Exp) -> bool {
        use T::UnannotatedExp_ as E;
        let E::BinopExp(outer_l, outer_bop, _, outer_r) = &exp.exp.value else {
            return false;
        };
        // TODO handle negation
        let E::BinopExp(l1, op_l, _, r1) = &outer_l.exp.value else {
            return false;
        };
        let E::BinopExp(l2, op_r, _, r2) = &outer_r.exp.value else {
            return false;
        };
        let Some((outer, inner_l, inner_r)) = binop_case(outer_bop, l1, op_l, r1, l2, op_r, r2)
        else {
            return false;
        };
        let simplification = simplify(outer, inner_l, inner_r);
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

/// Each binary operator is represented as a 3-bit number where each bit represents a range of
/// possible values. With three bits, 0bGEL we are "drawing" an interval of ranges. The comparison
/// `true` if the value is within the interval. so for `x cmp y``
/// ```text
/// G E L
///     ^ this bit represents x < y (less than the equal bit)
///   ^ this bit represents x == y (the equal bit)
/// ^ this bit represents x > y (greater than the equal bit)
/// ```
/// We then take the disjunction of intervals by the bits--creating a bitset.
/// So for example, `>=` is 0b110 since the interval is either greater OR equal.
/// And for `!=` is 0b101 since the interval is either not equal OR less than. We are only dealing
/// with primitives so we know the values are well ordered.
/// From there we can then bitwise-or the bits (set union) when the outer operation is `||` and
/// bitwise-and the bits (set intersection) when the outer operation is `&&` to get the final
/// "simplified" operation. If all bits are set, then the operation is always true. If no bits are
/// set, then the operation is always false.
fn simplify(outer: BoolOp, inner_l: CmpOp, inner_r: CmpOp) -> Simplification {
    let lbits = inner_l as u8;
    let rbits = inner_r as u8;
    let simplification = match outer {
        BoolOp::And => lbits & rbits,
        BoolOp::Or => lbits | rbits,
    };
    match simplification {
        FALSE => Simplification::AlwaysFalse,
        LT => Simplification::Reducible(CmpOp::Lt),
        EQ => Simplification::Reducible(CmpOp::Eq),
        LE => Simplification::Reducible(CmpOp::Le),
        GT => Simplification::Reducible(CmpOp::Gt),
        NEQ => Simplification::Reducible(CmpOp::Neq),
        GE => Simplification::Reducible(CmpOp::Ge),
        TRUE => Simplification::AlwaysTrue,
        _ => unreachable!(),
    }
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
    l1: &T::Exp,
    op_l: &BinOp,
    r1: &T::Exp,
    l2: &T::Exp,
    op_r: &BinOp,
    r2: &T::Exp,
) -> Option<(BoolOp, CmpOp, CmpOp)> {
    let outer = bool_op(outer_bop)?;
    let inner_l = cmp_op(op_l)?;
    let inner_r = cmp_op(op_r)?;
    let (inner_l, inner_r) = operand_case(l1, inner_l, r1, l2, inner_r, r2)?;
    Some((outer, inner_l, inner_r))
}

fn operand_case(
    l1: &T::Exp,
    op1: CmpOp,
    r1: &T::Exp,
    l2: &T::Exp,
    op2: CmpOp,
    r2: &T::Exp,
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
