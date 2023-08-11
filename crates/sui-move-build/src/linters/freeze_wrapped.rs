// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This analysis flags freezing instances of structs containing (transitively or not) other structs
//! with the key ability. In other words flags freezing of structs whose fields (directly or not)
//! wrap objects.

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

/// Information about a field.
#[derive(Debug, Clone)]
struct FieldInfo {
    /// Name of the field
    fname: Symbol,
    /// Location of the field type
    ftype_loc: Loc,
    /// Abilities of the field type
    abilities: Option<E::AbilitySet>,
}

impl FieldInfo {
    fn new(fname: Symbol, ftype_loc: Loc, abilities: Option<E::AbilitySet>) -> Self {
        Self {
            fname,
            ftype_loc,
            abilities,
        }
    }
}

/// Information about a field that wraps other objects.
#[derive(Debug, Clone)]
struct WrappingFieldInfo {
    finfo: FieldInfo,
    /// Location of the type of the wrapped object.
    wrapped_type_loc: Loc,
    /// Is the wrapping direct or indirect
    direct: bool,
}

impl WrappingFieldInfo {
    fn new(fname: Symbol, ftype_loc: Loc, wrapped_type_loc: Loc, direct: bool) -> Self {
        let finfo = FieldInfo::new(fname, ftype_loc, None);
        Self {
            finfo,
            wrapped_type_loc,
            direct,
        }
    }

    fn fname(&self) -> Symbol {
        self.finfo.fname
    }

    fn ftype_loc(&self) -> Loc {
        self.finfo.ftype_loc
    }

    fn wrapped_type_loc(&self) -> Loc {
        self.wrapped_type_loc
    }

    fn direct(&self) -> bool {
        self.direct
    }
}

/// Structs (per-module) that have fields wrapping other objects.
type WrappingFields = BTreeMap<E::ModuleIdent, BTreeMap<P::StructName, Option<WrappingFieldInfo>>>;

