// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    compiler_info::CompilerInfo,
    symbols::{
        add_member_use_def, expansion_mod_ident_to_map_key, find_datatype, type_def_loc, DefInfo,
        DefMap, LocalDef, MemberDefInfo, ModuleDefs, References, UseDef, UseDefMap,
    },
    utils::{ignored_function, loc_start_to_lsp_position_opt},
};

use move_compiler::{
    diagnostics::warning_filters::WarningFilters,
    expansion::ast::{self as E, ModuleIdent},
    naming::ast as N,
    parser::ast::{self as P, ConstantName},
    shared::{files::MappedFiles, ide::MacroCallInfo, Identifier, Name},
    typing::{
        ast as T,
        visitor::{LValueKind, TypingVisitorContext},
    },
};
use move_ir_types::location::{sp, Loc};
use move_symbol_pool::Symbol;

use im::OrdMap;
use lsp_types::Position;
use std::collections::BTreeMap;

/// Data used during anlysis over typed AST
pub struct TypingAnalysisContext<'a> {
    /// Outermost definitions in a module (structs, consts, functions), keyd on a ModuleIdent
    /// string so that we can access it regardless of the ModuleIdent representation
    /// (e.g., in the parsing AST or in the typing AST)
    pub mod_outer_defs: &'a mut BTreeMap<String, ModuleDefs>,
    /// Mapped file information for translating locations into positions
    pub files: &'a MappedFiles,
    /// Associates uses for a given definition to allow displaying all references
    pub references: &'a mut References,
    /// Additional information about definitions
    pub def_info: &'a mut DefMap,
    /// A UseDefMap for a given module (needs to be appropriately set before the module
    /// processing starts)
    pub use_defs: UseDefMap,
    /// Current module identifier string (needs to be appropriately set before the module
    /// processing starts)
    pub current_mod_ident_str: Option<String>,
    /// Alias lengths in access paths for a given module (needs to be appropriately
    /// set before the module processing starts)
    pub alias_lengths: &'a BTreeMap<Position, usize>,
    /// In some cases (e.g., when processing bodies of macros) we want to keep traversing
    /// the AST but without recording the actual metadata (uses, definitions, types, etc.)
    pub traverse_only: bool,
    /// Contains type params where relevant (e.g. when processing module members)
    pub type_params: BTreeMap<Symbol, Loc>,
    /// Associates uses for a given definition to allow displaying all references
    /// Current expression scope, for use when traversing expressions and recording usage.
    pub expression_scope: OrdMap<Symbol, LocalDef>,
    /// IDE Annotation Information from the Compiler
    pub compiler_info: &'a mut CompilerInfo,
}

fn def_info_to_type_def_loc(
    mod_outer_defs: &BTreeMap<String, ModuleDefs>,
    def_info: &DefInfo,
) -> Option<Loc> {
    match def_info {
        DefInfo::Type(t) => type_def_loc(mod_outer_defs, t),
        DefInfo::Function(..) => None,
        DefInfo::Struct(mod_ident, name, ..) => {
            let mod_ident_str = expansion_mod_ident_to_map_key(mod_ident);
            mod_outer_defs
                .get(&mod_ident_str)
                .and_then(|mod_defs| find_datatype(mod_defs, name))
        }
        DefInfo::Enum(mod_ident, name, ..) => {
            let mod_ident_str = expansion_mod_ident_to_map_key(mod_ident);
            mod_outer_defs
                .get(&mod_ident_str)
                .and_then(|mod_defs| find_datatype(mod_defs, name))
        }
        DefInfo::Variant(..) => None,
        DefInfo::Field(.., t, _) => type_def_loc(mod_outer_defs, t),
        DefInfo::Local(_, t, _, _, _) => type_def_loc(mod_outer_defs, t),
        DefInfo::Const(_, _, t, _, _) => type_def_loc(mod_outer_defs, t),
        DefInfo::Module(..) => None,
    }
}

