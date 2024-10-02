// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Enforces that public functions use `&mut TxContext` instead of `&TxContext` to ensure upgradability.
//! Detects and reports instances where a non-mutable reference to `TxContext` is used in public function signatures.
//! Promotes best practices for future-proofing smart contract code by allowing mutation of the transaction context.
use super::{LinterDiagnosticCategory, LinterDiagnosticCode, LINT_WARNING_PREFIX};

use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    expansion::ast::{ModuleIdent, Visibility},
    naming::ast::Type_,
    parser::ast::FunctionName,
    shared::CompilationEnv,
    sui_mode::{SUI_ADDR_NAME, TX_CONTEXT_MODULE_NAME, TX_CONTEXT_TYPE_NAME},
    typing::{
        ast as T,
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;

const REQUIRE_MUTABLE_TX_CONTEXT_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Sui as u8,
    LinterDiagnosticCode::PreferMutableTxContext as u8,
    "prefer '&mut TxContext' over '&TxContext'",
);

pub struct PreferMutableTxContext;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for PreferMutableTxContext {
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

    fn visit_module_custom(&mut self, ident: ModuleIdent, _mdef: &T::ModuleDefinition) -> bool {
        // skip if in 'sui::tx_context'
        ident.value.is(SUI_ADDR_NAME, TX_CONTEXT_MODULE_NAME)
    }

    fn visit_function_custom(
        &mut self,
        _module: ModuleIdent,
        _function_name: FunctionName,
        fdef: &T::Function,
    ) -> bool {
        if !matches!(&fdef.visibility, Visibility::Public(_)) {
            return false;
        }

        for (_, _, sp!(loc, param_ty_)) in &fdef.signature.parameters {
            if matches!(
                param_ty_,
                Type_::Ref(false, t) if t.value.is(SUI_ADDR_NAME, TX_CONTEXT_MODULE_NAME, TX_CONTEXT_TYPE_NAME),
            ) {
                report_non_mutable_tx_context(self.env, *loc);
            }
        }

        false
    }
}

fn report_non_mutable_tx_context(env: &mut CompilationEnv, loc: Loc) {
    let msg = format!(
        "'public' functions should prefer '&mut {0}' over '&{0}' for better upgradability.",
        TX_CONTEXT_TYPE_NAME
    );
    let mut diag = diag!(REQUIRE_MUTABLE_TX_CONTEXT_DIAG, (loc, msg));
    diag.add_note(
        "When upgrading, the public function cannot be modified to take '&mut TxContext' instead \
         of '&TxContext'. As such, it is recommended to consider using '&mut TxContext' to \
         future-proof the function.",
    );
    env.add_diag(diag);
}
