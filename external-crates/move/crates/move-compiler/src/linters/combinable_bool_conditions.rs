//! The `CombinableBool` detects and warns about boolean conditions in Move code that can be simplified.
//! It identifies comparisons that are logically equivalent and suggests more concise alternatives.
//! This rule focuses on simplifying expressions involving `==`, `<`, `>`, and `!=` operators to improve code readability.
use move_ir_types::location::Loc;

use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    parser::ast::BinOp_,
    shared::CompilationEnv,
    typing::{
        ast::{self as T, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};

use super::{LinterDiagnosticCategory, COMBINABLE_COMPARISON_DIAG_CODE, LINT_WARNING_PREFIX};

const COMBINABLE_BOOL_COND_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Complexity as u8,
    COMBINABLE_COMPARISON_DIAG_CODE,
    "",
);

pub struct CombinableBool;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for CombinableBool {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        let UnannotatedExp_::BinopExp(e1, op, _, e2) = &exp.exp.value else {
            return false;
        };
        let (
            UnannotatedExp_::BinopExp(lhs1, op1, _, rhs1),
            UnannotatedExp_::BinopExp(lhs2, op2, _, rhs2),
        ) = (&e1.exp.value, &e2.exp.value)
        else {
            return false;
        };
        // Check both exp side are the same
        if lhs1 == lhs2 && rhs1 == rhs2 {
            if is_module_call(lhs1) || is_module_call(rhs1) {
                return false;
            };
            process_combinable_exp(self.env, exp.exp.loc, &op1.value, &op2.value, &op.value);
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

fn process_combinable_exp(
    env: &mut CompilationEnv,
    loc: Loc,
    op1: &BinOp_,
    op2: &BinOp_,
    parent_op: &BinOp_,
) {
    match (op1, op2) {
        (BinOp_::Eq, BinOp_::Lt)
        | (BinOp_::Lt, BinOp_::Eq)
        | (BinOp_::Eq, BinOp_::Gt)
        | (BinOp_::Gt, BinOp_::Eq)
        | (BinOp_::Ge, BinOp_::Eq)
        | (BinOp_::Eq, BinOp_::Ge)
        | (BinOp_::Le, BinOp_::Eq)
        | (BinOp_::Eq, BinOp_::Le)
        | (BinOp_::Neq, BinOp_::Lt)
        | (BinOp_::Lt, BinOp_::Neq)
        | (BinOp_::Neq, BinOp_::Gt)
        | (BinOp_::Gt, BinOp_::Neq) => {
            suggest_simplification(env, loc, op1, op2, parent_op);
        }
        _ => {}
    }
}

fn suggest_simplification(
    env: &mut CompilationEnv,
    loc: Loc,
    op1: &BinOp_,
    op2: &BinOp_,
    parent_op: &BinOp_,
) {
    let message = match (op1, op2, parent_op) {
        (BinOp_::Eq, BinOp_::Lt, BinOp_::And)
        | (BinOp_::Eq, BinOp_::Gt, BinOp_::And)
        | (BinOp_::Gt, BinOp_::Eq, BinOp_::And)
        | (BinOp_::Lt, BinOp_::Eq, BinOp_::And) => {
            "This is always contradictory and can be simplified to false"
        }
        (BinOp_::Eq, BinOp_::Lt, _)
        | (BinOp_::Eq, BinOp_::Gt, _)
        | (BinOp_::Lt, BinOp_::Eq, _)
        | (BinOp_::Gt, BinOp_::Eq, _) => "Consider simplifying to `<=` or `>=` respectively.",
        (BinOp_::Ge, BinOp_::Eq, BinOp_::And)
        | (BinOp_::Le, BinOp_::Eq, BinOp_::And)
        | (BinOp_::Eq, BinOp_::Le, BinOp_::And)
        | (BinOp_::Eq, BinOp_::Ge, BinOp_::And) => "Consider simplifying to `==`.",
        (BinOp_::Neq, BinOp_::Lt, BinOp_::And)
        | (BinOp_::Neq, BinOp_::Gt, BinOp_::And)
        | (BinOp_::Gt, BinOp_::Neq, BinOp_::And)
        | (BinOp_::Lt, BinOp_::Neq, BinOp_::And) => {
            "Consider simplifying to `<` or `>` respectively."
        }
        _ => return,
    };
    add_replaceable_comparison_diag(env, loc, message);
}

fn add_replaceable_comparison_diag(env: &mut CompilationEnv, loc: Loc, message: &str) {
    let d = diag!(COMBINABLE_BOOL_COND_DIAG, (loc, message));
    env.add_diag(d);
}

fn is_module_call(exp: &T::Exp) -> bool {
    matches!(exp.exp.value, UnannotatedExp_::ModuleCall(_))
}
