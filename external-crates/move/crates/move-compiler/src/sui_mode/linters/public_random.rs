// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This analysis flags uses of random::Random and random::RandomGenerator in public functions.

use crate::expansion::ast::ModuleIdent;
use crate::parser::ast::FunctionName;
use crate::sui_mode::{SUI_ADDR_NAME, SUI_ADDR_VALUE};
use crate::typing::visitor::simple_visitor;
use crate::{
    diag,
    diagnostics::codes::{custom, DiagnosticInfo, Severity},
    expansion::ast::Visibility,
    naming::ast as N,
    typing::ast as T,
};

use super::{
    LinterDiagnosticCategory, LinterDiagnosticCode, LINT_WARNING_PREFIX,
    RANDOM_GENERATOR_STRUCT_NAME, RANDOM_MOD_NAME, RANDOM_STRUCT_NAME,
};

const PUBLIC_RANDOM_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Sui as u8,
    LinterDiagnosticCode::PublicRandom as u8,
    "Risky use of 'sui::random'",
);

simple_visitor!(
    PublicRandomVisitor,
    fn visit_module_custom(&mut self, ident: ModuleIdent, mdef: &T::ModuleDefinition) -> bool {
        // skips if true
        mdef.attributes.is_test_or_test_only() || ident.value.address.is(&SUI_ADDR_VALUE)
    },
    fn visit_function_custom(
        &mut self,
        _module: ModuleIdent,
        fname: FunctionName,
        fdef: &T::Function,
    ) -> bool {
        if fdef.attributes.is_test_or_test_only()
            || !matches!(fdef.visibility, Visibility::Public(_))
        {
            return true;
        }
        for (_, _, t) in &fdef.signature.parameters {
            if let Some(struct_name) = is_random_or_random_generator(t) {
                let tloc = t.loc;
                let msg =
                    format!("'public' function '{fname}' accepts '{struct_name}' as a parameter");
                let mut d = diag!(PUBLIC_RANDOM_DIAG, (tloc, msg));
                let note = format!("Functions that accept '{}::{}::{}' as a parameter might be abused by attackers by inspecting the results of randomness",
                                   SUI_ADDR_NAME, RANDOM_MOD_NAME, struct_name);
                d.add_note(note);
                d.add_note("Non-public functions are preferred");
                self.add_diag(d);
            }
        }
        true
    }
);

fn is_random_or_random_generator(sp!(_, t): &N::Type) -> Option<&str> {
    use N::Type_ as T;
    match t {
        T::Ref(_, inner_t) => is_random_or_random_generator(inner_t),
        T::Apply(_, sp!(_, tname), _) => {
            if tname.is(&SUI_ADDR_VALUE, RANDOM_MOD_NAME, RANDOM_STRUCT_NAME) {
                Some(RANDOM_STRUCT_NAME)
            } else if tname.is(
                &SUI_ADDR_VALUE,
                RANDOM_MOD_NAME,
                RANDOM_GENERATOR_STRUCT_NAME,
            ) {
                Some(RANDOM_GENERATOR_STRUCT_NAME)
            } else {
                None
            }
        }
        T::Unit | T::Param(_) | T::Var(_) | T::Anything | T::UnresolvedError | T::Fun(_, _) => None,
    }
}
