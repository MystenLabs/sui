// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    compiler_info::CompilerInfo,
    symbols::{
        add_fun_use_def, add_struct_use_def, def_info_to_type_def_loc,
        expansion_mod_ident_to_map_key, type_def_loc, DefInfo, LocalDef, ModuleDefs,
        UseDef, UseDefMap, UseLoc, References, DefMap,
    },
    utils::{ignored_function, loc_start_to_lsp_position_opt},
};

use move_compiler::{
    diagnostics as diag,
    expansion::ast::{self as E, ModuleIdent},
    naming::ast as N,
    parser::ast as P,
    shared::{files::{self, FilePosition, MappedFiles}, ide::MacroCallInfo, Identifier, Name},
    typing::{
        ast as T,
        visitor::{LValueKind, TypingVisitorContext},
    },
};
use move_ir_types::location::{sp, Loc};
use move_symbol_pool::Symbol;

use im::OrdMap;
use lsp_types::Position;
use std::collections::{BTreeMap, BTreeSet};

/// Data used during anlysis over typed AST
pub struct TypingAnalysisContext<'a> {
    /// Outermost definitions in a module (structs, consts, functions), keyd on a ModuleIdent
    /// string so that we can access it regardless of the ModuleIdent representation
    /// (e.g., in the parsing AST or in the typing AST)
    pub mod_outer_defs: &'a BTreeMap<String, ModuleDefs>,
    /// Mapped file information for translating locations into positions
    pub files: &'a MappedFiles,
    pub references: &'a mut References,
    /// Additional information about definitions
    pub def_info: &'a mut DefMap,
    /// A UseDefMap for a given module (needs to be appropriately set before the module
    /// processing starts)
    pub use_defs: UseDefMap,
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

