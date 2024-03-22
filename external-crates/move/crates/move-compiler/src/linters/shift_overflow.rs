//! Detect potential overflow scenarios where the number of bits being shifted exceeds the bit width of
//! the variable being shifted, which could lead to unintended behavior or loss of data. If such a
//! potential overflow is detected, a warning is generated to alert the developer.
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    expansion::ast::Value_,
    naming::ast::{BuiltinTypeName_, TypeName_, Type_},
    parser::ast::BinOp_,
    shared::{program_info::TypingProgramInfo, CompilationEnv},
    typing::{
        ast::{self as T, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;
use std::str::FromStr;

use super::{LinterDiagCategory, LINTER_DEFAULT_DIAG_CODE, LINT_WARNING_PREFIX};

const SHIFT_OPERATION_OVERFLOW_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagCategory::ShiftOperationOverflow as u8,
    LINTER_DEFAULT_DIAG_CODE,
    "Potential overflow detected. The number of bits being shifted exceeds the bit width of the variable being shifted.",
);

pub struct ShiftOperationOverflow;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for ShiftOperationOverflow {
    type Context<'a> = Context<'a>;

    fn context<'a>(
        env: &'a mut CompilationEnv,
        _program_info: &'a TypingProgramInfo,
        _program: &T::Program_,
    ) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        // Check if the expression is a binary operation and if it is a shift operation.
        if let UnannotatedExp_::BinopExp(lhs, op, _, rhs) = &exp.exp.value {
            // can't do  let UnannotatedExp_::BinopExp(lhs, BinOp_::Shl | BinOp_::Shr, _, rhs) = &exp.exp.value else { return };
            // because the op is Spanned<BinOp_> and not BinOp_
            if matches!(op.value, BinOp_::Shl | BinOp_::Shr) {
                match (
                    get_bit_width(&lhs.ty.value),
                    get_shift_amount(&rhs.exp.value),
                ) {
                    (Some(bit_width), Some(shift_amount)) if shift_amount >= bit_width => {
                        report_overflow(self.env, shift_amount, bit_width, op.loc);
                    }
                    _ => (),
                }
            }
        }
        false
    }
    fn add_warning_filter_scope(&mut self, filter: WarningFilters) {
        self.env.add_warning_filter_scope(filter)
    }

    fn pop_warning_filter_scope(&mut self) {
        self.env.pop_warning_filter_scope()
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
    if let UnannotatedExp_::Value(v) = value {
        match &v.value {
            Value_::U8(v) => Some(*v as u128),
            _ => None,
        }
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
