// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Enforces that public functions use `&mut TxContext` instead of `&TxContext` to ensure upgradability.
//! Detects and reports instances where a non-mutable reference to `TxContext` is used in public function signatures.
//! Promotes best practices for future-proofing smart contract code by allowing mutation of the transaction context.
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    expansion::ast::{ModuleIdent, Visibility},
    naming::ast::{TypeName_, Type_},
    parser::ast::{DatatypeName, FunctionName},
    shared::CompilationEnv,
    typing::{
        ast as T,
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;

use super::{LinterDiagnosticCategory, LINT_WARNING_PREFIX, REQUIRE_MUTABLE_TX_CONTEXT_DIAG_CODE};

const REQUIRE_MUTABLE_TX_CONTEXT_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Suspicious as u8,
    REQUIRE_MUTABLE_TX_CONTEXT_DIAG_CODE,
    "",
);

pub struct RequireMutableTxContext;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for RequireMutableTxContext {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_function_custom(
        &mut self,
        _module: ModuleIdent,
        _function_name: FunctionName,
        fdef: &mut T::Function,
    ) -> bool {
        if let Visibility::Public(_) = fdef.visibility {
            for param in &fdef.signature.parameters {
                if let Type_::Ref(false, var_type) = &param.2.value {
                    if let Type_::Apply(_, type_name, _) = &var_type.value {
                        if let TypeName_::ModuleType(_, DatatypeName(sp!(_, struct_name))) =
                            &type_name.value
                        {
                            if struct_name.to_string() == "TxContext" {
                                report_non_mutable_tx_context(self.env, type_name.loc);
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

fn report_non_mutable_tx_context(env: &mut CompilationEnv, loc: Loc) {
    let diag = diag!(
        REQUIRE_MUTABLE_TX_CONTEXT_DIAG,
        (loc, "Public functions should take `&mut TxContext` instead of `&TxContext` for better upgradability.")
    );
    env.add_diag(diag);
}
