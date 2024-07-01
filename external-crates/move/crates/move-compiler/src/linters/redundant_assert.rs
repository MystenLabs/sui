//! Detects and warns about `assert!(true)` and `assert!(false)` calls in the code.
//! `assert!(true)` is redundant and can be removed for cleaner code.
//! `assert!(false)` indicates an always-failing code path, suggesting a need for review or refactor.
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    expansion::ast::Value_,
    shared::CompilationEnv,
    typing::{
        ast::{self as T, BuiltinFunction_, ExpListItem, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;

use super::{LinterDiagnosticCategory, LINT_WARNING_PREFIX, REDUNDANT_ASSERT_DIAG_CODE};

const REDUNDANT_ASSERT_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Correctness as u8,
    REDUNDANT_ASSERT_DIAG_CODE,
    "",
);

pub struct AssertTrueFals;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for AssertTrueFals {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        if let UnannotatedExp_::Builtin(builtin_func, assert_exp) = &exp.exp.value {
            if let BuiltinFunction_::Assert(_) = builtin_func.value {
                if let UnannotatedExp_::ExpList(args) = &assert_exp.exp.value {
                    if args.len() == 2 {
                        if let ExpListItem::Single(item_exp, _) = &args[0] {
                            if let UnannotatedExp_::Value(value) = &item_exp.exp.value {
                                if let Value_::Bool(b) = value.value {
                                    report_redundant_assert(self.env, assert_exp.exp.loc, b);
                                }
                            }
                        }
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

fn report_redundant_assert(env: &mut CompilationEnv, loc: Loc, bool_value: bool) {
    let msg = format!(
        "Detected a redundant `assert!({})` call. Consider removing it.",
        bool_value
    );
    let diag = diag!(REDUNDANT_ASSERT_DIAG, (loc, msg));
    env.add_diag(diag);
}