impl TypingAnalysisContext<'_> {
    /// Returns the `lsp_types::Position` start for a location, but may fail if we didn't see the
    /// definition already.
    fn file_start_position_opt(&self, loc: &Loc) -> Option<Position> {
        self.files.file_start_position_opt(loc).map(|p| p.into())
    }

    /// Returns the `lsp_types::Position` start for a location, but may fail if we didn't see the
    /// definition already. This should only be used on things we already indexed.
    fn file_start_position(&self, loc: &Loc) -> Position {
        loc_start_to_lsp_position_opt(self.files, loc).unwrap()
    }

    fn reset_for_module_member(&mut self) {
        self.type_params = BTreeMap::new();
        self.expression_scope = OrdMap::new();
    }

    /// Add type parameter to a scope holding type params
    fn add_type_param(&mut self, tp: &N::TParam) {
        if self.traverse_only {
            return;
        }
        let start = self.file_start_position(&tp.user_specified_name.loc);
        let tname = tp.user_specified_name.value;
        let fhash = tp.user_specified_name.loc.file_hash();
        // enter self-definition for type param
        let type_def_info =
            DefInfo::Type(sp(tp.user_specified_name.loc, N::Type_::Param(tp.clone())));
        let ident_type_def_loc = def_info_to_type_def_loc(self.mod_outer_defs, &type_def_info);

        self.use_defs.insert(
            start.line,
            UseDef::new(
                self.references,
                self.alias_lengths,
                fhash,
                start,
                tp.user_specified_name.loc,
                &tname,
                ident_type_def_loc,
            ),
        );
        self.def_info
            .insert(tp.user_specified_name.loc, type_def_info);
        let exists = self.type_params.insert(tname, tp.user_specified_name.loc);
        debug_assert!(exists.is_none());
    }

    /// Add use of a const identifier
    fn add_const_use_def(&mut self, module_ident: &E::ModuleIdent_, name: &ConstantName) {
        if self.traverse_only {
            return;
        }
        let use_pos = name.loc();
        let use_name = name.value();
        let mod_ident_str = expansion_mod_ident_to_map_key(module_ident);
        let Some(mod_defs) = self.mod_outer_defs.get(&mod_ident_str) else {
            return;
        };
        // insert use of the const's module
        let mod_name = module_ident.module;
        if let Some(mod_name_start) = self.file_start_position_opt(&mod_name.loc()) {
            // a module will not be present if a constant belongs to an implicit module
            self.use_defs.insert(
                mod_name_start.line,
                UseDef::new(
                    self.references,
                    self.alias_lengths,
                    mod_name.loc().file_hash(),
                    mod_name_start,
                    mod_defs.name_loc,
                    &mod_name.value(),
                    None,
                ),
            );
        }

        let Some(name_start) = self.file_start_position_opt(&use_pos) else {
            debug_assert!(false);
            return;
        };
        if let Some(const_def) = mod_defs.constants.get(&use_name) {
            let const_info = self.def_info.get(&const_def.name_loc).unwrap();
            let ident_type_def_loc = def_info_to_type_def_loc(self.mod_outer_defs, const_info);
            self.use_defs.insert(
                name_start.line,
                UseDef::new(
                    self.references,
                    self.alias_lengths,
                    use_pos.file_hash(),
                    name_start,
                    const_def.name_loc,
                    &use_name,
                    ident_type_def_loc,
                ),
            );
        }
    }

    /// Add a defintion of a local (including function params).
    fn add_local_def(
        &mut self,
        loc: &Loc,
        name: &Symbol,
        def_type: N::Type,
        with_let: bool,
        mutable: bool,
        guard_loc: Option<Loc>,
    ) {
        if self.traverse_only {
            return;
        }
        let Some(name_start) = self.file_start_position_opt(loc) else {
            debug_assert!(false);
            return;
        };
        self.expression_scope.insert(
            *name,
            LocalDef {
                def_loc: *loc,
                def_type: def_type.clone(),
            },
        );
        // in other languages only one definition is allowed per scope but in move an (and
        // in rust) a variable can be re-defined in the same scope replacing the previous
        // definition

        // enter self-definition for def name
        let ident_type_def_loc = type_def_loc(self.mod_outer_defs, &def_type);
        self.use_defs.insert(
            name_start.line,
            UseDef::new(
                self.references,
                self.alias_lengths,
                loc.file_hash(),
                name_start,
                *loc,
                name,
                ident_type_def_loc,
            ),
        );
        self.def_info.insert(
            *loc,
            DefInfo::Local(*name, def_type, with_let, mutable, guard_loc),
        );
    }

    /// Add a use for and identifier whose definition is expected to be local to a function, and
    /// pair it with an appropriate definition
    fn add_local_use_def(&mut self, use_name: &Symbol, use_pos: &Loc) {
        if self.traverse_only {
            return;
        }
        let Some(name_start) = self.file_start_position_opt(use_pos) else {
            debug_assert!(false);
            return;
        };
        if let Some(local_def) = self.expression_scope.get(use_name) {
            let ident_type_def_loc = type_def_loc(self.mod_outer_defs, &local_def.def_type);
            self.use_defs.insert(
                name_start.line,
                UseDef::new(
                    self.references,
                    self.alias_lengths,
                    use_pos.file_hash(),
                    name_start,
                    local_def.def_loc,
                    use_name,
                    ident_type_def_loc,
                ),
            );
        }
    }

    /// Add use of a function identifier
    fn add_fun_use_def(
        &mut self,
        module_ident: &E::ModuleIdent,
        fun_def_name: &Symbol, // may be different from use_name for methods
        use_name: &Symbol,
        use_pos: &Loc,
    ) -> Option<UseDef> {
        if self.traverse_only {
            return None;
        }
        let mod_name = module_ident.value.module;
        let mod_name_start_opt = self.file_start_position_opt(&mod_name.loc());

        let Some(mod_defs) = self
            .mod_outer_defs
            .get_mut(&expansion_mod_ident_to_map_key(&module_ident.value))
        else {
            // this should not happen but due to a fix in unifying generation of mod ident map keys,
            // but just in case - it's better to report it than to crash the analyzer due to
            // unchecked unwrap
            eprintln!(
                "WARNING: could not locate module {:?} when processing a call to {}{}",
                module_ident.value, module_ident.value, fun_def_name
            );
            return None;
        };
        // insert use of the functions's module
        if let Some(mod_name_start) = mod_name_start_opt {
            // a module will not be present if a function belongs to an implicit module
            self.use_defs.insert(
                mod_name_start.line,
                UseDef::new(
                    self.references,
                    self.alias_lengths,
                    mod_name.loc().file_hash(),
                    mod_name_start,
                    mod_defs.name_loc,
                    &mod_name.value(),
                    None,
                ),
            );
        }

        let mut use_defs = std::mem::replace(&mut self.use_defs, UseDefMap::new());
        let mut refs = std::mem::take(self.references);
        let result = add_member_use_def(
            fun_def_name,
            self.files,
            mod_defs,
            use_name,
            use_pos,
            &mut refs,
            self.def_info,
            &mut use_defs,
            self.alias_lengths,
        );
        let _ = std::mem::replace(&mut self.use_defs, use_defs);
        let _ = std::mem::replace(self.references, refs);

        if result.is_none() {
            debug_assert!(false);
        }
        result
    }

    /// Add use of a datatype identifier
    fn add_datatype_use_def(&mut self, mident: &ModuleIdent, use_name: &P::DatatypeName) {
        if self.traverse_only {
            return;
        }
        let mod_ident_str = expansion_mod_ident_to_map_key(&mident.value);
        let Some(mod_defs) = self.mod_outer_defs.get(&mod_ident_str) else {
            return;
        };
        // insert use of the struct's module
        let mod_name = mident.value.module;
        if let Some(mod_name_start) = self.file_start_position_opt(&mod_name.loc()) {
            // a module will not be present if a struct belongs to an implicit module
            self.use_defs.insert(
                mod_name_start.line,
                UseDef::new(
                    self.references,
                    self.alias_lengths,
                    mod_name.loc().file_hash(),
                    mod_name_start,
                    mod_defs.name_loc,
                    &mod_name.value(),
                    None,
                ),
            );
        }

        let mut use_defs = std::mem::replace(&mut self.use_defs, UseDefMap::new());
        let mut refs = std::mem::take(self.references);
        add_member_use_def(
            &use_name.value(),
            self.files,
            mod_defs,
            &use_name.value(),
            &use_name.loc(),
            &mut refs,
            self.def_info,
            &mut use_defs,
            self.alias_lengths,
        );
        let _ = std::mem::replace(&mut self.use_defs, use_defs);
        let _ = std::mem::replace(self.references, refs);
    }

    /// Add a type for a struct field given its type
    fn add_struct_field_type_use_def(
        &mut self,
        field_type: &N::Type,
        use_name: &Symbol,
        use_pos: &Loc,
    ) {
        let sp!(_, typ) = field_type;
        match typ {
            N::Type_::Ref(_, t) => self.add_struct_field_type_use_def(t, use_name, use_pos),
            N::Type_::Apply(
                _,
                sp!(_, N::TypeName_::ModuleType(sp!(_, mod_ident), struct_name)),
                _,
            ) => {
                self.add_field_use_def(
                    mod_ident,
                    &struct_name.value(),
                    None,
                    use_name,
                    use_pos,
                    /* named_only */ false,
                );
            }
            _ => (),
        }
    }

    fn add_variant_use_def(
        &mut self,
        module_ident: &E::ModuleIdent,
        enum_name: &P::DatatypeName,
        variant_name: &P::VariantName,
    ) {
        let use_name = variant_name.value();
        let use_loc = variant_name.loc();
        if self.traverse_only {
            return;
        }
        let mod_ident_str = expansion_mod_ident_to_map_key(&module_ident.value);
        let Some(name_start) = self.file_start_position_opt(&use_loc) else {
            debug_assert!(false);
            return;
        };
        // get module info
        let Some(mod_defs) = self.mod_outer_defs.get(&mod_ident_str) else {
            return;
        };
        // get the enum
        let Some(def) = mod_defs.enums.get(&enum_name.value()) else {
            return;
        };
        // get variants info
        let MemberDefInfo::Enum { variants_info } = &def.info else {
            return;
        };
        // get variant's fields
        let Some((vloc, _, _)) = variants_info.get(&use_name) else {
            return;
        };

        self.use_defs.insert(
            name_start.line,
            UseDef::new(
                self.references,
                self.alias_lengths,
                use_loc.file_hash(),
                name_start,
                *vloc,
                &use_name,
                None,
            ),
        );
    }

    /// Add use of a variant field identifier. In some cases, such as packing (controlled by `named_only`), we only
    /// want to add named fields.
    fn add_field_use_def(
        &mut self,
        module_ident: &E::ModuleIdent_,
        datatype_name: &Symbol,
        variant_name_opt: Option<&Symbol>,
        use_name: &Symbol,
        use_loc: &Loc,
        named_only: bool,
    ) {
        if self.traverse_only {
            return;
        }
        let mod_ident_str = expansion_mod_ident_to_map_key(module_ident);
        let Some(name_start) = self.file_start_position_opt(use_loc) else {
            debug_assert!(false);
            return;
        };
        // get module info
        let Some(mod_defs) = self.mod_outer_defs.get(&mod_ident_str) else {
            return;
        };

        let (field_defs, positional) = if let Some(variant_name) = variant_name_opt {
            // get the enum
            let Some(def) = mod_defs.enums.get(datatype_name) else {
                return;
            };

            // get variants info
            let MemberDefInfo::Enum { variants_info } = &def.info else {
                return;
            };
            // get variant's fields
            let Some((_, field_defs, positional)) = variants_info.get(variant_name) else {
                return;
            };
            (field_defs, positional)
        } else {
            // get the struct
            let Some(def) = mod_defs.structs.get(datatype_name) else {
                return;
            };
            // get variant's fields
            let MemberDefInfo::Struct {
                field_defs,
                positional,
            } = &def.info
            else {
                return;
            };
            (field_defs, positional)
        };

        if *positional && named_only {
            return;
        }

        for fdef in field_defs {
            if fdef.name == *use_name {
                let field_info = self.def_info.get(&fdef.loc).unwrap();
                let ident_type_def_loc = def_info_to_type_def_loc(self.mod_outer_defs, field_info);
                self.use_defs.insert(
                    name_start.line,
                    UseDef::new(
                        self.references,
                        self.alias_lengths,
                        use_loc.file_hash(),
                        name_start,
                        fdef.loc,
                        use_name,
                        ident_type_def_loc,
                    ),
                );
            }
        }
    }

    fn process_module_call(
        &mut self,
        module: &ModuleIdent,
        name: &P::FunctionName,
        method_name: Option<Name>,
        tyargs: &[N::Type],
        args: Option<&T::Exp>,
    ) {
        let fun_name = name.value();
        // a function name (same as fun_name) or method name (different from fun_name)
        let fun_use = method_name.unwrap_or_else(|| sp(name.loc(), name.value()));
        let call_use_def = self.add_fun_use_def(module, &fun_name, &fun_use.value, &fun_use.loc);
        // handle type parameters
        for t in tyargs.iter() {
            self.visit_type(None, t);
        }
        if let Some(args) = args {
            self.visit_exp(args);
        }

        // populate call info obtained during parsing with the location of the call site's target function
        let Some(use_def) = call_use_def else {
            return;
        };
        assert!(self.current_mod_ident_str.is_some());
        let Some(callsite_mod_defs) = self
            .mod_outer_defs
            .get_mut(&self.current_mod_ident_str.clone().unwrap())
        else {
            return;
        };
        if let Some(info) = callsite_mod_defs.call_infos.get_mut(&fun_use.loc) {
            info.def_loc = Some(use_def.def_loc());
        }
    }

    fn process_match_patterm(&mut self, match_pat: &T::MatchPattern) {
        use T::UnannotatedPat_ as UA;

        self.visit_type(None, &match_pat.ty);
        match &match_pat.pat.value {
            UA::Variant(mident, name, vname, tyargs, fields)
            | UA::BorrowVariant(_, mident, name, vname, tyargs, fields) => {
                self.add_datatype_use_def(mident, name);
                self.add_variant_use_def(mident, name, vname);
                tyargs.iter().for_each(|t| self.visit_type(None, t));
                for (fpos, fname, (_, (_, pat))) in fields.iter() {
                    if self.compiler_info.ellipsis_binders.get(&fpos).is_none() {
                        self.add_field_use_def(
                            &mident.value,
                            &name.value(),
                            Some(&vname.value()),
                            fname,
                            &fpos,
                            /* named_only */ false,
                        );
                    }
                    self.process_match_patterm(pat);
                }
            }
            UA::Struct(mident, name, tyargs, fields)
            | UA::BorrowStruct(_, mident, name, tyargs, fields) => {
                self.add_datatype_use_def(mident, name);
                tyargs.iter().for_each(|t| self.visit_type(None, t));
                for (fpos, fname, (_, (_, pat))) in fields.iter() {
                    if self.compiler_info.ellipsis_binders.get(&fpos).is_none() {
                        self.add_field_use_def(
                            &mident.value,
                            &name.value(),
                            None,
                            fname,
                            &fpos,
                            /* named_only */ false,
                        );
                    }
                    self.process_match_patterm(pat);
                }
            }
            UA::Constant(mod_ident, name) => self.add_const_use_def(&mod_ident.value, name),
            UA::Or(pat1, pat2) => {
                self.process_match_patterm(pat1);
                self.process_match_patterm(pat2);
            }
            // variable definition in `At` is added when `T::MatchArm_.binders`
            // is processed in `process_match_arm`
            UA::At(_, pat) => self.process_match_patterm(pat),
            // variable definition in `Binder`` is added when `T::MatchArm_.binders`
            // is processed in `process_match_arm`
            UA::Binder(_, _) => (),
            UA::Literal(_) | UA::Wildcard | UA::ErrorPat => (),
        }
    }

    fn process_match_arm(&mut self, sp!(_, arm): &T::MatchArm) {
        self.process_match_patterm(&arm.pattern);
        let guard_loc = arm.guard.as_ref().map(|exp| exp.exp.loc);
        arm.binders.iter().for_each(|(var, ty)| {
            self.add_local_def(
                &var.loc,
                &var.value.name,
                ty.clone(),
                false,
                false,
                guard_loc,
            );
        });

        if let Some(exp) = &arm.guard {
            self.visit_exp(exp);
            // Enum guard variables have different type (immutable reference) than variables in
            // patterns and in the RHS of the match arm. However, at the AST level they share
            // the same definition, stored in the IDE in a map key-ed on the definition's location.
            // In order to display (on hover) two different types for these variables, we do
            // the following:
            // - remember which `DefInfo::LocalDef`s represent match arm definition (above) and what
            //   is the position of their arm's guard
            // - remember which region represents a guard expression (below)
            // - when processing on-hover, we see if for a given use the definition is a
            //   match arm definition and if this use is inside a correct guard block; if both these
            //   conditions hold, we change the displayed info for this variable to reflect
            //   it being an immutable reference
            let guard_loc = exp.exp.loc;
            self.compiler_info
                .guards
                .entry(guard_loc.file_hash())
                .or_default()
                .insert(guard_loc);
        }
        self.visit_exp(&arm.rhs);
    }
}

