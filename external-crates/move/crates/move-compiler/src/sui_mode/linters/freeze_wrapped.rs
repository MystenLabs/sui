// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This analysis flags freezing instances of structs containing (transitively or not) other structs
//! with the key ability. In other words flags freezing of structs whose fields (directly or not)
//! wrap objects.

use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        warning_filters::WarningFilters,
        Diagnostic, DiagnosticReporter, Diagnostics,
    },
    expansion::ast as E,
    naming::ast as N,
    parser::ast::{self as P, Ability_},
    shared::{program_info::TypingProgramInfo, CompilationEnv, Identifier},
    sui_mode::{
        linters::{
            LinterDiagnosticCategory, LinterDiagnosticCode, FREEZE_FUN, LINT_WARNING_PREFIX,
            PUBLIC_FREEZE_FUN, TRANSFER_MOD_NAME,
        },
        SUI_ADDR_VALUE,
    },
    typing::{
        ast as T,
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_core_types::account_address::AccountAddress;
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use std::{collections::BTreeMap, sync::Arc};

const FREEZE_WRAPPING_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Sui as u8,
    LinterDiagnosticCode::FreezeWrapped as u8,
    "attempting to freeze wrapped objects",
);

const FREEZE_FUNCTIONS: &[(AccountAddress, &str, &str)] = &[
    (SUI_ADDR_VALUE, TRANSFER_MOD_NAME, PUBLIC_FREEZE_FUN),
    (SUI_ADDR_VALUE, TRANSFER_MOD_NAME, FREEZE_FUN),
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
    #[allow(unused)]
    env: &'a CompilationEnv,
    reporter: DiagnosticReporter<'a>,
    program_info: Arc<TypingProgramInfo>,
    /// Memoizes information about struct fields wrapping other objects as they are discovered
    wrapping_fields: WrappingFields,
}

impl TypingVisitorConstructor for FreezeWrappedVisitor {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a CompilationEnv, program: &T::Program) -> Self::Context<'a> {
        let reporter = env.diagnostic_reporter_at_top_level();
        Context {
            env,
            reporter,
            program_info: program.info.clone(),
            wrapping_fields: WrappingFields::new(),
        }
    }
}

impl Context<'_> {
    fn add_diag(&self, diag: Diagnostic) {
        self.reporter.add_diag(diag);
    }

    #[allow(unused)]
    fn add_diags(&self, diags: Diagnostics) {
        self.reporter.add_diags(diags);
    }
}

impl<'a> TypingVisitorContext for Context<'a> {
    fn visit_module_custom(&mut self, _ident: E::ModuleIdent, mdef: &T::ModuleDefinition) -> bool {
        // skips if true
        mdef.attributes.is_test_or_test_only()
    }

    fn visit_function_custom(
        &mut self,
        _module: E::ModuleIdent,
        _function_name: P::FunctionName,
        fdef: &T::Function,
    ) -> bool {
        // skips if true
        fdef.attributes.is_test_or_test_only()
    }

    fn visit_exp_custom(&mut self, exp: &T::Exp) -> bool {
        use T::UnannotatedExp_ as E;
        if let E::ModuleCall(fun) = &exp.exp.value {
            if FREEZE_FUNCTIONS.iter().any(|(addr, module, fname)| {
                fun.module.value.is(addr, *module) && &fun.name.value().as_str() == fname
            }) {
                let Some(sp!(_, N::TypeName_::ModuleType(mident, sname))) =
                    fun.type_arguments[0].value.type_name()
                else {
                    // struct with a given name not found
                    return false;
                };
                if let Some(wrapping_field_info) = self.find_wrapping_field_loc(mident, sname) {
                    add_diag(
                        self,
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

    fn push_warning_filter_scope(&mut self, filters: WarningFilters) {
        self.reporter.push_warning_filter_scope(filters)
    }

    fn pop_warning_filter_scope(&mut self) {
        self.reporter.pop_warning_filter_scope()
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
                    self.find_wrapping_field_loc(mident, sname)
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
            | T::UnresolvedError
            | T::Fun(_, _) => None,
        }
    }

    /// Find if a field (if any) of a given struct identified by mident and sname that is wrapping
    /// other objects, and return its location. In case this function is called recursively (we also
    /// track recursion depth) to find "inner" fields wrapping objects, the "outer" field
    /// information is included as well.
    fn find_wrapping_field_loc(
        &mut self,
        mident: &E::ModuleIdent,
        sname: &P::DatatypeName,
    ) -> Option<WrappingFieldInfo> {
        let memoized_info = self.wrapping_fields.get(mident).and_then(|m| m.get(sname));
        if memoized_info.is_none() {
            let info = self.find_wrapping_field_loc_impl(mident, sname);
            self.wrapping_fields
                .entry(*mident)
                .or_default()
                .insert(*sname, info);
        }
        *self
            .wrapping_fields
            .get(mident)
            .and_then(|m| m.get(sname))
            .unwrap()
    }

    fn find_wrapping_field_loc_impl(
        &mut self,
        mident: &E::ModuleIdent,
        sname: &P::DatatypeName,
    ) -> Option<WrappingFieldInfo> {
        let info = self.program_info.clone();
        let sdef = info.struct_definition(mident, sname);
        let N::StructFields::Defined(_, sfields) = &sdef.fields else {
            return None;
        };
        sfields.iter().find_map(|(_, fname, (_, (_, ftype)))| {
            let res = self.wraps_object(ftype);
            res.map(|(wrapped_tloc, direct)| {
                WrappingFieldInfo::new(*fname, ftype.loc, wrapped_tloc, direct)
            })
        })
    }
}

fn add_diag(
    context: &mut Context,
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
    context.add_diag(d);
}
