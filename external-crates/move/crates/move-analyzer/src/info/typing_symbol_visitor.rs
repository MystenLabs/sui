// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::symbols::{ignored_function, DefInfo, DefLoc, ModuleDefs, UseDef, UseDefMap, UseLoc};

use lsp_types::Position;
use move_command_line_common::files::FileHash;
use move_compiler::{diagnostics as diag, typing::visitor::TypingVisitorContext};
use move_ir_types::ast::ModuleDefinition;
use move_symbol_pool::Symbol;

use codespan_reporting::files::SimpleFiles;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Data used during symbolication over typed AST
pub struct TypingSymbolicator<'a> {
    /// Outermost definitions in a module (structs, consts, functions), keyd on a ModuleIdent
    /// string so that we can access it regardless of the ModuleIdent representation
    /// (e.g., in the parsing AST or in the typing AST)
    pub mod_outer_defs: &'a BTreeMap<String, ModuleDefs>,
    /// A mapping from file names to file content (used to obtain source file locations)
    pub files: &'a SimpleFiles<Symbol, String>,
    /// A mapping from file hashes to file IDs (used to obtain source file locations)
    pub file_id_mapping: &'a HashMap<FileHash, usize>,
    // A mapping from file IDs to a split vector of the lines in each file (used to build docstrings)
    /// Contains type params where relevant (e.g. when processing function definition)
    pub type_params: BTreeMap<Symbol, DefLoc>,
    /// Associates uses for a given definition to allow displaying all references
    pub references: &'a mut BTreeMap<DefLoc, BTreeSet<UseLoc>>,
    /// Additional information about definitions
    pub def_info: &'a mut BTreeMap<DefLoc, DefInfo>,
    /// A UseDefMap for a given module (needs to be appropriately set before the module
    /// processing starts)
    pub use_defs: UseDefMap,
    /// Alias lengths in access paths for a given module (needs to be appropriately
    /// set before the module processing starts)
    pub alias_lengths: &'a BTreeMap<Position, usize>,
    /// In some cases (e.g., when processing bodies of macros) we want to keep traversing
    /// the AST but without recording the actual metadata (uses, definitions, types, etc.)
    pub traverse_only: bool,
}

impl<'a> TypingVisitorContext for TypingSymbolicator<'a> {
    // Nothing to do -- we're not producing errors.
    fn add_warning_filter_scope(&mut self, _filter: diag::WarningFilters) {}

    // Nothing to do -- we're not producing errors.
    fn pop_warning_filter_scope(&mut self) {}

    fn visit_module_custom(
        &mut self,
        _ident: move_compiler::expansion::ast::ModuleIdent,
        _mdef: &mut move_compiler::typing::ast::ModuleDefinition,
    ) -> bool {
        false
    }

    fn visit_struct_custom(
        &mut self,
        _module: move_compiler::expansion::ast::ModuleIdent,
        _struct_name: move_compiler::parser::ast::DatatypeName,
        _sdef: &mut move_compiler::naming::ast::StructDefinition,
    ) -> bool {
        false
    }

    fn visit_enum_custom(
        &mut self,
        _module: move_compiler::expansion::ast::ModuleIdent,
        _enum_name: move_compiler::parser::ast::DatatypeName,
        _edef: &mut move_compiler::naming::ast::EnumDefinition,
    ) -> bool {
        false
    }

    fn visit_constant_custom(
        &mut self,
        _module: move_compiler::expansion::ast::ModuleIdent,
        _constant_name: move_compiler::parser::ast::ConstantName,
        _cdef: &mut move_compiler::typing::ast::Constant,
    ) -> bool {
        false
    }

    fn visit_function_custom(
        &mut self,
        _module: move_compiler::expansion::ast::ModuleIdent,
        _function_name: move_compiler::parser::ast::FunctionName,
        _fdef: &mut move_compiler::typing::ast::Function,
    ) -> bool {
        false
    }

    fn visit_seq_item_custom(
        &mut self,
        _seq_item: &mut move_compiler::typing::ast::SequenceItem,
    ) -> bool {
        false
    }

    fn visit_exp_custom(&mut self, _exp: &mut move_compiler::typing::ast::Exp) -> bool {
        false
    }
}