impl<'a> TypingVisitorContext for TypingAnalysisContext<'a> {
    // Nothing to do -- we're not producing errors.
    fn push_warning_filter_scope(&mut self, _filter: WarningFilters) {}

    // Nothing to do -- we're not producing errors.
    fn pop_warning_filter_scope(&mut self) {}

    // We do want to visit types.
    const VISIT_TYPES: bool = true;

    // We do want to visit lvalues.
    const VISIT_LVALUES: bool = true;

    // We do want to visit lvalues.
    const VISIT_USE_FUNS: bool = true;

    fn visit_struct(
        &mut self,
        module: move_compiler::expansion::ast::ModuleIdent,
        struct_name: P::DatatypeName,
        sdef: &N::StructDefinition,
    ) {
        self.reset_for_module_member();
        assert!(self.current_mod_ident_str.is_none());
        self.current_mod_ident_str = Some(expansion_mod_ident_to_map_key(&module.value));
        let file_hash = struct_name.loc().file_hash();
        // enter self-definition for struct name (unwrap safe - done when inserting def)
        let name_start = self.file_start_position(&struct_name.loc());
        let struct_info = self.def_info.get(&struct_name.loc()).unwrap();
        let struct_type_def = def_info_to_type_def_loc(self.mod_outer_defs, struct_info);
        self.use_defs.insert(
            name_start.line,
            UseDef::new(
                self.references,
                self.alias_lengths,
                file_hash,
                name_start,
                struct_name.loc(),
                &struct_name.value(),
                struct_type_def,
            ),
        );
        for stp in &sdef.type_parameters {
            self.add_type_param(&stp.param);
        }
        if let N::StructFields::Defined(positional, fields) = &sdef.fields {
            for (fpos, fname, (_, (_, ty))) in fields {
                self.visit_type(None, ty);
                if !*positional {
                    // Enter self-definition for field name (unwrap safe - done when inserting def),
                    // but only if the fields are named. Positional fields, introduced in Move 2024
                    // version of the language, have "fake" locations and could make the displayed
                    // results confusing. The reason for "fake" locations is that a struct has one
                    // internal representation in the compiler for both structs with named and
                    // positional fields (and the latter's fields don't have the actual names).
                    let start = self.file_start_position(&fpos);
                    let field_info = DefInfo::Type(ty.clone());
                    let ident_type_def_loc =
                        def_info_to_type_def_loc(self.mod_outer_defs, &field_info);
                    self.use_defs.insert(
                        start.line,
                        UseDef::new(
                            self.references,
                            self.alias_lengths,
                            fpos.file_hash(),
                            start,
                            fpos,
                            fname,
                            ident_type_def_loc,
                        ),
                    );
                }
            }
        }
        self.current_mod_ident_str = None;
    }