impl TypingAnalysisContext<'_> {
    /// Returns the `lsp_types::Position` start for a location, but may fail if we didn't see the
    /// definition already.
    fn file_start_position_opt(&self, loc: &Loc) -> Option<files::FilePosition> {
        self.files.file_start_position_opt(loc)
    }

    /// Returns the `lsp_types::Position` start for a location, but may fail if we didn't see the
    /// definition already. This should only be used on things we already indexed.
    fn file_start_position(&self, loc: &Loc) -> lsp_types::Position {
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
    fn add_const_use_def(
        &mut self,
        module_ident: &E::ModuleIdent,
        use_name: &Symbol,
        use_pos: &Loc,
    ) {
        if self.traverse_only {
            return;
        }
        let mod_ident_str = expansion_mod_ident_to_map_key(&module_ident.value);
        let Some(mod_defs) = self.mod_outer_defs.get(&mod_ident_str) else {
            return;
        };
        // insert use of the const's module
        let mod_name = module_ident.value.module;
        if let Some(mod_name_start) = self.file_start_position_opt(&mod_name.loc()) {
            // a module will not be present if a constant belongs to an implicit module
            self.use_defs.insert(
                mod_name_start.position.line_offset() as u32,
                UseDef::new(
                    self.references,
                    self.alias_lengths,
                    mod_name.loc().file_hash(),
                    mod_name_start.into(),
                    mod_defs.name_loc,
                    &mod_name.value(),
                    None,
                ),
            );
        }

        let Some(name_start) = self.file_start_position_opt(use_pos) else {
            debug_assert!(false);
            return;
        };
        if let Some(const_def) = mod_defs.constants.get(use_name) {
            let def_fhash = self.mod_outer_defs.get(&mod_ident_str).unwrap().fhash;
            let const_info = self
                .def_info
                .get(&const_def.name_loc)
                .unwrap();
            let ident_type_def_loc = def_info_to_type_def_loc(self.mod_outer_defs, const_info);
            self.use_defs.insert(
                name_start.position.line_offset() as u32,
                UseDef::new(
                    self.references,
                    self.alias_lengths,
                    use_pos.file_hash(),
                    name_start.into(),
                    const_def.name_loc,
                    use_name,
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
            name_start.position.line_offset() as u32,
            UseDef::new(
                self.references,
                self.alias_lengths,
                loc.file_hash(),
                name_start.into(),
                *loc,
                name,
                ident_type_def_loc,
            ),
        );
        self.def_info.insert(
            *loc,
            DefInfo::Local(*name, def_type, with_let, mutable),
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
                name_start.position.line_offset() as u32,
                UseDef::new(
                    self.references,
                    self.alias_lengths,
                    use_pos.file_hash(),
                    name_start.into(),
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
    ) {
        if self.traverse_only {
            return;
        }
        let mod_ident_str = expansion_mod_ident_to_map_key(&module_ident.value);
        let Some(mod_defs) = self.mod_outer_defs.get(&mod_ident_str) else {
            return;
        };
        // insert use of the functions's module
        let mod_name = module_ident.value.module;
        if let Some(mod_name_start) = self.file_start_position_opt(&mod_name.loc()) {
            // a module will not be present if a function belongs to an implicit module
            self.use_defs.insert(
                mod_name_start.position.line_offset() as u32,
                UseDef::new(
                    self.references,
                    self.alias_lengths,
                    mod_name.loc().file_hash(),
                    mod_name_start.into(),
                    mod_defs.name_loc,
                    &mod_name.value(),
                    None,
                ),
            );
        }

        let mut use_defs = std::mem::replace(&mut self.use_defs, UseDefMap::new());
        let mut refs = std::mem::take(self.references);
        let result = add_fun_use_def(
            fun_def_name,
            self.mod_outer_defs,
            self.files,
            mod_ident_str,
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
    }

    /// Add use of a datatype identifier
    fn add_datatype_use_def(
        &mut self,
        mident: &ModuleIdent,
        use_name: &P::DatatypeName,
        use_pos: &Loc,
    ) {
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
                mod_name_start.position.line_offset() as u32,
                UseDef::new(
                    self.references,
                    self.alias_lengths,
                    mod_name.loc().file_hash(),
                    mod_name_start.into(),
                    mod_defs.name_loc,
                    &mod_name.value(),
                    None,
                ),
            );
        }

        let mut use_defs = std::mem::replace(&mut self.use_defs, UseDefMap::new());
        let mut refs = std::mem::take(self.references);
        add_struct_use_def(
            self.mod_outer_defs,
            self.files,
            mod_ident_str,
            mod_defs,
            &use_name.value(),
            use_pos,
            &mut refs,
            self.def_info,
            &mut use_defs,
            self.alias_lengths,
        );
        let _ = std::mem::replace(&mut self.use_defs, use_defs);
        let _ = std::mem::replace(self.references, refs);
    }

    /// Add a type for a struct field given its type
    fn add_field_type_use_def(&mut self, field_type: &N::Type, use_name: &Symbol, use_pos: &Loc) {
        let sp!(_, typ) = field_type;
        match typ {
            N::Type_::Ref(_, t) => self.add_field_type_use_def(t, use_name, use_pos),
            N::Type_::Apply(
                _,
                sp!(_, N::TypeName_::ModuleType(sp!(_, mod_ident), struct_name)),
                _,
            ) => {
                self.add_field_use_def(mod_ident, &struct_name.value(), use_name, use_pos);
            }
            _ => (),
        }
    }

    /// Add use of a struct field identifier
    fn add_field_use_def(
        &mut self,
        module_ident: &E::ModuleIdent_,
        struct_name: &Symbol,
        use_name: &Symbol,
        use_pos: &Loc,
    ) {
        if self.traverse_only {
            return;
        }
        let mod_ident_str = expansion_mod_ident_to_map_key(module_ident);
        let Some(name_start) = self.file_start_position_opt(use_pos) else {
            debug_assert!(false);
            return;
        };
        let Some(mod_defs) = self.mod_outer_defs.get(&mod_ident_str) else {
            return;
        };
        if let Some(def) = mod_defs.structs.get(struct_name) {
            for fdef in &def.field_defs {
                if fdef.name == *use_name {
                    let def_fhash = self.mod_outer_defs.get(&mod_ident_str).unwrap().fhash;
                    let field_info = self
                        .def_info
                        .get(&fdef.loc)
                        .unwrap();
                    let ident_type_def_loc =
                        def_info_to_type_def_loc(self.mod_outer_defs, field_info);
                    self.use_defs.insert(
                        name_start.position.line_offset() as u32,
                        UseDef::new(
                            self.references,
                            self.alias_lengths,
                            use_pos.file_hash(),
                            name_start.into(),
                            fdef.loc,
                            use_name,
                            ident_type_def_loc,
                        ),
                    );
                }
            }
        }
    }

    fn process_module_call(
        &mut self,
        module: &ModuleIdent,
        name: &P::FunctionName,
        method_name: Option<Name>,
        tyargs: &mut [N::Type],
        args: Option<&mut Box<T::Exp>>,
    ) {
        let Some(mod_def) = self
            .mod_outer_defs
            .get(&expansion_mod_ident_to_map_key(&module.value))
        else {
            // this should not happen but due to a fix in unifying generation of mod ident map keys,
            // but just in case - it's better to report it than to crash the analyzer due to
            // unchecked unwrap
            eprintln!(
                "WARNING: could not locate module {:?} when processing a call to {}{}",
                module, module, name
            );
            return;
        };

        if mod_def.functions.get(&name.value()).is_none() {
            return;
        }

        let fun_name = name.value();
        // a function name (same as fun_name) or method name (different from fun_name)
        let fun_use = method_name.unwrap_or_else(|| sp(name.loc(), name.value()));
        self.add_fun_use_def(module, &fun_name, &fun_use.value, &fun_use.loc);
        // handle type parameters
        for t in tyargs.iter_mut() {
            self.visit_type(None, t);
        }
        if let Some(args) = args {
            self.visit_exp(args);
        }
    }
}

impl<'a> TypingVisitorContext for TypingAnalysisContext<'a> {
    // Nothing to do -- we're not producing errors.
    fn add_warning_filter_scope(&mut self, _filter: diag::WarningFilters) {}

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
        _module: move_compiler::expansion::ast::ModuleIdent,
        struct_name: P::DatatypeName,
        sdef: &mut N::StructDefinition,
    ) {
        self.reset_for_module_member();
        let file_hash = struct_name.loc().file_hash();
        // enter self-definition for struct name (unwrap safe - done when inserting def)
        let name_start = self.file_start_position(&struct_name.loc());
        let struct_info = self
            .def_info
            .get(&struct_name.loc())
            .unwrap();
        let struct_type_def = def_info_to_type_def_loc(self.mod_outer_defs, struct_info);
        self.use_defs.insert(
            name_start.line,
            UseDef::new(
                self.references,
                self.alias_lengths,
                file_hash,
                name_start.into(),
                struct_name.loc(),
                &struct_name.value(),
                struct_type_def,
            ),
        );
        for stp in &sdef.type_parameters {
            self.add_type_param(&stp.param);
        }
        if let N::StructFields::Defined(positional, fields) = &mut sdef.fields {
            for (fpos, fname, (_, ty)) in fields {
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
    }

    fn visit_enum_custom(
        &mut self,
        _module: move_compiler::expansion::ast::ModuleIdent,
        _enum_name: P::DatatypeName,
        _edef: &mut N::EnumDefinition,
    ) -> bool {
        self.reset_for_module_member();
        // TODO: support enums
        false
    }

    fn visit_variant_custom(
        &mut self,
        _module: &move_compiler::expansion::ast::ModuleIdent,
        _enum_name: &P::DatatypeName,
        _variant_name: P::VariantName,
        _vdef: &mut N::VariantDefinition,
    ) -> bool {
        // TODO: support enums
        false
    }

    fn visit_constant(
        &mut self,
        _module: move_compiler::expansion::ast::ModuleIdent,
        constant_name: P::ConstantName,
        cdef: &mut T::Constant,
    ) {
        self.reset_for_module_member();
        let loc = constant_name.loc();
        // enter self-definition for const name (unwrap safe - done when inserting def)
        let name_start = self.file_start_position(&loc);
        let const_info = self
            .def_info
            .get(&loc)
            .unwrap();
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
        self.visit_exp(&mut cdef.value);
    }

    fn visit_function(
        &mut self,
        _module: move_compiler::expansion::ast::ModuleIdent,
        function_name: P::FunctionName,
        fdef: &mut T::Function,
    ) {
        self.reset_for_module_member();
        if ignored_function(function_name.value()) {
            return;
        }
        let loc = function_name.loc();
        // first, enter self-definition for function name (unwrap safe - done when inserting def)
        let name_start = self.file_start_position(&loc);
        let fun_info = self
            .def_info
            .get(&loc)
            .unwrap();
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

        for (mutability, pname, ptype) in &mut fdef.signature.parameters {
            self.visit_type(None, ptype);

            // add definition of the parameter
            self.add_local_def(
                &pname.loc,
                &pname.value.name,
                ptype.clone(),
                false, /* with_let */
                matches!(mutability, E::Mutability::Mut(_)),
            );
        }

        match &mut fdef.body.value {
            T::FunctionBody_::Defined(seq) => {
                self.visit_seq(seq);
            }
            T::FunctionBody_::Macro | T::FunctionBody_::Native => (),
        }
        // process return types
        self.visit_type(None, &mut fdef.signature.return_type);

        // clear type params from the scope
        self.type_params.clear();
    }

    fn visit_lvalue(&mut self, kind: &LValueKind, lvalue: &mut T::LValue) {
        // Visit an lvalue. If it's just avariable, we add it as a local def. If it'x a field
        // access, we switch modes (`for_unpack` = true) and record them as field use defs instead.
        let mut for_unpack = false;
        let mut lvalue_queue = vec![lvalue];
        while let Some(next) = lvalue_queue.pop() {
            match &mut next.value {
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
                            );
                        }
                        LValueKind::Assign => self.add_local_use_def(&var.value.name, &var.loc),
                    }
                }
                T::LValue_::Unpack(mident, name, _tyargs, fields)
                | T::LValue_::BorrowUnpack(_, mident, name, _tyargs, fields) => {
                    for_unpack = true;
                    self.add_datatype_use_def(mident, name, &name.loc());
                    for (fpos, fname, (_, (_, lvalue))) in fields {
                        self.add_field_use_def(&mident.value, &name.value(), fname, &fpos);
                        lvalue_queue.push(lvalue);
                    }
                }
                T::LValue_::UnpackVariant(_, _, _, _, _)
                | T::LValue_::BorrowUnpackVariant(_, _, _, _, _, _) => {
                    debug_assert!(false, "Enums are not supported by move analyzser.");
                }
            }
        }
    }

    fn visit_seq(&mut self, (use_funs, seq): &mut T::Sequence) {
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

    fn visit_exp_custom(&mut self, _exp: &mut T::Exp) -> bool {
        use T::UnannotatedExp_ as TE;

        fn visit_exp_inner(visitor: &mut TypingAnalysisContext<'_>, exp: &mut T::Exp) -> bool {
            let exp_loc = exp.exp.loc;
            visitor.visit_type(Some(exp_loc), &mut exp.ty);
            match &mut exp.exp.value {
                TE::Move { from_user: _, var }
                | TE::Copy { from_user: _, var }
                | TE::Use(var)
                | TE::BorrowLocal(_, var) => {
                    visitor.add_local_use_def(&var.value.name, &var.loc);
                    true
                }
                TE::Constant(mod_ident, name) => {
                    visitor.add_const_use_def(mod_ident, &name.value(), &name.loc());
                    true
                }
                TE::ModuleCall(mod_call) => {
                    let call = &mut **mod_call;
                    visitor.process_module_call(
                        &call.module,
                        &call.name,
                        call.method_name,
                        &mut call.type_arguments,
                        Some(&mut call.arguments),
                    );
                    true
                }
                TE::Pack(mident, name, tyargs, fields) => {
                    // add use of the struct name
                    visitor.add_datatype_use_def(mident, name, &name.loc());
                    for (fpos, fname, (_, (_, init_exp))) in fields.iter_mut() {
                        // add use of the field name
                        visitor.add_field_use_def(&mident.value, &name.value(), fname, &fpos);
                        // add field initialization expression
                        visitor.visit_exp(init_exp);
                    }
                    // add type params
                    for t in tyargs.iter_mut() {
                        visitor.visit_type(Some(exp_loc), t);
                    }
                    true
                }
                TE::Borrow(_, exp, field) => {
                    visitor.visit_exp(exp);
                    visitor.add_field_type_use_def(&exp.ty, &field.value(), &field.loc());
                    true
                }
                TE::PackVariant(_, _, _, _, _) => false, // TODO support these
                TE::VariantMatch(_, _, _) => false,      // TODO: support these
                TE::Match(_, _) => {
                    // These should be gone after match compilation.
                    debug_assert!(false);
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
                | TE::ErrorConstant { .. }
                | TE::UnresolvedError => false,
            }
        }

        let expanded_lambda = self.compiler_info.is_expanded_lambda(&_exp.exp.loc);
        if let Some(macro_call_info) = self.compiler_info.get_macro_info(&_exp.exp.loc) {
            debug_assert!(!expanded_lambda, "Compiler info issue");
            let MacroCallInfo {
                module,
                name,
                method_name,
                mut type_arguments,
                mut by_value_args,
            } = macro_call_info.clone();
            self.process_module_call(&module, &name, method_name, &mut type_arguments, None);
            by_value_args
                .iter_mut()
                .for_each(|a| self.visit_seq_item(a));
            let old_traverse_mode = self.traverse_only;
            // stop adding new use-defs etc.
            self.traverse_only = true;
            let result = visit_exp_inner(self, _exp);
            self.traverse_only = old_traverse_mode;
            result
        } else if expanded_lambda {
            let old_traverse_mode = self.traverse_only;
            // start adding new use-defs etc. when processing a lambda argument
            self.traverse_only = false;
            let result = visit_exp_inner(self, _exp);
            self.traverse_only = old_traverse_mode;
            result
        } else {
            visit_exp_inner(self, _exp)
        }
    }

    fn visit_type_custom(&mut self, exp_loc: Option<Loc>, ty: &mut N::Type) -> bool {
        if self.traverse_only {
            return true;
        }
        let loc = ty.loc;
        match &mut ty.value {
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
                    name_start.position.line_offset() as u32,
                    UseDef::new(
                        self.references,
                        self.alias_lengths,
                        use_pos.file_hash(),
                        name_start.into(),
                        *def_loc,
                        &use_name,
                        ident_type_def_loc,
                    ),
                );
                true
            }
            N::Type_::Apply(_, sp!(_, type_name), tyargs) => {
                if let N::TypeName_::ModuleType(mod_ident, struct_name) = type_name {
                    self.add_datatype_use_def(mod_ident, struct_name, &struct_name.loc());
                } // otherwise nothing to be done for other type names
                for t in tyargs.iter_mut() {
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

    fn visit_use_funs(&mut self, use_funs: &mut N::UseFuns) {
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
                    self.add_datatype_use_def(&mod_ident, &struct_name, &struct_name.loc());
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