impl diag::PositionInfo for TypingSymbolicator<'_> {
    fn files(&self) -> &SimpleFiles<Symbol, std::sync::Arc<str>> {
        self.files
    }

    fn file_mapping(&self) -> &HashMap<FileHash, move_compiler::diagnostics::FileId> {
        self.file_id_mapping
    }
}
/*
impl<'a> TypingSymbolicator<'a> {
    /// Get symbols for the whole module
    fn mod_symbols(&mut self, mod_def: &ModuleDefinition, mod_ident_str: &str) {
        for (pos, name, fun) in &mod_def.functions {
            if ignored_function(*name) {
                continue;
            }
            // enter self-definition for function name (unwrap safe - done when inserting def)
            let name_start = get_start_loc(&pos, self.files, self.file_id_mapping).unwrap();
            let fun_info = self
                .def_info
                .get(&DefLoc::new(pos.file_hash(), name_start))
                .unwrap();
            let fun_type_def = def_info_to_type_def_loc(self.mod_outer_defs, fun_info);
            let use_def = UseDef::new(
                self.references,
                self.alias_lengths,
                pos.file_hash(),
                name_start,
                pos.file_hash(),
                name_start,
                name,
                fun_type_def,
            );

            self.use_defs.insert(name_start.line, use_def);
            self.fun_symbols(fun);
        }

        for (pos, name, c) in &mod_def.constants {
            // enter self-definition for const name (unwrap safe - done when inserting def)
            let name_start = get_start_loc(&pos, self.files, self.file_id_mapping).unwrap();
            let const_info = self
                .def_info
                .get(&DefLoc::new(pos.file_hash(), name_start))
                .unwrap();
            let ident_type_def_loc = def_info_to_type_def_loc(self.mod_outer_defs, const_info);
            self.use_defs.insert(
                name_start.line,
                UseDef::new(
                    self.references,
                    self.alias_lengths,
                    pos.file_hash(),
                    name_start,
                    pos.file_hash(),
                    name_start,
                    name,
                    ident_type_def_loc,
                ),
            );
            // scope must be passed here but it's not expected to be populated
            let mut scope = OrdMap::new();
            self.exp_symbols(&c.value, &mut scope);
        }

        for (pos, name, s) in &mod_def.structs {
            // enter self-definition for struct name (unwrap safe - done when inserting def)
            let name_start = get_start_loc(&pos, self.files, self.file_id_mapping).unwrap();
            let struct_info = self
                .def_info
                .get(&DefLoc::new(pos.file_hash(), name_start))
                .unwrap();
            let struct_type_def = def_info_to_type_def_loc(self.mod_outer_defs, struct_info);
            self.use_defs.insert(
                name_start.line,
                UseDef::new(
                    self.references,
                    self.alias_lengths,
                    pos.file_hash(),
                    name_start,
                    pos.file_hash(),
                    name_start,
                    name,
                    struct_type_def,
                ),
            );

            self.struct_symbols(s, name, mod_ident_str);
        }
        self.use_funs_symbols(&mod_def.use_funs);
    }

    /// Get symbols for struct definition
    fn struct_symbols(
        &mut self,
        struct_def: &StructDefinition,
        _struct_name: &Symbol,
        _mod_ident_str: &str,
    ) {
        // create scope designated to contain type parameters (if any)
        let mut tp_scope = BTreeMap::new();
        for stp in &struct_def.type_parameters {
            self.add_type_param(&stp.param, &mut tp_scope);
        }

        self.type_params = tp_scope;
        if let StructFields::Defined(positional, fields) = &struct_def.fields {
            for (fpos, fname, (_, t)) in fields {
                self.add_type_id_use_def(t);
                if !positional {
                    // Enter self-definition for field name (unwrap safe - done when inserting def),
                    // but only if the fields are named. Positional fields, introduced in Move 2024
                    // version of the language, have "fake" locations and could make the displayed
                    // results confusing. The reason for "fake" locations is that a struct has one
                    // internal representation in the compiler for both structs with named and
                    // positional fields (and the latter's fields don't have the actual names).
                    let start = get_start_loc(&fpos, self.files, self.file_id_mapping).unwrap();
                    let field_info = DefInfo::Type(t.clone());
                    let ident_type_def_loc =
                        def_info_to_type_def_loc(self.mod_outer_defs, &field_info);
                    self.use_defs.insert(
                        start.line,
                        UseDef::new(
                            self.references,
                            self.alias_lengths,
                            fpos.file_hash(),
                            start,
                            fpos.file_hash(),
                            start,
                            fname,
                            ident_type_def_loc,
                        ),
                    );
                }
            }
        }
    }

    /// Get symbols for a function definition
    fn fun_symbols(&mut self, fun: &Function) {
        // create scope designated to contain type parameters (if any)
        let mut tp_scope = BTreeMap::new();
        for tp in &fun.signature.type_parameters {
            self.add_type_param(tp, &mut tp_scope);
        }
        self.type_params = tp_scope;

        // scope for the main function scope (for parameters and
        // function body)
        let mut scope = OrdMap::new();

        for (mutability, pname, ptype) in &fun.signature.parameters {
            self.add_type_id_use_def(ptype);

            // add definition of the parameter
            self.add_local_def(
                &pname.loc,
                &pname.value.name,
                &mut scope,
                ptype.clone(),
                false, /* with_let */
                matches!(mutability, Mutability::Mut(_)),
            );
        }

        match &fun.body.value {
            FunctionBody_::Defined((use_funs, sequence)) => {
                self.use_funs_symbols(use_funs);
                for seq_item in sequence {
                    self.seq_item_symbols(&mut scope, seq_item);
                }
            }
            FunctionBody_::Macro | FunctionBody_::Native => (),
        }

        // process return types
        self.add_type_id_use_def(&fun.signature.return_type);

        // clear type params from the scope
        self.type_params.clear();
    }

    /// Get symbols for a sequence representing function body
    fn seq_item_symbols(&mut self, scope: &mut OrdMap<Symbol, LocalDef>, seq_item: &SequenceItem) {
        use SequenceItem_ as I;
        match &seq_item.value {
            I::Seq(e) => self.exp_symbols(e, scope),
            I::Declare(lvalues) => self.lvalue_list_symbols(true, lvalues, scope),
            I::Bind(lvalues, opt_types, e) => {
                // process RHS first to avoid accidentally binding its identifiers to LHS (which now
                // will be put into the current scope only after RHS is processed)
                self.exp_symbols(e, scope);
                for opt_t in opt_types {
                    match opt_t {
                        Some(t) => self.add_type_id_use_def(t),
                        None => (),
                    }
                }
                self.lvalue_list_symbols(true, lvalues, scope);
            }
        }
    }

    /// Get symbols for a list of lvalues
    fn lvalue_list_symbols(
        &mut self,
        define: bool,
        lvalues: &LValueList,
        scope: &mut OrdMap<Symbol, LocalDef>,
    ) {
        for lval in &lvalues.value {
            self.lvalue_symbols(define, lval, scope, false /* for unpack */);
        }
    }

    /// Get symbols for a single lvalue
    fn lvalue_symbols(
        &mut self,
        define: bool,
        lval: &LValue,
        scope: &mut OrdMap<Symbol, LocalDef>,
        for_unpack: bool,
    ) {
        match &lval.value {
            LValue_::Var {
                mut_, var, ty: t, ..
            } => {
                if define {
                    self.add_local_def(
                        &var.loc,
                        &var.value.name,
                        scope,
                        *t.clone(),
                        define && !for_unpack, // with_let (only for simple definition, e.g., `let t = 1;``)
                        mut_.map(|m| matches!(m, Mutability::Mut(_)))
                            .unwrap_or_default(),
                    );
                } else {
                    self.add_local_use_def(&var.value.name, &var.loc, scope)
                }
            }
            LValue_::Unpack(ident, name, tparams, fields) => {
                self.unpack_symbols(define, ident, name, tparams, fields, scope);
            }
            LValue_::BorrowUnpack(_, ident, name, tparams, fields) => {
                self.unpack_symbols(define, ident, name, tparams, fields, scope);
            }
            LValue_::Ignore => (),
            LValue_::UnpackVariant(..) | LValue_::BorrowUnpackVariant(..) => {
                debug_assert!(false, "Enums are not supported by move analyzser.");
            }
        }
    }

    /// Get symbols for the unpack statement
    fn unpack_symbols(
        &mut self,
        define: bool,
        ident: &ModuleIdent,
        name: &DatatypeName,
        tparams: &Vec<Type>,
        fields: &Fields<(Type, LValue)>,
        scope: &mut OrdMap<Symbol, LocalDef>,
    ) {
        // add use of the struct name
        self.add_struct_use_def(ident, &name.value(), &name.loc());
        for (fpos, fname, (_, (_, lvalue))) in fields {
            // add use of the field name
            self.add_field_use_def(&ident.value, &name.value(), fname, &fpos);
            // add definition or use of a variable used for struct field unpacking
            self.lvalue_symbols(define, lvalue, scope, true /* for_unpack */);
        }
        // add type params
        for t in tparams {
            self.add_type_id_use_def(t);
        }
    }

    /// Get symbols for an expression
    fn exp_symbols(&mut self, exp: &Exp, scope: &mut OrdMap<Symbol, LocalDef>) {
        use UnannotatedExp_ as E;
        match &exp.exp.value {
            E::Move { from_user: _, var } => {
                self.add_local_use_def(&var.value.name, &var.loc, scope)
            }
            E::Copy { from_user: _, var } => {
                self.add_local_use_def(&var.value.name, &var.loc, scope)
            }
            E::Use(var) => self.add_local_use_def(&var.value.name, &var.loc, scope),
            E::Constant(mod_ident, name) => {
                self.add_const_use_def(mod_ident, &name.value(), &name.loc())
            }
            E::ModuleCall(mod_call) => self.mod_call_symbols(
                &mod_call.module,
                mod_call.name,
                mod_call.method_name,
                &mod_call.type_arguments,
                Some(&mod_call.arguments),
                scope,
            ),
            E::Builtin(builtin_fun, exp) => {
                use BuiltinFunction_ as BF;
                match &builtin_fun.value {
                    BF::Freeze(t) => self.add_type_id_use_def(t),
                    BF::Assert(_) => (),
                }
                self.exp_symbols(exp, scope);
            }
            E::Vector(_, _, t, exp) => {
                self.add_type_id_use_def(t);
                self.exp_symbols(exp, scope);
            }
            E::IfElse(cond, t, f) => {
                self.exp_symbols(cond, scope);
                self.exp_symbols(t, scope);
                self.exp_symbols(f, scope);
            }
            E::While(_, cond, body) => {
                self.exp_symbols(cond, scope);
                self.exp_symbols(body, scope);
            }
            E::Loop { body, .. } => {
                self.exp_symbols(body, scope);
            }
            E::NamedBlock(_, (use_funs, sequence)) => {
                let old_traverse_mode = self.traverse_only;
                // start adding new use-defs etc. when processing an argument
                if use_funs.color == 0 {
                    self.traverse_only = false;
                }
                self.use_funs_symbols(use_funs);
                // a named block is a new var scope
                let mut new_scope = scope.clone();
                for seq_item in sequence {
                    self.seq_item_symbols(&mut new_scope, seq_item);
                }
                if use_funs.color == 0 {
                    self.traverse_only = old_traverse_mode;
                }
            }
            E::Block((use_funs, sequence)) => {
                let old_traverse_mode = self.traverse_only;
                // start adding new use-defs etc. when processing arguments
                if use_funs.color == 0 {
                    self.traverse_only = false;
                }
                self.use_funs_symbols(use_funs);
                // a block is a new var scope
                let mut new_scope = scope.clone();
                for seq_item in sequence {
                    self.seq_item_symbols(&mut new_scope, seq_item);
                }
                if use_funs.color == 0 {
                    self.traverse_only = old_traverse_mode;
                }
            }
            E::IDEAnnotation(info, exp) => {
                match info {
                    IDEInfo::MacroCallInfo(MacroCallInfo {
                        module,
                        name,
                        method_name,
                        type_arguments,
                        by_value_args,
                    }) => {
                        self.mod_call_symbols(
                            module,
                            *name,
                            *method_name,
                            type_arguments,
                            None,
                            scope,
                        );
                        by_value_args
                            .iter()
                            .for_each(|a| self.seq_item_symbols(scope, a));
                        let old_traverse_mode = self.traverse_only;
                        // stop adding new use-defs etc.
                        self.traverse_only = true;
                        self.exp_symbols(exp, scope);
                        self.traverse_only = old_traverse_mode;
                    }
                    IDEInfo::ExpandedLambda => {
                        let old_traverse_mode = self.traverse_only;
                        // start adding new use-defs etc. when processing a lambda argument
                        self.traverse_only = false;
                        self.exp_symbols(exp, scope);
                        self.traverse_only = old_traverse_mode;
                    }
                }
            }
            E::Assign(lvalues, opt_types, e) => {
                self.lvalue_list_symbols(false, lvalues, scope);
                for opt_t in opt_types {
                    match opt_t {
                        Some(t) => self.add_type_id_use_def(t),
                        None => (),
                    }
                }
                self.exp_symbols(e, scope);
            }
            E::Mutate(lhs, rhs) => {
                self.exp_symbols(lhs, scope);
                self.exp_symbols(rhs, scope);
            }
            E::Return(exp) => self.exp_symbols(exp, scope),
            E::Abort(exp) => self.exp_symbols(exp, scope),
            E::Give(_, exp) => self.exp_symbols(exp, scope),
            E::Dereference(exp) => self.exp_symbols(exp, scope),
            E::UnaryExp(_, exp) => self.exp_symbols(exp, scope),
            E::BinopExp(lhs, _, _, rhs) => {
                self.exp_symbols(lhs, scope);
                self.exp_symbols(rhs, scope);
            }
            E::Pack(ident, name, tparams, fields) => {
                self.pack_symbols(ident, name, tparams, fields, scope);
            }
            E::PackVariant(ident, name, _, tparams, fields) => {
                self.pack_symbols(ident, name, tparams, fields, scope);
            }
            E::ExpList(list_items) => {
                for item in list_items {
                    let exp = match item {
                        // TODO: are types important for symbolication here (and, more generally,
                        // what's a splat?)
                        ExpListItem::Single(e, _) => e,
                        ExpListItem::Splat(_, e, _) => e,
                    };
                    self.exp_symbols(exp, scope);
                }
            }
            E::Borrow(_, exp, field) => {
                self.exp_symbols(exp, scope);
                // get expression type to match fname to a struct def
                self.add_field_type_use_def(&exp.ty, &field.value(), &field.loc());
            }
            E::TempBorrow(_, exp) => {
                self.exp_symbols(exp, scope);
            }
            E::BorrowLocal(_, var) => self.add_local_use_def(&var.value.name, &var.loc, scope),
            E::Cast(exp, t) => {
                self.exp_symbols(exp, scope);
                self.add_type_id_use_def(t);
            }
            E::Annotate(exp, t) => {
                self.exp_symbols(exp, scope);
                self.add_type_id_use_def(t);
            }
            E::AutocompleteDotAccess { base_exp, .. } => {
                self.exp_symbols(base_exp, scope);
            }
            E::Unit { .. }
            | E::Value(_)
            | E::Continue(_)
            | E::ErrorConstant { .. }
            | E::UnresolvedError
            | E::Match(_, _) // TODO: support it
            | E::VariantMatch(_, _, _) => (), // TODO: support it
        }
    }

    fn use_funs_symbols(&mut self, use_funs: &UseFuns) {
        let UseFuns {
            resolved,
            implicit_candidates,
            color: _,
        } = use_funs;

        // at typing there should be no unresolved candidates (it's also checked in typing
        // translaction pass)
        assert!(implicit_candidates.is_empty());

        for uses in resolved.values() {
            for (use_loc, use_name, u) in uses {
                if let TypeName_::ModuleType(mod_ident, struct_name) = u.tname.value {
                    self.add_struct_use_def(&mod_ident, &struct_name.value(), &struct_name.loc());
                } // otherwise nothing to be done for other type names
                let (module_ident, fun_def) = u.target_function;
                let fun_def_name = fun_def.value();
                let fun_def_loc = fun_def.loc();
                self.add_fun_use_def(&module_ident, &fun_def_name, use_name, &use_loc);
                self.add_fun_use_def(&module_ident, &fun_def_name, &fun_def_name, &fun_def_loc);
            }
        }
    }

    /// Add a type for a struct field given its type
    fn add_field_type_use_def(&mut self, field_type: &Type, use_name: &Symbol, use_pos: &Loc) {
        let sp!(_, typ) = field_type;
        match typ {
            Type_::Ref(_, t) => self.add_field_type_use_def(t, use_name, use_pos),
            Type_::Apply(_, sp!(_, TypeName_::ModuleType(sp!(_, mod_ident), struct_name)), _) => {
                self.add_field_use_def(mod_ident, &struct_name.value(), use_name, use_pos);
            }
            _ => (),
        }
    }

    fn mod_call_symbols(
        &mut self,
        mod_ident: &E::ModuleIdent,
        name: FunctionName,
        method_name: Option<Name>,
        type_arguments: &[Type],
        arguments: Option<&Exp>,
        scope: &mut OrdMap<Symbol, LocalDef>,
    ) {
        let Some(mod_def) = self
            .mod_outer_defs
            .get(&expansion_mod_ident_to_map_key(&mod_ident.value))
        else {
            // this should not happen but due to a fix in unifying generation of mod ident map keys,
            // but just in case - it's better to report it than to crash the analyzer due to
            // unchecked unwrap
            eprintln!(
                "WARNING: could not locate module {:?} when processing a call to {}{}",
                mod_ident, mod_ident, name
            );
            return;
        };

        if mod_def.functions.get(&name.value()).is_none() {
            return;
        }

        let fun_name = name.value();
        // a function name (same as fun_name) or method name (different from fun_name)
        let fun_use = method_name.unwrap_or_else(|| sp(name.loc(), name.value()));
        self.add_fun_use_def(mod_ident, &fun_name, &fun_use.value, &fun_use.loc);
        // handle type parameters
        for t in type_arguments {
            self.add_type_id_use_def(t);
        }

        // handle arguments
        if let Some(args) = arguments {
            self.exp_symbols(args, scope);
        }
    }

    /// Get symbols for the pack expression
    fn pack_symbols(
        &mut self,
        ident: &ModuleIdent,
        name: &DatatypeName,
        tparams: &Vec<Type>,
        fields: &Fields<(Type, Exp)>,
        scope: &mut OrdMap<Symbol, LocalDef>,
    ) {
        // add use of the struct name
        self.add_struct_use_def(ident, &name.value(), &name.loc());
        for (fpos, fname, (_, (_, init_exp))) in fields {
            // add use of the field name
            self.add_field_use_def(&ident.value, &name.value(), fname, &fpos);
            // add field initialization expression
            self.exp_symbols(init_exp, scope);
        }
        // add type params
        for t in tparams {
            self.add_type_id_use_def(t);
        }
    }

    /// Helper functions

    /// Add type parameter to a scope holding type params
    fn add_type_param(&mut self, tp: &TParam, tp_scope: &mut BTreeMap<Symbol, DefLoc>) {
        if self.traverse_only {
            return;
        }
        match get_start_loc(
            &tp.user_specified_name.loc,
            self.files,
            self.file_id_mapping,
        ) {
            Some(start) => {
                let tname = tp.user_specified_name.value;
                let fhash = tp.user_specified_name.loc.file_hash();
                // enter self-definition for type param
                let type_def_info =
                    DefInfo::Type(sp(tp.user_specified_name.loc, Type_::Param(tp.clone())));
                let ident_type_def_loc =
                    def_info_to_type_def_loc(self.mod_outer_defs, &type_def_info);

                self.use_defs.insert(
                    start.line,
                    UseDef::new(
                        self.references,
                        self.alias_lengths,
                        fhash,
                        start,
                        fhash,
                        start,
                        &tname,
                        ident_type_def_loc,
                    ),
                );
                self.def_info
                    .insert(DefLoc::new(fhash, start), type_def_info);
                let exists = tp_scope.insert(tname, DefLoc::new(fhash, start));
                debug_assert!(exists.is_none());
            }
            None => {
                debug_assert!(false);
            }
        };
    }

    /// Add use of a const identifier
    fn add_const_use_def(&mut self, module_ident: &ModuleIdent, use_name: &Symbol, use_pos: &Loc) {
        if self.traverse_only {
            return;
        }
        let mod_ident_str = expansion_mod_ident_to_map_key(&module_ident.value);
        let Some(mod_defs) = self.mod_outer_defs.get(&mod_ident_str) else {
            return;
        };
        // insert use of the const's module
        let mod_name = module_ident.value.module;
        if let Some(mod_name_start) =
            get_start_loc(&mod_name.loc(), self.files, self.file_id_mapping)
        {
            // a module will not be present if a constant belongs to an implicit module
            self.use_defs.insert(
                mod_name_start.line,
                UseDef::new(
                    self.references,
                    self.alias_lengths,
                    mod_name.loc().file_hash(),
                    mod_name_start,
                    mod_defs.fhash,
                    mod_defs.start,
                    &mod_name.value(),
                    None,
                ),
            );
        }

        let Some(name_start) = get_start_loc(use_pos, self.files, self.file_id_mapping) else {
            debug_assert!(false);
            return;
        };
        if let Some(const_def) = mod_defs.constants.get(use_name) {
            let def_fhash = self.mod_outer_defs.get(&mod_ident_str).unwrap().fhash;
            let const_info = self
                .def_info
                .get(&DefLoc::new(def_fhash, const_def.name_start))
                .unwrap();
            let ident_type_def_loc = def_info_to_type_def_loc(self.mod_outer_defs, const_info);
            self.use_defs.insert(
                name_start.line,
                UseDef::new(
                    self.references,
                    self.alias_lengths,
                    use_pos.file_hash(),
                    name_start,
                    def_fhash,
                    const_def.name_start,
                    use_name,
                    ident_type_def_loc,
                ),
            );
        }
    }

    /// Add use of a function identifier
    fn add_fun_use_def(
        &mut self,
        module_ident: &ModuleIdent,
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
        if let Some(mod_name_start) =
            get_start_loc(&mod_name.loc(), self.files, self.file_id_mapping)
        {
            // a module will not be present if a function belongs to an implicit module
            self.use_defs.insert(
                mod_name_start.line,
                UseDef::new(
                    self.references,
                    self.alias_lengths,
                    mod_name.loc().file_hash(),
                    mod_name_start,
                    mod_defs.fhash,
                    mod_defs.start,
                    &mod_name.value(),
                    None,
                ),
            );
        }

        if add_fun_use_def(
            fun_def_name,
            self.mod_outer_defs,
            self.files,
            self.file_id_mapping,
            mod_ident_str,
            mod_defs,
            use_name,
            use_pos,
            self.references,
            self.def_info,
            &mut self.use_defs,
            self.alias_lengths,
        )
        .is_none()
        {
            debug_assert!(false);
        }
    }

    /// Add use of a struct identifier
    fn add_struct_use_def(&mut self, module_ident: &ModuleIdent, use_name: &Symbol, use_pos: &Loc) {
        if self.traverse_only {
            return;
        }
        let mod_ident_str = expansion_mod_ident_to_map_key(&module_ident.value);
        let Some(mod_defs) = self.mod_outer_defs.get(&mod_ident_str) else {
            return;
        };
        // insert use of the struct's module
        let mod_name = module_ident.value.module;
        if let Some(mod_name_start) =
            get_start_loc(&mod_name.loc(), self.files, self.file_id_mapping)
        {
            // a module will not be present if a struct belongs to an implicit module
            self.use_defs.insert(
                mod_name_start.line,
                UseDef::new(
                    self.references,
                    self.alias_lengths,
                    mod_name.loc().file_hash(),
                    mod_name_start,
                    mod_defs.fhash,
                    mod_defs.start,
                    &mod_name.value(),
                    None,
                ),
            );
        }

        if add_struct_use_def(
            self.mod_outer_defs,
            self.files,
            self.file_id_mapping,
            mod_ident_str,
            mod_defs,
            use_name,
            use_pos,
            self.references,
            self.def_info,
            &mut self.use_defs,
            self.alias_lengths,
        )
        .is_none()
        {
            debug_assert!(false);
        }
    }

    /// Add use of a struct field identifier
    fn add_field_use_def(
        &mut self,
        module_ident: &ModuleIdent_,
        struct_name: &Symbol,
        use_name: &Symbol,
        use_pos: &Loc,
    ) {
        if self.traverse_only {
            return;
        }
        let mod_ident_str = expansion_mod_ident_to_map_key(module_ident);
        let Some(name_start) = get_start_loc(use_pos, self.files, self.file_id_mapping) else {
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
                        .get(&DefLoc::new(def_fhash, fdef.start))
                        .unwrap();
                    let ident_type_def_loc =
                        def_info_to_type_def_loc(self.mod_outer_defs, field_info);
                    self.use_defs.insert(
                        name_start.line,
                        UseDef::new(
                            self.references,
                            self.alias_lengths,
                            use_pos.file_hash(),
                            name_start,
                            def_fhash,
                            fdef.start,
                            use_name,
                            ident_type_def_loc,
                        ),
                    );
                }
            }
        }
    }

    /// Add use of a type identifier
    fn add_type_id_use_def(&mut self, id_type: &Type) {
        if self.traverse_only {
            return;
        }
        let sp!(pos, typ) = id_type;
        match typ {
            Type_::Ref(_, t) => self.add_type_id_use_def(t),
            Type_::Param(tparam) => {
                let sp!(use_pos, use_name) = tparam.user_specified_name;
                match get_start_loc(pos, self.files, self.file_id_mapping) {
                    Some(name_start) => match self.type_params.get(&use_name) {
                        Some(def_loc) => {
                            let ident_type_def_loc = type_def_loc(self.mod_outer_defs, id_type);
                            self.use_defs.insert(
                                name_start.line,
                                UseDef::new(
                                    self.references,
                                    self.alias_lengths,
                                    use_pos.file_hash(),
                                    name_start,
                                    def_loc.fhash,
                                    def_loc.start,
                                    &use_name,
                                    ident_type_def_loc,
                                ),
                            );
                        }
                        None => debug_assert!(false),
                    },
                    None => debug_assert!(false), // a type param should not be missing
                }
            }
            Type_::Apply(_, sp!(_, type_name), tparams) => {
                if let TypeName_::ModuleType(mod_ident, struct_name) = type_name {
                    self.add_struct_use_def(mod_ident, &struct_name.value(), &struct_name.loc());
                } // otherwise nothing to be done for other type names
                for t in tparams {
                    self.add_type_id_use_def(t);
                }
            }
            Type_::Fun(v, t) => {
                for t in v {
                    self.add_type_id_use_def(t);
                }
                self.add_type_id_use_def(t);
            }
            Type_::Unit | Type_::Var(_) | Type_::Anything | Type_::UnresolvedError => (), // nothing to be done for the other types
        }
    }

    /// Add a defintion of a local (including function params).
    fn add_local_def(
        &mut self,
        pos: &Loc,
        name: &Symbol,
        scope: &mut OrdMap<Symbol, LocalDef>,
        def_type: Type,
        with_let: bool,
        mutable: bool,
    ) {
        if self.traverse_only {
            return;
        }
        match get_start_loc(pos, self.files, self.file_id_mapping) {
            Some(name_start) => {
                let def_loc = DefLoc::new(pos.file_hash(), name_start);
                scope.insert(
                    *name,
                    LocalDef {
                        def_loc,
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
                        pos.file_hash(),
                        name_start,
                        pos.file_hash(),
                        name_start,
                        name,
                        ident_type_def_loc,
                    ),
                );
                self.def_info.insert(
                    DefLoc::new(pos.file_hash(), name_start),
                    DefInfo::Local(*name, def_type, with_let, mutable),
                );
            }
            None => {
                debug_assert!(false);
            }
        }
    }

    /// Add a use for and identifier whose definition is expected to be local to a function, and
    /// pair it with an appropriate definition
    fn add_local_use_def(
        &mut self,
        use_name: &Symbol,
        use_pos: &Loc,
        scope: &OrdMap<Symbol, LocalDef>,
    ) {
        if self.traverse_only {
            return;
        }
        let name_start = match get_start_loc(use_pos, self.files, self.file_id_mapping) {
            Some(v) => v,
            None => {
                debug_assert!(false);
                return;
            }
        };

        if let Some(local_def) = scope.get(use_name) {
            let ident_type_def_loc = type_def_loc(self.mod_outer_defs, &local_def.def_type);
            self.use_defs.insert(
                name_start.line,
                UseDef::new(
                    self.references,
                    self.alias_lengths,
                    use_pos.file_hash(),
                    name_start,
                    local_def.def_loc.fhash,
                    local_def.def_loc.start,
                    use_name,
                    ident_type_def_loc,
                ),
            );
        }
    }
}

