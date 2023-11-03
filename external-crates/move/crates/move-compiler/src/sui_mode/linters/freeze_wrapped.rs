// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This analysis flags freezing instances of structs containing (transitively or not) other structs
//! with the key ability. In other words flags freezing of structs whose fields (directly or not)
//! wrap objects.

use std::collections::BTreeMap;

use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    expansion::ast as E,
    naming::ast as N,
    parser::ast::{self as P, Ability_},
    shared::{program_info::TypingProgramInfo, CompilationEnv, Identifier},
    typing::{
        ast as T,
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;

use super::{
    base_type, LinterDiagCategory, FREEZE_FUN, LINTER_DEFAULT_DIAG_CODE, LINT_WARNING_PREFIX,
    PUBLIC_FREEZE_FUN, SUI_PKG_NAME, TRANSFER_MOD_NAME,
};

const FREEZE_WRAPPING_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagCategory::FreezeWrapped as u8,
    LINTER_DEFAULT_DIAG_CODE,
    "attempting to freeze wrapped objects",
);

const FREEZE_FUNCTIONS: &[(&str, &str, &str)] = &[
    (SUI_PKG_NAME, TRANSFER_MOD_NAME, PUBLIC_FREEZE_FUN),
    (SUI_PKG_NAME, TRANSFER_MOD_NAME, FREEZE_FUN),
];

/// Information about a field that wraps other objects.
#[derive(Debug, Clone, Copy)]
struct WrappingFieldInfo {
    /// Name of the field
    fname: Symbol,
    /// Location of the field type
    ftype_loc: Loc,
    /// Location of the type of the wrapped object.
    wrapped_type_loc: Loc,
    /// Is the wrapping direct or indirect
    direct: bool,
}

impl WrappingFieldInfo {
    fn new(fname: Symbol, ftype_loc: Loc, wrapped_type_loc: Loc, direct: bool) -> Self {
        Self {
            fname,
            ftype_loc,
            wrapped_type_loc,
            direct,
        }
    }
}

/// Structs (per-module) that have fields wrapping other objects.
type WrappingFields =
    BTreeMap<E::ModuleIdent, BTreeMap<P::DatatypeName, Option<WrappingFieldInfo>>>;

pub struct FreezeWrappedVisitor;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
    program_info: &'a TypingProgramInfo,
    /// Memoizes information about struct fields wrapping other objects as they are discovered
    wrapping_fields: WrappingFields,
}

impl TypingVisitorConstructor for FreezeWrappedVisitor {
    type Context<'a> = Context<'a>;

    fn context<'a>(
        env: &'a mut CompilationEnv,
        program_info: &'a TypingProgramInfo,
        _program: &T::Program_,
    ) -> Self::Context<'a> {
        Context {
            env,
            program_info,
            wrapping_fields: WrappingFields::new(),
        }
    }
}

impl<'a> TypingVisitorContext for Context<'a> {
    fn visit_module_custom(
        &mut self,
        _ident: E::ModuleIdent,
        mdef: &mut T::ModuleDefinition,
    ) -> bool {
        // skips if true
        mdef.attributes.is_test_or_test_only()
    }

    fn visit_function_custom(
        &mut self,
        _module: E::ModuleIdent,
        _function_name: P::FunctionName,
        fdef: &mut T::Function,
    ) -> bool {
        // skips if true
        fdef.attributes.is_test_or_test_only()
    }

    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        use T::UnannotatedExp_ as E;
        if let E::ModuleCall(fun) = &exp.exp.value {
            if FREEZE_FUNCTIONS.iter().any(|(addr, module, fname)| {
                fun.module.value.is(*addr, *module) && &fun.name.value().as_str() == fname
            }) {
                let Some(bt) = base_type(&fun.type_arguments[0]) else {
                    // not an (potentially dereferenced) N::Type_::Apply nor N::Type_::Param
                    return false;
                };
                let N::Type_::Apply(_, tname, _) = &bt.value else {
                    // not a struct type
                    return false;
                };
                let N::TypeName_::ModuleType(mident, sname) = tname.value else {
                    // struct with a given name not found
                    return false;
                };
                if let Some(wrapping_field_info) = self.find_wrapping_field_loc(mident, sname) {
                    add_diag(
                        self.env,
                        fun.arguments.exp.loc,
                        sname.value(),
                        wrapping_field_info,
                    );
                }
            }
        }
        // always return false to process arguments of the call
        false
    }

    fn add_warning_filter_scope(&mut self, filter: WarningFilters) {
        self.env.add_warning_filter_scope(filter)
    }

    fn pop_warning_filter_scope(&mut self) {
        self.env.pop_warning_filter_scope()
    }
}

