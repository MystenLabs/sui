//! Detects comparisons where a variable is compared to 'true' or 'false' using
//! equality (==) or inequality (!=) operators and provides suggestions to simplify the comparisons.
//! Examples: if (x == true) can be simplified to if (x), if (x == false) can be simplified to if (!x)
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    expansion::ast::Value_,
    parser::ast::BinOp_,
    shared::{program_info::TypingProgramInfo, CompilationEnv},
    typing::{
        ast::{self as T, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;

use super::{LinterDiagCategory, LINTER_DEFAULT_DIAG_CODE, LINT_WARNING_PREFIX};

const BOOL_COMPARISON_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagCategory::UnnecessaryBoolComparison as u8,
    LINTER_DEFAULT_DIAG_CODE,
    "unnecessary boolean comparison to true or false",
);

pub struct BoolComparison;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for BoolComparison {
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
        if let UnannotatedExp_::BinopExp(e1, op, _, e2) = &exp.exp.value {
            // Check if the operation is an equality comparison
            if let BinOp_::Eq | BinOp_::Neq = &op.value {
                // Check if one side is a boolean literal and the other is a boolean expression
                let bool_comparison = match (&e1.exp.value, &e2.exp.value) {
                    (UnannotatedExp_::Value(v), _) | (_, UnannotatedExp_::Value(v)) => {
                        if let Value_::Bool(b) = &v.value {
                            Some(b)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                if let Some(b) = bool_comparison {
                    let simplification = match (op.value, b) {
                        (BinOp_::Eq, true) | (BinOp_::Neq, false) => {
                            "Consider simplifying this expression to the variable itself."
                        }
                        (BinOp_::Eq, false) | (BinOp_::Neq, true) => {
                            "Consider simplifying this expression using logical negation (!)."
                        }
                        _ => "", // This case should not occur
                    };

                    if !simplification.is_empty() {
                        let loc = exp.exp.loc;
                        add_bool_comparison_diag(self.env, loc, simplification);
                    }
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

fn add_bool_comparison_diag(env: &mut CompilationEnv, loc: Loc, message: &str) {
    let d = diag!(
        BOOL_COMPARISON_DIAG,
        (
            loc,
            format!("This boolean comparison is unnecessary. {}", message)
        )
    );
    env.add_diag(d);
}