/// Add use of a function identifier
fn add_fun_use_def(
    fun_def_name: &Symbol, // may be different from use_name for methods
    mod_outer_defs: &BTreeMap<String, ModuleDefs>,
    files: &SimpleFiles<Symbol, String>,
    file_id_mapping: &HashMap<FileHash, usize>,
    mod_ident_str: String,
    mod_defs: &ModuleDefs,
    use_name: &Symbol,
    use_pos: &Loc,
    references: &mut BTreeMap<DefLoc, BTreeSet<UseLoc>>,
    def_info: &BTreeMap<DefLoc, DefInfo>,
    use_defs: &mut UseDefMap,
    alias_lengths: &BTreeMap<Position, usize>,
) -> Option<UseDef> {
    let Some(name_start) = get_start_loc(use_pos, files, file_id_mapping) else {
        debug_assert!(false);
        return None;
    };
    if let Some(func_def) = mod_defs.functions.get(fun_def_name) {
        let def_fhash = mod_outer_defs.get(&mod_ident_str).unwrap().fhash;
        let fun_info = def_info
            .get(&DefLoc::new(def_fhash, func_def.start))
            .unwrap();
        let ident_type_def_loc = def_info_to_type_def_loc(mod_outer_defs, fun_info);
        let ud = UseDef::new(
            references,
            alias_lengths,
            use_pos.file_hash(),
            name_start,
            def_fhash,
            func_def.start,
            use_name,
            ident_type_def_loc,
        );
        use_defs.insert(name_start.line, ud.clone());
        return Some(ud);
    }
    None
}