impl<'a> Context<'a> {
    /// Checks if a given field (identified by ftype and fname) wraps other objects and, if so,
    /// returns its location and information on whether wrapping is direct or indirect.
    fn wraps_object(
        &mut self,
        sp!(_, ftype_): &N::Type,
    ) -> Option<(Loc, /* direct wrapping */ bool)> {
        use N::Type_ as T;
        match ftype_ {
            T::Param(p) => {
                if p.abilities.has_ability_(P::Ability_::Key) {
                    Some((p.user_specified_name.loc, true))
                } else {
                    None
                }
            }
            T::Apply(Some(abilities), sp!(_, N::TypeName_::ModuleType(mident, sname)), _) => {
                if abilities.has_ability_(Ability_::Key) {
                    let sloc = self.program_info.struct_declared_loc(mident, sname);
                    Some((sloc, true))
                } else {
                    self.find_wrapping_field_loc(*mident, *sname)
                        .as_ref()
                        .map(|info| (info.wrapped_type_loc, false))
                }
            }
            T::Apply(None, _, _) => unreachable!("ICE type expansion should have occurred"),
            T::Apply(_, _, _)
            | T::Unit
            | T::Ref(_, _)
            | T::Var(_)
            | T::Anything
            | T::UnresolvedError => None,
        }
    }

    /// Find if a field (if any) of a given struct identified by mident and sname that is wrapping
    /// other objects, and return its location. In case this function is called recursively (we also
    /// track recursion depth) to find "inner" fields wrapping objects, the "outer" field
    /// information is included as well.
    fn find_wrapping_field_loc(
        &mut self,
        mident: E::ModuleIdent,
        sname: P::DatatypeName,
    ) -> Option<WrappingFieldInfo> {
        let memoized_info = self
            .wrapping_fields
            .get(&mident)
            .and_then(|m| m.get(&sname));
        if memoized_info.is_none() {
            let info = self.find_wrapping_field_loc_impl(mident, sname);
            self.wrapping_fields
                .entry(mident)
                .or_default()
                .insert(sname, info);
        }
        *self
            .wrapping_fields
            .get(&mident)
            .and_then(|m| m.get(&sname))
            .unwrap()
    }

    fn find_wrapping_field_loc_impl(
        &mut self,
        mident: E::ModuleIdent,
        sname: P::DatatypeName,
    ) -> Option<WrappingFieldInfo> {
        let sdef = self.program_info.struct_definition(&mident, &sname);
        let N::StructFields::Defined(sfields) = &sdef.fields else {
            return None;
        };
        sfields.iter().find_map(|(_, fname, (_, ftype))| {
            let res = self.wraps_object(ftype);
            res.map(|(wrapped_tloc, direct)| {
                WrappingFieldInfo::new(*fname, ftype.loc, wrapped_tloc, direct)
            })
        })
    }
}

fn add_diag(
    env: &mut CompilationEnv,
    freeze_arg_loc: Loc,
    frozen_struct_name: Symbol,
    info: WrappingFieldInfo,
) {
    let WrappingFieldInfo {
        fname: frozen_field_name,
        ftype_loc: frozen_field_tloc,
        wrapped_type_loc: wrapped_tloc,
        direct,
    } = info;
    let msg = format!(
        "Freezing an object of type '{frozen_struct_name}' also \
         freezes all objects wrapped in its field '{frozen_field_name}'."
    );
    let uid_msg = format!(
        "The field of this type {} a wrapped object",
        if !direct { "indirectly contains" } else { "is" }
    );
    let mut d = diag!(
        FREEZE_WRAPPING_DIAG,
        (freeze_arg_loc, msg),
        (frozen_field_tloc, uid_msg)
    );

    if !direct {
        d.add_secondary_label((wrapped_tloc, "Indirectly wrapped object is of this type"));
    }
    env.add_diag(d);
}
