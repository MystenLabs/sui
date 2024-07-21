//! Detects meaningless math operations like `x * 0`, `x << 0`, `x >> 0`, `x * 1`, `x + 0`, `x - 0`, and `x / 0`.
//! Aims to reduce code redundancy and improve clarity by flagging operations with no effect.
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    expansion::ast::Value_,
    parser::ast::BinOp_,
    shared::CompilationEnv,
    typing::{
        ast::{self as T, Exp, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;

use super::{LinterDiagnosticCategory, LINT_WARNING_PREFIX, MEANINGLESS_MATH_OP_DIAG_CODE};

const MEANINGLESS_MATH_OP_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Complexity as u8,
    MEANINGLESS_MATH_OP_DIAG_CODE,
    "Detected a meaningless math operation that has no effect.",
);

pub struct MeaninglessMathOperation;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for MeaninglessMathOperation {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        if let UnannotatedExp_::BinopExp(_, op, _, rhs) = &exp.exp.value {
            match op.value {
                BinOp_::Mul | BinOp_::Div => {
                    if is_zero(&rhs) || (matches!(op.value, BinOp_::Mul) && is_one(&rhs)) {
                        report_meaningless_math_op(self.env, op.loc);
                    }
                }
                BinOp_::Add | BinOp_::Sub => {
                    if is_zero(&rhs) {
                        report_meaningless_math_op(self.env, op.loc);
                    }
                }
                BinOp_::Shl | BinOp_::Shr => {
                    if is_zero(&rhs) {
                        report_meaningless_math_op(self.env, op.loc);
                    }
                }
                _ => {}
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

fn is_zero(exp: &Exp) -> bool {
    if let UnannotatedExp_::Value(spanned) = &exp.exp.value {
        matches!(spanned.value, Value_::U64(0))
    } else {
        false
    }
}

fn is_one(exp: &Exp) -> bool {
    if let UnannotatedExp_::Value(spanned) = &exp.exp.value {
        matches!(spanned.value, Value_::U64(1))
    } else {
        false
    }
}

fn report_meaningless_math_op(env: &mut CompilationEnv, loc: Loc) {
    let diag = diag!(
        MEANINGLESS_MATH_OP_DIAG,
        (
            loc,
            "Detected a meaningless math operation that has no effect.",
        )
    );
    env.add_diag(diag);
}