/// Add use of a struct identifier
fn add_struct_use_def(
    mod_outer_defs: &BTreeMap<String, ModuleDefs>,
    files: &SimpleFiles<Symbol, String>,
    file_id_mapping: &HashMap<FileHash, usize>,
    mod_ident_str: String,
    mod_defs: &ModuleDefs,
    use_name: &Symbol,
    use_pos: &Loc,
    references: &mut BTreeMap<DefLoc, BTreeSet<UseLoc>>,
    def_info: &BTreeMap<DefLoc, DefInfo>,
    use_defs: &mut UseDefMap,
    alias_lengths: &BTreeMap<Position, usize>,
) -> Option<UseDef> {
    let Some(name_start) = get_start_loc(use_pos, files, file_id_mapping) else {
        debug_assert!(false);
        return None;
    };
    if let Some(def) = mod_defs.structs.get(use_name) {
        let def_fhash = mod_outer_defs.get(&mod_ident_str).unwrap().fhash;
        let struct_info = def_info
            .get(&DefLoc::new(def_fhash, def.name_start))
            .unwrap();
        let ident_type_def_loc = def_info_to_type_def_loc(mod_outer_defs, struct_info);
        let ud = UseDef::new(
            references,
            alias_lengths,
            use_pos.file_hash(),
            name_start,
            def_fhash,
            def.name_start,
            use_name,
            ident_type_def_loc,
        );
        use_defs.insert(name_start.line, ud.clone());
        return Some(ud);
    }
    None
}