#[derive(Default)]
pub struct FreezeWrappedVisitor {
    /// Memoizes information about struct fields wrapping other objects as they are discovered
    wrapping_fields: WrappingFields,
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
                    if let Some(wrapping_field_info) = self.find_wrapping_field_loc(
                        modules, mident, sname, /* outer_field_info */ None,
                        /* field_depth  */ 0,
                    ) {
                        add_diag(
                            env,
                            fun.arguments.exp.loc,
                            sname.value(),
                            wrapping_field_info.fname(),
                            wrapping_field_info.ftype_loc(),
                            wrapping_field_info.wrapped_type_loc(),
                            wrapping_field_info.direct(),
                        );
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

    /// Checks if a given field (identified by ftype and fname) wraps other objects and, if so,
    /// returns its location and information on whether wrapping is direct or indirect.
    fn wraps_object(
        &mut self,
        ftype: &N::Type,
        fname: Symbol,
        modules: &UniqueMap<E::ModuleIdent, T::ModuleDefinition>,
        field_depth: usize,
    ) -> Option<(Loc, /* direct wrapping */ bool)> {
        use N::Type_ as T;
        let Some(bt) = base_type(ftype) else{
            return None;
        };
        let sp!(_, bt) = bt;
        match bt {
            T::Param(p) => {
                if p.abilities.has_ability_(P::Ability_::Key) {
                    return Some((p.user_specified_name.loc, field_depth == 1));
                }
                None
            }
            T::Apply(abilities, tname, _) => {
                if let N::TypeName_::ModuleType(mident, sname) = tname.value {
                    // don't have to check all variants of H::TypeName_ as only H::TypeName_::ModuleType
                    // can be a struct or have abilities
                    if let Some(wrapping_field_info) = self.find_wrapping_field_loc(
                        modules,
                        mident,
                        sname,
                        Some(FieldInfo::new(fname, ftype.loc, abilities.clone())),
                        field_depth,
                    ) {
                        return Some((
                            wrapping_field_info.wrapped_type_loc,
                            wrapping_field_info.direct,
                        ));
                    }
                }
                None
            }
            T::Unit | T::Ref(_, _) | T::Var(_) | T::Anything | T::UnresolvedError => None,
        }
    }

    /// Find if a field (if any) of a given struct identified by mident and sname that is wrapping
    /// other objects, and return its location. In case this function is called recursively (we also
    /// track recursion depth) to find "inner" fields wrapping objects, the "outer" field
    /// information is included as well.
    fn find_wrapping_field_loc(
        &mut self,
        modules: &UniqueMap<E::ModuleIdent, T::ModuleDefinition>,
        mident: E::ModuleIdent,
        sname: P::StructName,
        outer_field_info: Option<FieldInfo>,
        field_depth: usize,
    ) -> Option<WrappingFieldInfo> {
        let (wrapping_field_info, info_inserted) = self.get_wrapping_field(&mident, &sname);
        if wrapping_field_info.is_some() {
            // found memoized field wrapping an object
            return wrapping_field_info;
        }
        if info_inserted {
            // did not find fields wrapping object in the past - makes no sense to keep looking
            return None;
        }
        let Some((sfields, sloc)) = self.struct_fields(&sname, &mident, modules) else {
            return None;
        };

        // In this function we may be either looking at the top level struct (to find whether it has
        // fields wrapping object) or for a nested struct representing a type of one of the outer
        // fields (to find whether it is a wrapped object or its fields have wrapped object). In the
        // latter case (that's when struct_abilities is Some) we need to check if the struct itself
        // is an object.
        if let Some(outer_info) = outer_field_info {
            if let Some(ability_set) = outer_info.abilities {
                if ability_set.has_ability_(P::Ability_::Key) {
                    return Some(WrappingFieldInfo::new(
                        outer_info.fname,
                        outer_info.ftype_loc,
                        sloc,
                        field_depth == 1,
                    ));
                }
            }
        }

        let wrapping_field_info = sfields.iter().find_map(|(_, fname, (_, ftype))| {
            let res = self.wraps_object(ftype, *fname, modules, field_depth + 1);
            if let Some((wrapped_tloc, direct)) = res {
                // a field wrapping an object found - memoize it
                return Some(self.insert_wrapping_field(
                    mident,
                    sname,
                    *fname,
                    ftype.loc,
                    wrapped_tloc,
                    direct,
                ));
            }
            None
        });

        if wrapping_field_info.is_none() {
            // no field containing wrapped objects was found in a given struct
            self.insert_no_wrapping_field(mident, sname);
        }
        wrapping_field_info
    }

    /// Memoizes information about a field wrapping other objects in WrappingFields.
    fn insert_wrapping_field(
        &mut self,
        mident: E::ModuleIdent,
        sname: P::StructName,
        fname: Symbol,
        ftype_loc: Loc,
        wrapped_type_loc: Loc,
        direct: bool,
    ) -> WrappingFieldInfo {
        let wrapping_field_info =
            WrappingFieldInfo::new(fname, ftype_loc, wrapped_type_loc, direct);
        self.wrapping_fields
            .entry(mident)
            .or_insert_with(BTreeMap::new)
            .insert(sname, Some(wrapping_field_info.clone()));
        wrapping_field_info
    }

    /// Memoizes information about lack of fields wrapping other object in a given struct in
    /// WrappingFields.
    fn insert_no_wrapping_field(&mut self, mident: E::ModuleIdent, sname: P::StructName) {
        self.wrapping_fields
            .entry(mident)
            .or_insert_with(BTreeMap::new)
            .insert(sname, None);
    }

    /// Returns information about whether there exists a memoized field of a given struct wrapping
    /// other objects:
    /// - (Some(WrappingfieldInfo), true) if info was inserted and there is such a field
    /// - (None, true)                    if info was inserted and there is no such a field
    /// - (None, false)                   if info was not inserted previously
    fn get_wrapping_field(
        &self,
        mident: &E::ModuleIdent,
        sname: &P::StructName,
    ) -> (Option<WrappingFieldInfo>, bool) {
        let mut info_inserted = false;
        let Some(structs) = self.wrapping_fields.get(mident) else {
            return (None, info_inserted);
        };
        let Some(wrapping_field_info) = structs.get(sname) else {
            return (None, info_inserted);
        };
        info_inserted = true;
        (wrapping_field_info.clone(), info_inserted)
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
        FREEZE_WRAPPING_DIAG,
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
