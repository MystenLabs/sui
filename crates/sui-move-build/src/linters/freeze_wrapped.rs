// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This analysis flags freezing instances of structs containing, transitively or not, other structs
//! with the key ability.

use std::collections::BTreeMap;

use move_compiler::{
    diag,
    diagnostics::codes::{custom, DiagnosticInfo, Severity},
    expansion::ast as E,
    naming::ast as N,
    parser::ast as P,
    shared::{unique_map::UniqueMap, CompilationEnv, Identifier},
    typing::{ast as T, core::ProgramInfo, visitor::TypingVisitor},
};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;

use super::{
    LinterDiagCategory, FREEZE_FUN, LINTER_DEFAULT_DIAG_CODE, LINT_WARNING_PREFIX,
    PUBLIC_FREEZE_FUN, SUI_PKG_NAME, TRANSFER_MOD_NAME,
};

const FREEZE_KEY_DIAG: DiagnosticInfo = custom(
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

#[derive(Debug, Clone)]
struct KeyFieldInfo {
    fname: Symbol,
    /// Type of the field that directly or indirectly wraps an object.
    ftype_loc: Loc,
    /// Location of the type of the wrapped object.
    wrapped_type_loc: Loc,
    /// Is the wrapping direct or indirect
    direct: bool,
}

/// Structs (per-module) that have fields with the key ability
type KeyFields = BTreeMap<E::ModuleIdent, BTreeMap<P::StructName, Option<KeyFieldInfo>>>;

#[derive(Default)]
pub struct FreezeWrappedVisitor {
    /// Memoizes information about struct fields with key ability as they are discovered
    key_fields: KeyFields,
}

impl TypingVisitor for FreezeWrappedVisitor {
    fn visit(
        &mut self,
        env: &mut CompilationEnv,
        _program_info: &ProgramInfo,
        program: &mut T::Program,
    ) {
        for (_, _, mdef) in program.modules.iter() {
            env.add_warning_filter_scope(mdef.warning_filter.clone());

            for (_, _, fdef) in mdef.functions.iter() {
                env.add_warning_filter_scope(fdef.warning_filter.clone());

                if let T::FunctionBody_::Defined(seq) = &fdef.body.value {
                    self.visit_seq(seq, env, &program.modules);
                }

                env.pop_warning_filter_scope();
            }

            env.pop_warning_filter_scope();
        }
    }
}

impl FreezeWrappedVisitor {
    fn visit_seq_item(
        &mut self,
        sp!(_, seq_item): &T::SequenceItem,
        env: &mut CompilationEnv,
        modules: &UniqueMap<E::ModuleIdent, T::ModuleDefinition>,
    ) {
        use T::SequenceItem_ as SI;
        match seq_item {
            SI::Seq(e) => self.visit_exp(e, env, modules),
            SI::Declare(_) => (),
            SI::Bind(_, _, e) => self.visit_exp(e, env, modules),
        }
    }

    fn visit_exp(
        &mut self,
        exp: &T::Exp,
        env: &mut CompilationEnv,
        modules: &UniqueMap<E::ModuleIdent, T::ModuleDefinition>,
    ) {
        use T::UnannotatedExp_ as E;
        let sp!(_, uexp) = &exp.exp;
        match uexp {
            E::ModuleCall(fun) => {
                if FREEZE_FUNCTIONS.iter().any(|(addr, module, fname)| {
                    fun.module.value.is(*addr, *module) && &fun.name.value().as_str() == fname
                }) {
                    let Some(bt) = base_type(&fun.type_arguments[0]) else {
                        // not an (potentially dereferenced) N::Type_::Apply nor N::Type_::Param
                        return;
                    };
                    let N::Type_::Apply(_,tname, _) = &bt.value else {
                        // not a struct type
                        return;
                    };
                    let N::TypeName_::ModuleType(mident, sname) = tname.value else {
                        // struct with a given name not found
                        return;
                    };
                    // see if info about fields of a give struct was already found
                    let (key_field_info, info_inserted) = self.get_key_field(&mident, &sname);
                    if let Some(info) = key_field_info {
                        add_diag(
                            env,
                            fun.arguments.exp.loc,
                            sname.value(),
                            info.fname,
                            info.ftype_loc,
                            info.wrapped_type_loc,
                            info.direct,
                        );
                    }
                    if info_inserted {
                        // did not find fields in the past - makes no sense to keep looking
                        return;
                    }

                    let Some((sfields, _)) = self.struct_fields(&sname, &mident, modules) else {
                        // fields for a given struct could not be located
                        return;
                    };

                    let info_found = sfields.iter().any(|(_, fname, (_, ftype))| {
                        let res = self.contains_key(ftype, modules, /* field_depth */ 0);
                        if let Some((wrapped_tloc, direct)) = res {
                            // field containing wrapped objects found
                            self.insert_key_field(
                                mident,
                                sname,
                                *fname,
                                ftype.loc,
                                wrapped_tloc,
                                direct,
                            );
                            add_diag(
                                env,
                                fun.arguments.exp.loc,
                                sname.value(),
                                *fname,
                                ftype.loc,
                                wrapped_tloc,
                                direct,
                            );
                            return true;
                        }
                        false
                    });
                    if !info_found {
                        // no field containing wrapped objects was found in a given struct
                        self.insert_no_key_field(mident, sname);
                    }
                }
            }
            E::Builtin(_, e) => self.visit_exp(e, env, modules),
            E::Vector(_, _, _, e) => self.visit_exp(e, env, modules),
            E::IfElse(e1, e2, e3) => {
                self.visit_exp(e1, env, modules);
                self.visit_exp(e2, env, modules);
                self.visit_exp(e3, env, modules);
            }
            E::While(e1, e2) => {
                self.visit_exp(e1, env, modules);
                self.visit_exp(e2, env, modules);
            }
            E::Loop { has_break: _, body } => self.visit_exp(body, env, modules),
            E::Block(seq) => self.visit_seq(seq, env, modules),
            E::Assign(_, _, e) => self.visit_exp(e, env, modules),
            E::Mutate(e1, e2) => {
                self.visit_exp(e1, env, modules);
                self.visit_exp(e2, env, modules);
            }
            E::Return(e) => self.visit_exp(e, env, modules),
            E::Abort(e) => self.visit_exp(e, env, modules),
            E::Dereference(e) => self.visit_exp(e, env, modules),
            E::UnaryExp(_, e) => self.visit_exp(e, env, modules),
            E::BinopExp(e1, _, _, e2) => {
                self.visit_exp(e1, env, modules);
                self.visit_exp(e2, env, modules);
            }
            E::Pack(_, _, _, fields) => fields
                .iter()
                .for_each(|(_, _, (_, (_, e)))| self.visit_exp(e, env, modules)),
            E::ExpList(list) => {
                for l in list {
                    match l {
                        T::ExpListItem::Single(e, _) => self.visit_exp(e, env, modules),
                        T::ExpListItem::Splat(_, e, _) => self.visit_exp(e, env, modules),
                    }
                }
            }
            E::Borrow(_, e, _) => self.visit_exp(e, env, modules),
            E::TempBorrow(_, e) => self.visit_exp(e, env, modules),
            E::Cast(e, _) => self.visit_exp(e, env, modules),
            E::Annotate(e, _) => self.visit_exp(e, env, modules),
            E::Unit { .. }
            | E::Value(_)
            | E::Move { .. }
            | E::Copy { .. }
            | E::Use(_)
            | E::Constant(..)
            | E::Break
            | E::Continue
            | E::BorrowLocal(..)
            | E::Spec(..)
            | E::UnresolvedError => (),
        }
    }

    fn visit_seq(
        &mut self,
        seq: &T::Sequence,
        env: &mut CompilationEnv,
        modules: &UniqueMap<E::ModuleIdent, T::ModuleDefinition>,
    ) {
        for s in seq {
            self.visit_seq_item(s, env, modules);
        }
    }

    fn struct_fields<'a>(
        &mut self,
        sname: &P::StructName,
        mident: &E::ModuleIdent,
        modules: &'a UniqueMap<E::ModuleIdent, T::ModuleDefinition>,
    ) -> Option<(&'a E::Fields<N::Type>, Loc)> {
        if let Some(mdef) = modules.get(mident) {
            if let Some(sdef) = mdef.structs.get(sname) {
                if let N::StructFields::Defined(sfields) = &sdef.fields {
                    // unwrap is safe since we know that mdef.structs.get succeeded
                    return Some((sfields, *mdef.structs.get_loc(sname).unwrap()));
                }
            }
        }
        None
    }

    fn contains_key(
        &mut self,
        t: &N::Type,
        modules: &UniqueMap<E::ModuleIdent, T::ModuleDefinition>,
        field_depth: usize,
    ) -> Option<(Loc, /* direct wrapping */ bool)> {
        use N::Type_ as T;
        let Some(bt) = base_type(t) else{
        return None;
    };
        let sp!(_, bt) = bt;
        match bt {
            T::Param(p) => {
                if p.abilities.has_ability_(P::Ability_::Key) {
                    return Some((p.user_specified_name.loc, field_depth == 0));
                }
                None
            }
            T::Apply(abilities, tname, _) => {
                if let N::TypeName_::ModuleType(mident, sname) = tname.value {
                    // don't have to check all variants of H::TypeName_ as only H::TypeName_::ModuleType
                    // can be a struct or have abilities
                    let (key_field_info, info_inserted) = self.get_key_field(&mident, &sname);
                    if let Some(info) = key_field_info {
                        return Some((info.wrapped_type_loc, field_depth == 0));
                    }
                    if info_inserted {
                        // did not find fields in the past - makes no sense to keep looking
                        return None;
                    }

                    if let Some((sfields, sloc)) = self.struct_fields(&sname, &mident, modules) {
                        // we could take out the ability set check out of the if condition but it should
                        // not matter as only struct can have abilities defined on them and having it
                        // here allows us to return the location of the struct type (rather than the
                        // location of struct name)
                        if let Some(ability_set) = abilities {
                            if ability_set.has_ability_(P::Ability_::Key) {
                                return Some((sloc, field_depth == 0));
                            }
                        }
                        let info = sfields.iter().find_map(|(_, fname, (_, ftype))| {
                            let res = self.contains_key(ftype, modules, field_depth + 1);
                            if let Some((wrapped_tloc, direct)) = res {
                                self.insert_key_field(
                                    mident,
                                    sname,
                                    *fname,
                                    ftype.loc,
                                    wrapped_tloc,
                                    direct,
                                );
                            }
                            res
                        });
                        if info.is_none() {
                            // no field containing wrapped objects was found in a given struct
                            self.insert_no_key_field(mident, sname);
                        }
                        return info;
                    }
                }
                None
            }
            T::Unit | T::Ref(_, _) | T::Var(_) | T::Anything | T::UnresolvedError => None,
        }
    }

    /// Inserts information about a field with key ability to KeyFields.
    fn insert_key_field(
        &mut self,
        mident: E::ModuleIdent,
        sname: P::StructName,
        fname: Symbol,
        ftype_loc: Loc,
        wrapped_type_loc: Loc,
        direct: bool,
    ) {
        self.key_fields
            .entry(mident)
            .or_insert_with(BTreeMap::new)
            .insert(
                sname,
                Some(KeyFieldInfo {
                    fname,
                    ftype_loc,
                    wrapped_type_loc,
                    direct,
                }),
            );
    }

    /// Inserts information about lack of fields with key ability in a give struct to KeyFields.
    fn insert_no_key_field(&mut self, mident: E::ModuleIdent, sname: P::StructName) {
        self.key_fields
            .entry(mident)
            .or_insert_with(BTreeMap::new)
            .insert(sname, None);
    }

    /// Returns information about whether a field of a given struct has a key ability:
    /// - (Some(KeyFieldInfo), true) if info was inserted and there is such a field
    /// - (None, true)               if info was inserted and there is no such a field
    /// - (None, false)              if info was not inserted previously
    fn get_key_field(
        &self,
        mident: &E::ModuleIdent,
        sname: &P::StructName,
    ) -> (Option<KeyFieldInfo>, bool) {
        let mut info_inserted = false;
        let Some(structs) = self.key_fields.get(mident) else {
            return (None, info_inserted);
        };
        let Some(key_field_info) = structs.get(sname) else {
            return (None, info_inserted);
        };
        info_inserted = true;
        (key_field_info.clone(), info_inserted)
    }
}

fn add_diag(
    env: &mut CompilationEnv,
    freeze_arg_loc: Loc,
    frozen_struct_name: Symbol,
    frozen_field_name: Symbol,
    frozen_field_tloc: Loc,
    wrapped_tloc: Loc,
    direct: bool,
) {
    let msg = format!(
        "Freezing an object of type '{frozen_struct_name}' also \
         freezes all objects wrapped in its field '{frozen_field_name}'."
    );
    let uid_msg = format!(
        "The field of this type {} a wrapped object",
        if !direct { "indirectly contains" } else { "is" }
    );
    let mut d = diag!(
        FREEZE_KEY_DIAG,
        (freeze_arg_loc, msg),
        (frozen_field_tloc, uid_msg)
    );

    if !direct {
        d.add_secondary_label((wrapped_tloc, "Indirectly wrapped object is of this type"));
    }
    env.add_diag(d);
}

fn base_type(t: &N::Type) -> Option<&N::Type> {
    use N::Type_ as T;
    match &t.value {
        T::Ref(_, inner_t) => base_type(inner_t),
        T::Apply(_, _, _) | T::Param(_) => Some(t),
        T::Unit | T::Var(_) | T::Anything | T::UnresolvedError => None,
    }
}