    fn visit_enum_custom(
        &mut self,
        module: move_compiler::expansion::ast::ModuleIdent,
        enum_name: P::DatatypeName,
        edef: &N::EnumDefinition,
    ) -> bool {
        self.reset_for_module_member();
        assert!(self.current_mod_ident_str.is_none());
        self.current_mod_ident_str = Some(expansion_mod_ident_to_map_key(&module.value));
        let file_hash = enum_name.loc().file_hash();
        // enter self-definition for enum name (unwrap safe - done when inserting def)
        let name_start = self.file_start_position(&enum_name.loc());
        let enum_info = self.def_info.get(&enum_name.loc()).unwrap();
        let enum_type_def = def_info_to_type_def_loc(self.mod_outer_defs, enum_info);
        self.use_defs.insert(
            name_start.line,
            UseDef::new(
                self.references,
                self.alias_lengths,
                file_hash,
                name_start,
                enum_name.loc(),
                &enum_name.value(),
                enum_type_def,
            ),
        );
        for etp in &edef.type_parameters {
            self.add_type_param(&etp.param);
        }

        for (vname, vdef) in edef.variants.key_cloned_iter() {
            self.visit_variant(&module, &enum_name, vname, vdef);
        }
        self.current_mod_ident_str = None;
        true
    }

