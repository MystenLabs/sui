// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Enforces that public functions use `&mut TxContext` instead of `&TxContext` to ensure upgradability.
//! Detects and reports instances where a non-mutable reference to `TxContext` is used in public function signatures.
//! Promotes best practices for future-proofing smart contract code by allowing mutation of the transaction context.
use super::{LinterDiagnosticCategory, LINT_WARNING_PREFIX, LinterDiagnosticCode};
use crate::expansion::ast::Mutability;
use crate::naming::ast::{Type, TypeName, Var};
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

const REQUIRE_MUTABLE_TX_CONTEXT_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Suspicious as u8,
    LinterDiagnosticCode::RequireMutableTxContext as u8,
    "Public functions should take `&mut TxContext` instead of `&TxContext` for better upgradability.",
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
    fn add_warning_filter_scope(&mut self, filter: WarningFilters) {
        self.env.add_warning_filter_scope(filter)
    }
    fn pop_warning_filter_scope(&mut self) {
        self.env.pop_warning_filter_scope()
    }

    fn visit_function_custom(
        &mut self,
        _module: ModuleIdent,
        _function_name: FunctionName,
        fdef: &mut T::Function,
    ) -> bool {
        if let Visibility::Public(_) = fdef.visibility {
            self.check_function_parameters(&fdef.signature.parameters);
        }

        false
    }
}

impl Context<'_> {
    fn check_function_parameters(&mut self, parameters: &Vec<(Mutability, Var, Type)>) {
        for param in parameters {
            if let Some(loc) = self.is_immutable_tx_context(&param.2) {
                self.report_non_mutable_tx_context(loc);
            }
        }
    }

    fn is_immutable_tx_context(&self, param_type: &Type) -> Option<Loc> {
        match &param_type.value {
            Type_::Ref(false, var_type) => self.is_tx_context_type(var_type),
            _ => None,
        }
    }

    fn is_tx_context_type(&self, var_type: &Type) -> Option<Loc> {
        match &var_type.value {
            Type_::Apply(_, type_name, _) => self.is_sui_tx_context(type_name),
            _ => None,
        }
    }

    fn is_sui_tx_context(&self, type_name: &TypeName) -> Option<Loc> {
        match &type_name.value {
            TypeName_::ModuleType(module_ident, DatatypeName(sp!(_, struct_name))) => {
                if module_ident.value.is("sui", "tx_context")
                    && struct_name.to_string() == "TxContext"
                {
                    Some(type_name.loc)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn report_non_mutable_tx_context(&mut self, loc: Loc) {
        let diag = diag!(
            REQUIRE_MUTABLE_TX_CONTEXT_DIAG,
            (loc, "Public functions should take `&mut TxContext` instead of `&TxContext` for better upgradability.")
        );
        self.env.add_diag(diag);
    }
}