fn def_info_to_type_def_loc(
    mod_outer_defs: &BTreeMap<String, ModuleDefs>,
    def_info: &DefInfo,
) -> Option<DefLoc> {
    match def_info {
        DefInfo::Type(t) => type_def_loc(mod_outer_defs, t),
        DefInfo::Function(..) => None,
        DefInfo::Struct(mod_ident, name, ..) => find_struct(mod_outer_defs, mod_ident, name),
        DefInfo::Field(.., t, _) => type_def_loc(mod_outer_defs, t),
        DefInfo::Local(_, t, _, _) => type_def_loc(mod_outer_defs, t),
        DefInfo::Const(_, _, t, _, _) => type_def_loc(mod_outer_defs, t),
        DefInfo::Module(..) => None,
    }
}

fn def_info_doc_string(def_info: &DefInfo) -> Option<String> {
    match def_info {
        DefInfo::Type(_) => None,
        DefInfo::Function(.., s) => s.clone(),
        DefInfo::Struct(.., s) => s.clone(),
        DefInfo::Field(.., s) => s.clone(),
        DefInfo::Local(..) => None,
        DefInfo::Const(.., s) => s.clone(),
        DefInfo::Module(_, s) => s.clone(),
    }
}

fn type_def_loc(mod_outer_defs: &BTreeMap<String, ModuleDefs>, sp!(_, t): &Type) -> Option<DefLoc> {
    match t {
        Type_::Ref(_, r) => type_def_loc(mod_outer_defs, r),
        Type_::Apply(_, sp!(_, TypeName_::ModuleType(sp!(_, mod_ident), struct_name)), _) => {
            find_struct(mod_outer_defs, mod_ident, &struct_name.value())
        }
        _ => None,
    }
}

fn find_struct(
    mod_outer_defs: &BTreeMap<String, ModuleDefs>,
    mod_ident: &ModuleIdent_,
    struct_name: &Symbol,
) -> Option<DefLoc> {
    let mod_ident_str = expansion_mod_ident_to_map_key(mod_ident);
    let mod_defs = match mod_outer_defs.get(&mod_ident_str) {
        Some(v) => v,
        None => return None,
    };
    mod_defs.structs.get(struct_name).map(|struct_def| {
        let fhash = mod_defs.fhash;
        let start = struct_def.name_start;
        DefLoc::new(fhash, start)
    })
}
*/