    fn visit_variant_custom(
        &mut self,
        _module: &move_compiler::expansion::ast::ModuleIdent,
        _enum_name: &P::DatatypeName,
        variant_name: P::VariantName,
        vdef: &N::VariantDefinition,
    ) -> bool {
        let file_hash = variant_name.loc().file_hash();
        // enter self-definition for variant name (unwrap safe - done when inserting def)
        let vname_start = self.file_start_position(&variant_name.loc());
        let variant_info = self.def_info.get(&variant_name.loc()).unwrap();
        let vtype_def = def_info_to_type_def_loc(self.mod_outer_defs, variant_info);
        self.use_defs.insert(
            vname_start.line,
            UseDef::new(
                self.references,
                self.alias_lengths,
                file_hash,
                vname_start,
                variant_name.loc(),
                &variant_name.value(),
                vtype_def,
            ),
        );
        if let N::VariantFields::Defined(positional, fields) = &vdef.fields {
            for (floc, fname, (_, (_, ty))) in fields {
                self.visit_type(None, ty);
                if !*positional {
                    // enter self-definition for field name (unwrap safe - done when inserting def),
                    // but only if the fields are named (same as for structs - see comment there
                    // for more detailed explanation)
                    let start = self.file_start_position(&floc);
                    let field_info = DefInfo::Type(ty.clone());
                    let ident_type_def_loc =
                        def_info_to_type_def_loc(self.mod_outer_defs, &field_info);
                    self.use_defs.insert(
                        start.line,
                        UseDef::new(
                            self.references,
                            self.alias_lengths,
                            floc.file_hash(),
                            start,
                            floc,
                            fname,
                            ident_type_def_loc,
                        ),
                    );
                }
            }
        }

        true
    }

