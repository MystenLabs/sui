// Copyright (c) Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Detect potential overflow scenarios where the number of bits being shifted exceeds the bit width of
//! the variable being shifted, which could lead to unintended behavior or loss of data. If such a
//! potential overflow is detected, a warning is generated to alert the developer.
use super::{LinterDiagnosticCategory, LINT_WARNING_PREFIX, SHIFT_OVERFLOW_DIAG_CODE};
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    expansion::ast::Value_,
    naming::ast::{BuiltinTypeName_, Type_},
    parser::ast::BinOp_,
    shared::CompilationEnv,
    typing::{
        ast::{self as T, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;

const SHIFT_OPERATION_OVERFLOW_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Correctness as u8,
    SHIFT_OVERFLOW_DIAG_CODE,
    "Potential overflow detected. The number of bits being shifted exceeds the bit width of the variable being shifted.",
);

pub struct ShiftOperationOverflow;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for ShiftOperationOverflow {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn add_warning_filter_scope(&mut self, filter: WarningFilters) {
        self.env.add_warning_filter_scope(filter)
    }
    fn pop_warning_filter_scope(&mut self) {
        self.env.pop_warning_filter_scope()
    }

    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        // Check if the expression is a binary operation and if it is a shift operation.
        if let UnannotatedExp_::BinopExp(lhs, sp!(_, BinOp_::Shl | BinOp_::Shr), _, rhs) =
            &exp.exp.value
        {
            match (
                get_bit_width(&lhs.ty.value),
                get_shift_amount(&rhs.exp.value),
            ) {
                (Some(bit_width), Some(shift_amount)) if shift_amount >= bit_width => {
                    report_overflow(self.env, shift_amount, bit_width, exp.exp.loc);
                }
                _ => (),
            }
        }
        false
    }
}

fn get_bit_width(ty: &Type_) -> Option<u128> {
    ty.builtin_name().and_then(|typ| match typ.value {
        BuiltinTypeName_::U8 => Some(8),
        BuiltinTypeName_::U16 => Some(16),
        BuiltinTypeName_::U32 => Some(32),
        BuiltinTypeName_::U64 => Some(64),
        BuiltinTypeName_::U128 => Some(128),
        BuiltinTypeName_::U256 => Some(256),
        _ => None,
    })
}

fn get_shift_amount(value: &UnannotatedExp_) -> Option<u128> {
    if let UnannotatedExp_::BinopExp(lhs, op, _, rhs) = value {
        let shift_amount = match op {
            sp!(_, BinOp_::Mul) => match (&lhs.exp.value, &rhs.exp.value) {
                (
                    UnannotatedExp_::Value(sp!(_, Value_::U8(v1))),
                    UnannotatedExp_::Value(sp!(_, Value_::U8(v2))),
                ) => v1 * v2,
                _ => 0,
            },
            sp!(_, BinOp_::Add) => match (&lhs.exp.value, &rhs.exp.value) {
                (
                    UnannotatedExp_::Value(sp!(_, Value_::U8(v1))),
                    UnannotatedExp_::Value(sp!(_, Value_::U8(v2))),
                ) => v1 + v2,
                _ => 0,
            },
            sp!(_, BinOp_::Sub) => match (&lhs.exp.value, &rhs.exp.value) {
                (
                    UnannotatedExp_::Value(sp!(_, Value_::U8(v1))),
                    UnannotatedExp_::Value(sp!(_, Value_::U8(v2))),
                ) => v1 - v2,
                _ => 0,
            },
            sp!(_, BinOp_::Div) => match (&lhs.exp.value, &rhs.exp.value) {
                (
                    UnannotatedExp_::Value(sp!(_, Value_::U8(v1))),
                    UnannotatedExp_::Value(sp!(_, Value_::U8(v2))),
                ) => v1 / v2,
                _ => 0,
            },

            _ => 0,
        };
        return Some(shift_amount as u128);
    };
    if let UnannotatedExp_::Value(sp!(_, Value_::U8(v))) = value {
        Some(*v as u128)
    } else {
        None
    }
}

fn report_overflow(env: &mut CompilationEnv, shift_amount: u128, bit_width: u128, loc: Loc) {
    let msg = format!(
        "The {} of bits being shifted exceeds the {} bit width of the variable being shifted.",
        shift_amount, bit_width
    );
    let diag = diag!(SHIFT_OPERATION_OVERFLOW_DIAG, (loc, msg));
    env.add_diag(diag);
}
