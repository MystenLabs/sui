// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This analysis flags uses of random::Random and random::RandomGenerator in public functions.

use crate::{
    diag,
    diagnostics::codes::{custom, DiagnosticInfo, Severity},
    expansion::ast::Visibility,
    naming::ast as N,
    shared::{program_info::TypingProgramInfo, CompilationEnv},
    typing::{ast as T, visitor::TypingVisitor},
};
use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;

use super::{
    LinterDiagCategory, LINTER_DEFAULT_DIAG_CODE, LINT_WARNING_PREFIX,
    RANDOM_GENERATOR_STRUCT_NAME, RANDOM_MOD_NAME, RANDOM_STRUCT_NAME, SUI_PKG_NAME,
};

const RANDOM_OBJECTS_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagCategory::RandomObjects as u8,
    LINTER_DEFAULT_DIAG_CODE,
    "Risky use of random::Random or random::RandomGenerator in a public function",
);

pub struct RandomObjectsVisitor;

impl TypingVisitor for RandomObjectsVisitor {
    fn visit(
        &mut self,
        env: &mut CompilationEnv,
        _program_info: &TypingProgramInfo,
        program: &mut T::Program_,
    ) {
        for (_, _, mdef) in program.modules.iter() {
            if mdef.attributes.is_test_or_test_only() {
                continue;
            }
            env.add_warning_filter_scope(mdef.warning_filter.clone());
            mdef.functions
                .iter()
                .filter(|(_, _, fdef)| {
                    !fdef.attributes.is_test_or_test_only()
                        && matches!(fdef.visibility, Visibility::Public(_))
                })
                .for_each(|(sloc, fname, fdef)| func_def(env, *fname, fdef, sloc));
            env.pop_warning_filter_scope();
        }
    }
}

fn func_def(env: &mut CompilationEnv, fname: Symbol, fdef: &T::Function, sloc: Loc) {
    env.add_warning_filter_scope(fdef.warning_filter.clone());
    for (_, _, t) in &fdef.signature.parameters {
        if is_random_or_random_generator(t) {
            let msg = format!("Public function '{fname}' accepts sui::random::Random or sui::random::RandomGenerator as a parameter.");
            let uid_msg = "Functions that accept sui::random::Random or sui::random::RandomGenerator as a parameter might be abused by attackers. Private functions are preferred.";
            let d = diag!(RANDOM_OBJECTS_DIAG, (sloc, msg), (sloc, uid_msg)); // TODO: fix
            env.add_diag(d);
        }
    }
    env.pop_warning_filter_scope();
}

fn is_random_or_random_generator(sp!(_, t): &N::Type) -> bool {
    use N::Type_ as T;

    match t {
        T::Ref(_, inner_t) => is_random_or_random_generator(inner_t),
        T::Apply(_, tname, _) => {
            let sp!(_, tname) = tname;
            tname.is(SUI_PKG_NAME, RANDOM_MOD_NAME, RANDOM_STRUCT_NAME)
                | tname.is(SUI_PKG_NAME, RANDOM_MOD_NAME, RANDOM_GENERATOR_STRUCT_NAME)
        }
        T::Unit | T::Param(_) | T::Var(_) | T::Anything | T::UnresolvedError | T::Fun(_, _) => {
            false
        }
    }
}