    fn visit_constant(
        &mut self,
        module: move_compiler::expansion::ast::ModuleIdent,
        constant_name: P::ConstantName,
        cdef: &T::Constant,
    ) {
        self.reset_for_module_member();
        assert!(self.current_mod_ident_str.is_none());
        self.current_mod_ident_str = Some(expansion_mod_ident_to_map_key(&module.value));
        let loc = constant_name.loc();
        // enter self-definition for const name (unwrap safe - done when inserting def)
        let name_start = self.file_start_position(&loc);
        let const_info = self.def_info.get(&loc).unwrap();
        let ident_type_def_loc = def_info_to_type_def_loc(self.mod_outer_defs, const_info);
        self.use_defs.insert(
            name_start.line,
            UseDef::new(
                self.references,
                self.alias_lengths,
                loc.file_hash(),
                name_start,
                loc,
                &constant_name.value(),
                ident_type_def_loc,
            ),
        );
        self.visit_exp(&cdef.value);
        self.current_mod_ident_str = None;
    }

    fn visit_function(
        &mut self,
        module: move_compiler::expansion::ast::ModuleIdent,
        function_name: P::FunctionName,
        fdef: &T::Function,
    ) {
        self.reset_for_module_member();
        if ignored_function(function_name.value()) {
            return;
        }
        assert!(self.current_mod_ident_str.is_none());
        self.current_mod_ident_str = Some(expansion_mod_ident_to_map_key(&module.value));
        let loc = function_name.loc();
        // first, enter self-definition for function name (unwrap safe - done when inserting def)
        let name_start = self.file_start_position(&loc);
        let fun_info = self.def_info.get(&loc).unwrap();
        let fun_type_def = def_info_to_type_def_loc(self.mod_outer_defs, fun_info);
        let use_def = UseDef::new(
            self.references,
            self.alias_lengths,
            loc.file_hash(),
            name_start,
            loc,
            &function_name.value(),
            fun_type_def,
        );

        self.use_defs.insert(name_start.line, use_def);

        // Get symbols for a function definition
        for tp in &fdef.signature.type_parameters {
            self.add_type_param(tp);
        }

        for (mutability, pname, ptype) in &fdef.signature.parameters {
            self.visit_type(None, ptype);

            // add definition of the parameter
            self.add_local_def(
                &pname.loc,
                &pname.value.name,
                ptype.clone(),
                false, /* with_let */
                matches!(mutability, E::Mutability::Mut(_)),
                None,
            );
        }

        match &fdef.body.value {
            T::FunctionBody_::Defined(seq) => {
                self.visit_seq(fdef.body.loc, seq);
            }
            T::FunctionBody_::Macro | T::FunctionBody_::Native => (),
        }
        // process return types
        self.visit_type(None, &fdef.signature.return_type);

        // clear type params from the scope
        self.type_params.clear();
        self.current_mod_ident_str = None;
    }

