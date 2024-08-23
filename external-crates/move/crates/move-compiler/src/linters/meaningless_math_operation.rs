// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Detects meaningless math operations like `x * 0`, `x << 0`, `x >> 0`, `x * 1`, `x + 0`, `x - 0`
//! Aims to reduce code redundancy and improve clarity by flagging operations with no effect.
use crate::{
    cfgir::ast as G,
    cfgir::visitor::{CFGIRVisitorConstructor, CFGIRVisitorContext},
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    hlir::ast::{self as H, Value_},
    parser::ast::BinOp_,
    shared::CompilationEnv,
};
use move_core_types::u256::U256;
use move_ir_types::location::Loc;

use super::{LinterDiagnosticCategory, LINT_WARNING_PREFIX, MEANINGLESS_MATH_DIAG_CODE};

const MEANINGLESS_MATH_OP_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Complexity as u8,
    MEANINGLESS_MATH_DIAG_CODE,
    "math operation can be simplified",
);

pub struct MeaninglessMathOperation;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl CFGIRVisitorConstructor for MeaninglessMathOperation {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &G::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl CFGIRVisitorContext for Context<'_> {
    fn add_warning_filter_scope(&mut self, filter: WarningFilters) {
        self.env.add_warning_filter_scope(filter)
    }
    fn pop_warning_filter_scope(&mut self) {
        self.env.pop_warning_filter_scope()
    }

    fn visit_exp_custom(&mut self, exp: &mut H::Exp) -> bool {
        let H::UnannotatedExp_::BinopExp(lhs, op, rhs) = &exp.exp.value else {
            return false;
        };

        // unchanged operations
        let is_unchanged = match op.value {
            BinOp_::Mul => is_one(lhs).or(is_one(rhs)),
            BinOp_::Div => is_one(rhs),
            BinOp_::Add => is_zero(lhs).or(is_zero(rhs)),
            BinOp_::Sub => is_zero(rhs),
            BinOp_::Shl | BinOp_::Shr => is_zero(rhs),
            _ => None,
        };
        if let Some(meaningless_operand) = is_unchanged {
            let msg = "This operation has no effect and can be removed";
            self.env.add_diag(diag!(
                MEANINGLESS_MATH_OP_DIAG,
                (exp.exp.loc, msg),
                (meaningless_operand, "Because of this operand"),
            ));
        }

        // always zero
        let is_always_zero = match op.value {
            BinOp_::Mul => is_zero(lhs).or(is_zero(rhs)),
            BinOp_::Div => is_zero(lhs),
            BinOp_::Mod => is_zero(lhs).or(is_one(rhs)),
            _ => None,
        };
        if let Some(zero_operand) = is_always_zero {
            let msg = "This operation is always zero and can be replaced with '0'";
            self.env.add_diag(diag!(
                MEANINGLESS_MATH_OP_DIAG,
                (exp.exp.loc, msg),
                (zero_operand, "Because of this operand"),
            ));
        }

        // always one
        let is_always_one = match op.value {
            BinOp_::Mod => is_one(lhs),
            _ => None,
        };
        if let Some(one_operand) = is_always_one {
            let msg = "This operation is always one and can be replaced with '1'";
            self.env.add_diag(diag!(
                MEANINGLESS_MATH_OP_DIAG,
                (exp.exp.loc, msg),
                (one_operand, "Because of this operand"),
            ));
        }

        // for aborts, e.g. x / 0, we will let the optimizer give a warning

        false
    }
}

fn is_zero(exp: &H::Exp) -> Option<Loc> {
    let H::UnannotatedExp_::Value(sp!(loc, value_)) = &exp.exp.value else {
        return None;
    };
    match value_ {
        Value_::U8(0) | Value_::U16(0) | Value_::U32(0) | Value_::U64(0) | Value_::U128(0) => {
            Some(*loc)
        }
        Value_::U256(u) if u == &U256::zero() => Some(*loc),
        Value_::U8(_)
        | Value_::U16(_)
        | Value_::U32(_)
        | Value_::U64(_)
        | Value_::U128(_)
        | Value_::U256(_)
        | Value_::Address(_)
        | Value_::Bool(_)
        | Value_::Vector(_, _) => None,
    }
}

fn is_one(exp: &H::Exp) -> Option<Loc> {
    let H::UnannotatedExp_::Value(sp!(loc, value_)) = &exp.exp.value else {
        return None;
    };
    match value_ {
        Value_::U8(1) | Value_::U16(1) | Value_::U32(1) | Value_::U64(1) | Value_::U128(1) => {
            Some(*loc)
        }
        Value_::U256(u) if u == &U256::one() => Some(*loc),
        Value_::U8(_)
        | Value_::U16(_)
        | Value_::U32(_)
        | Value_::U64(_)
        | Value_::U128(_)
        | Value_::U256(_)
        | Value_::Address(_)
        | Value_::Bool(_)
        | Value_::Vector(_, _) => None,
    }
}
