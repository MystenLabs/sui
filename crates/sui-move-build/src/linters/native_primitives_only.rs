// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This analysis flags uses of the sui::coin::Coin struct in fields of other structs. In most cases
//! it's preferable to use sui::balance::Balance instead to save space.

use move_compiler::{
    diag,
    diagnostics::codes::{custom, DiagnosticInfo, Severity},
    naming::ast as N,
    shared::{CompilationEnv, Identifier},
    typing::{ast as T, core::ProgramInfo, visitor::TypingVisitor},
};
use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;

use super::{LinterDiagCategory, LINTER_DEFAULT_DIAG_CODE, LINT_WARNING_PREFIX};

const NATIVE_PRIMITIVE_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagCategory::NativePrimitivesOnly as u8,
    LINTER_DEFAULT_DIAG_CODE,
    "passing non-native types to native functions",
);

pub struct NativePrimitivesOnlyVisitor;

impl TypingVisitor for NativePrimitivesOnlyVisitor {
    fn visit(
        &mut self,
        env: &mut CompilationEnv,
        _program_info: &ProgramInfo,
        program: &mut T::Program,
    ) {
        for (_, _, mdef) in &program.modules {
            env.add_warning_filter_scope(mdef.warning_filter.clone());
            mdef.functions
                .key_cloned_iter()
                .for_each(|(fname, fdef)| check_native(env, fname.value(), fdef, fname.loc()));
            env.pop_warning_filter_scope();
        }
    }
}

fn check_native(env: &mut CompilationEnv, fname: Symbol, fun: &T::Function, sloc: Loc) {
    match fun.body.value {
        T::FunctionBody_::Defined(_) => {}
        T::FunctionBody_::Native => {
            let parameters = &fun.signature.parameters;
            for (var, type_) in parameters.iter() {
                if !is_native_or_ref(&type_.value) {
                    //This is a non-native type, so we prepare the message
                    let c = var.value.name;
                    let msg =
                        format!("The parameter '{c}' of '{fname}' is not native or a reference");
                    let uid_msg = "- It is recommended when implementing native functions to only deal with primitives";
                    let d = diag!(NATIVE_PRIMITIVE_DIAG, (var.loc, msg), (sloc, uid_msg));
                    env.add_diag(d);

                    // println!("Object in definition, time to shoot");
                }
            }
        }
    }
}
fn is_native_or_ref(element: &N::Type_) -> bool {
    match element {
        N::Type_::Apply(_option, typename, _stype) => match typename.value {
            N::TypeName_::Builtin(_) => {
                return true;
            }
            N::TypeName_::ModuleType(_mident, _sname) => {
                //Flash out that a struct should not be here!
                return false;
            }
            N::TypeName_::Multiple(_) => {
                return false;
            }
        },
        N::Type_::Ref(_is_mutable, referenced_element) => {
            //I have to check if it's a reference to a native type or not
            let el = &referenced_element.value;
            is_native_or_ref(el)
        }
        N::Type_::Unit => {
            return false;
        }
        N::Type_::Var(_) => {
            return false;
        }
        N::Type_::Anything => {
            return false;
        }
        N::Type_::UnresolvedError => {
            return false;
        }
        N::Type_::Param(_) => {
            return false;
        }
    }
}