    fn visit_lvalue(&mut self, kind: &LValueKind, lvalue: &T::LValue) {
        // Visit an lvalue. If it's just avariable, we add it as a local def. If it'x a field
        // access, we switch modes (`for_unpack` = true) and record them as field use defs instead.
        let mut for_unpack = false;
        let mut lvalue_queue = vec![lvalue];
        while let Some(next) = lvalue_queue.pop() {
            match &next.value {
                T::LValue_::Ignore => (),
                T::LValue_::Var {
                    mut_,
                    var,
                    ty,
                    unused_binding: _,
                } => {
                    match kind {
                        LValueKind::Bind => {
                            self.add_local_def(
                                &var.loc,
                                &var.value.name,
                                *ty.clone(),
                                !for_unpack, // (only for simple definition, e.g., `let t = 1;``)
                                mut_.is_some_and(|m| matches!(m, E::Mutability::Mut(_))),
                                None,
                            );
                        }
                        LValueKind::Assign => self.add_local_use_def(&var.value.name, &var.loc),
                    }
                }
                T::LValue_::Unpack(mident, name, tyargs, fields)
                | T::LValue_::BorrowUnpack(_, mident, name, tyargs, fields) => {
                    for_unpack = true;
                    self.add_datatype_use_def(mident, name);
                    tyargs.iter().for_each(|t| self.visit_type(None, t));
                    for (fpos, fname, (_, (_, lvalue))) in fields {
                        self.add_field_use_def(
                            &mident.value,
                            &name.value(),
                            None,
                            fname,
                            &fpos,
                            /* named_only */ false,
                        );
                        lvalue_queue.push(lvalue);
                    }
                }
                T::LValue_::UnpackVariant(mident, name, vname, tyargs, fields)
                | T::LValue_::BorrowUnpackVariant(_, mident, name, vname, tyargs, fields) => {
                    for_unpack = true;
                    self.add_datatype_use_def(mident, name);
                    tyargs.iter().for_each(|t| self.visit_type(None, t));
                    for (fpos, fname, (_, (_, lvalue))) in fields {
                        self.add_field_use_def(
                            &mident.value,
                            &name.value(),
                            Some(&vname.value()),
                            fname,
                            &fpos,
                            /* named_only */ false,
                        );
                        lvalue_queue.push(lvalue);
                    }
                }
            }
        }
    }

    fn visit_seq(&mut self, _loc: Loc, (use_funs, seq): &T::Sequence) {
        let old_traverse_mode = self.traverse_only;
        // start adding new use-defs etc. when processing arguments
        if use_funs.color == 0 {
            self.traverse_only = false;
        }
        self.visit_use_funs(use_funs);
        let new_scope = self.expression_scope.clone();
        let previous_scope = std::mem::replace(&mut self.expression_scope, new_scope);
        // a block is a new var scope
        for seq_item in seq {
            self.visit_seq_item(seq_item);
        }
        let _inner_scope = std::mem::replace(&mut self.expression_scope, previous_scope);
        if use_funs.color == 0 {
            self.traverse_only = old_traverse_mode;
        }
    }

    fn visit_exp_custom(&mut self, exp: &T::Exp) -> bool {
        use T::UnannotatedExp_ as TE;

        fn visit_exp_inner(visitor: &mut TypingAnalysisContext<'_>, exp: &T::Exp) -> bool {
            let exp_loc = exp.exp.loc;
            visitor.visit_type(Some(exp_loc), &exp.ty);
            match &exp.exp.value {
                TE::Move { from_user: _, var }
                | TE::Copy { from_user: _, var }
                | TE::Use(var)
                | TE::BorrowLocal(_, var) => {
                    visitor.add_local_use_def(&var.value.name, &var.loc);
                    true
                }
                TE::Constant(mod_ident, name) => {
                    visitor.add_const_use_def(&mod_ident.value, name);
                    true
                }
                TE::ModuleCall(mod_call) => {
                    let call = &**mod_call;
                    visitor.process_module_call(
                        &call.module,
                        &call.name,
                        call.method_name,
                        &call.type_arguments,
                        Some(&call.arguments),
                    );
                    true
                }
                TE::Pack(mident, name, tyargs, fields) => {
                    visitor.add_datatype_use_def(mident, name);
                    for (fpos, fname, (_, (_, init_exp))) in fields.iter() {
                        visitor.add_field_use_def(
                            &mident.value,
                            &name.value(),
                            None,
                            fname,
                            &fpos,
                            /* named_only */ true,
                        );
                        visitor.visit_exp(init_exp);
                    }
                    tyargs
                        .iter()
                        .for_each(|t| visitor.visit_type(Some(exp_loc), t));
                    true
                }
                TE::Borrow(_, exp, field) => {
                    visitor.visit_exp(exp);
                    visitor.add_struct_field_type_use_def(&exp.ty, &field.value(), &field.loc());
                    true
                }
                TE::PackVariant(mident, name, vname, tyargs, fields) => {
                    visitor.add_datatype_use_def(mident, name);
                    visitor.add_variant_use_def(mident, name, vname);
                    for (fpos, fname, (_, (_, init_exp))) in fields.iter() {
                        visitor.add_field_use_def(
                            &mident.value,
                            &name.value(),
                            Some(&vname.value()),
                            fname,
                            &fpos,
                            /* named_only */ true,
                        );
                        visitor.visit_exp(init_exp);
                    }
                    tyargs
                        .iter()
                        .for_each(|t| visitor.visit_type(Some(exp_loc), t));
                    true
                }
                TE::VariantMatch(..) => {
                    // These should not be available before match compilation.
                    debug_assert!(false);
                    true
                }
                TE::Match(exp, sp!(_, v)) => {
                    visitor.visit_exp(exp);
                    v.iter().for_each(|arm| visitor.process_match_arm(arm));
                    true
                }
                TE::ErrorConstant {
                    line_number_loc: _,
                    error_constant,
                } => {
                    // assume that constant is defined in the same module where it's used
                    // TODO: if above ever changes, we need to update this (presumably
                    // `ErrorConstant` will carry module ident at this point)
                    if let Some(name) = error_constant {
                        if let Some(mod_def) = visitor
                            .mod_outer_defs
                            .get(visitor.current_mod_ident_str.as_ref().unwrap())
                        {
                            visitor.add_const_use_def(&mod_def.ident.clone(), name);
                        }
                    };
                    true
                }
                TE::Unit { .. }
                | TE::Builtin(_, _)
                | TE::Vector(_, _, _, _)
                | TE::IfElse(_, _, _)
                | TE::While(_, _, _)
                | TE::Loop { .. }
                | TE::NamedBlock(_, _)
                | TE::Block(_)
                | TE::Assign(_, _, _)
                | TE::Mutate(_, _)
                | TE::Return(_)
                | TE::Abort(_)
                | TE::Give(_, _)
                | TE::Continue(_)
                | TE::Dereference(_)
                | TE::UnaryExp(_, _)
                | TE::BinopExp(_, _, _, _)
                | TE::ExpList(_)
                | TE::Value(_)
                | TE::TempBorrow(_, _)
                | TE::Cast(_, _)
                | TE::Annotate(_, _)
                | TE::UnresolvedError => false,
            }
        }

        let expanded_lambda = self.compiler_info.is_expanded_lambda(&exp.exp.loc);
        if let Some(macro_call_info) = self.compiler_info.get_macro_info(&exp.exp.loc) {
            debug_assert!(!expanded_lambda, "Compiler info issue");
            let MacroCallInfo {
                module,
                name,
                method_name,
                type_arguments,
                by_value_args,
            } = macro_call_info.clone();
            self.process_module_call(&module, &name, method_name, &type_arguments, None);
            by_value_args.iter().for_each(|a| self.visit_seq_item(a));
            let old_traverse_mode = self.traverse_only;
            // stop adding new use-defs etc.
            self.traverse_only = true;
            let result = visit_exp_inner(self, exp);
            self.traverse_only = old_traverse_mode;
            result
        } else if expanded_lambda {
            let old_traverse_mode = self.traverse_only;
            // start adding new use-defs etc. when processing a lambda argument
            self.traverse_only = false;
            let result = visit_exp_inner(self, exp);
            self.traverse_only = old_traverse_mode;
            result
        } else {
            visit_exp_inner(self, exp)
        }
    }

