use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    shared::{program_info::TypingProgramInfo, CompilationEnv},
    sui_mode::linters::{LinterDiagCategory, LINTER_DEFAULT_DIAG_CODE, LINT_WARNING_PREFIX},
    typing::{
        ast::{self as T, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;

const REDUNDANT_REF_DEREF_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagCategory::RedundantRefDeref as u8,
    LINTER_DEFAULT_DIAG_CODE,
    "",
);

pub struct RedundantRefDerefVisitor;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for RedundantRefDerefVisitor {
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
        // If the expression is a temporary borrow followed by a dereference that is itself a local borrow,
        // it indicates a redundant ref-deref pattern.
        if let UnannotatedExp_::TempBorrow(_, borrow_exp) = &exp.exp.value {
            if let UnannotatedExp_::Dereference(deref_exp) = &borrow_exp.exp.value {
                if let UnannotatedExp_::BorrowLocal(_, _) = &deref_exp.exp.value {
                    report_ref_deref(self.env, exp.exp.loc);
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

fn report_ref_deref(env: &mut CompilationEnv, loc: Loc) {
    let diag = diag!(
       REDUNDANT_REF_DEREF_DIAG,
        (loc, "Redundant borrow-dereference detected. Consider removing the borrow-dereference operation and using the expression directly.")
    );
    env.add_diag(diag);
}