    fn visit_type_custom(&mut self, exp_loc: Option<Loc>, ty: &N::Type) -> bool {
        if self.traverse_only {
            return true;
        }
        let loc = ty.loc;
        match &ty.value {
            N::Type_::Param(tparam) => {
                let sp!(use_pos, use_name) = tparam.user_specified_name;
                let Some(name_start) = self.file_start_position_opt(&loc) else {
                    debug_assert!(false); // a type param should not be missing
                    return true;
                };
                let Some(def_loc) = self.type_params.get(&use_name) else {
                    debug_assert!(
                        false,
                        "Could not find type for {use_name:#?} in {:#?}",
                        self.type_params
                    );
                    return true;
                };
                let ident_type_def_loc = type_def_loc(self.mod_outer_defs, ty);
                self.use_defs.insert(
                    name_start.line,
                    UseDef::new(
                        self.references,
                        self.alias_lengths,
                        use_pos.file_hash(),
                        name_start,
                        *def_loc,
                        &use_name,
                        ident_type_def_loc,
                    ),
                );
                true
            }
            N::Type_::Apply(_, sp!(_, type_name), tyargs) => {
                if let N::TypeName_::ModuleType(mod_ident, struct_name) = type_name {
                    self.add_datatype_use_def(mod_ident, struct_name);
                } // otherwise nothing to be done for other type names
                for t in tyargs.iter() {
                    self.visit_type(exp_loc, t);
                }
                true
            }
            // All of these cases just need to recur, which the visitor will handle for us.
            N::Type_::Unit
            | N::Type_::Ref(_, _)
            | N::Type_::Fun(_, _)
            | N::Type_::Var(_)
            | N::Type_::Anything
            | N::Type_::UnresolvedError => false,
        }
    }

    fn visit_use_funs(&mut self, use_funs: &N::UseFuns) {
        let N::UseFuns {
            resolved,
            implicit_candidates,
            color: _,
        } = use_funs;

        // at typing there should be no unresolved candidates (it's also checked in typing
        // translaction pass)
        assert!(implicit_candidates.is_empty());

        for uses in resolved.values() {
            for (use_loc, use_name, u) in uses {
                if let N::TypeName_::ModuleType(mod_ident, struct_name) = u.tname.value {
                    self.add_datatype_use_def(&mod_ident, &struct_name);
                } // otherwise nothing to be done for other type names
                let (module_ident, fun_def) = u.target_function;
                let fun_def_name = fun_def.value();
                let fun_def_loc = fun_def.loc();
                self.add_fun_use_def(&module_ident, &fun_def_name, use_name, &use_loc);
                self.add_fun_use_def(&module_ident, &fun_def_name, &fun_def_name, &fun_def_loc);
            }
        }
    }
}
