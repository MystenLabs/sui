// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    command_line::compiler::Visitor,
    diagnostics::WarningFilters,
    expansion::ast::ModuleIdent,
    naming::ast as N,
    parser::ast::{ConstantName, DatatypeName, FunctionName, VariantName},
    shared::CompilationEnv,
    typing::ast as T,
};

use move_ir_types::location::Loc;
use move_proc_macros::growing_stack;

pub type TypingVisitorObj = Box<dyn TypingVisitor>;

pub trait TypingVisitor {
    fn visit(&mut self, env: &mut CompilationEnv, program: &T::Program);

    fn visitor(self) -> Visitor
    where
        Self: 'static + Sized,
    {
        Visitor::TypingVisitor(Box::new(self))
    }
}

pub trait TypingVisitorConstructor {
    type Context<'a>: Sized + TypingVisitorContext;

    fn context<'a>(env: &'a mut CompilationEnv, program: &T::Program) -> Self::Context<'a>;

    fn visit(&mut self, env: &mut CompilationEnv, program: &T::Program) {
        let mut context = Self::context(env, program);
        context.visit(program);
    }
}

pub enum LValueKind {
    Bind,
    Assign,
}

pub trait TypingVisitorContext {
    fn add_warning_filter_scope(&mut self, filter: WarningFilters);
    fn pop_warning_filter_scope(&mut self);

    /// Indicates if types should be visited during the traversal of other forms (struct and enum
    /// definitions, function signatures, expressions, etc.). This will not visit lvalue types
    /// unless VISIT_LVALUES is also enabled.
    const VISIT_TYPES: bool = false;

    /// Indicates if lvalues should be visited during the traversal of sequence forms.
    const VISIT_LVALUES: bool = false;

    /// Indicates if use_funs should be visited during the traversal.
    const VISIT_USE_FUNS: bool = false;

    /// By default, the visitor will visit all modules, and all functions and constants therein.
    /// For functions and constants, it will also visit their expressions. To change this behavior,
    /// consider enabling `VISIT_LVALUES`, VISIT_TYPES`, and `VISIT_USE_FUNS` or overwriting one of
    /// the `visit_<name>_custom` functions defined on this trait, as appropriate.
    fn visit(&mut self, program: &T::Program) {
        for (mident, mdef) in program.modules.key_cloned_iter() {
            self.visit_module(mident, mdef);
        }
    }

    // -- MODULE DEFINITIONS --

    fn visit_module_custom(&mut self, _ident: ModuleIdent, _mdef: &T::ModuleDefinition) -> bool {
        false
    }

    fn visit_module(&mut self, ident: ModuleIdent, mdef: &T::ModuleDefinition) {
        self.add_warning_filter_scope(mdef.warning_filter.clone());
        if self.visit_module_custom(ident, mdef) {
            self.pop_warning_filter_scope();
            return;
        }
        for (struct_name, sdef) in mdef.structs.key_cloned_iter() {
            self.visit_struct(ident, struct_name, sdef)
        }
        for (enum_name, edef) in mdef.enums.key_cloned_iter() {
            self.visit_enum(ident, enum_name, edef)
        }
        for (constant_name, cdef) in mdef.constants.key_cloned_iter() {
            self.visit_constant(ident, constant_name, cdef)
        }
        for (function_name, fdef) in mdef.functions.key_cloned_iter() {
            self.visit_function(ident, function_name, fdef)
        }
        if Self::VISIT_USE_FUNS {
            self.visit_use_funs(&mdef.use_funs);
        }

        self.pop_warning_filter_scope();
    }

    // -- MODULE MEMBER DEFINITIONS --

    fn visit_struct_custom(
        &mut self,
        _module: ModuleIdent,
        _struct_name: DatatypeName,
        _sdef: &N::StructDefinition,
    ) -> bool {
        false
    }

    fn visit_struct(
        &mut self,
        module: ModuleIdent,
        struct_name: DatatypeName,
        sdef: &N::StructDefinition,
    ) {
        self.add_warning_filter_scope(sdef.warning_filter.clone());
        if self.visit_struct_custom(module, struct_name, sdef) {
            self.pop_warning_filter_scope();
            return;
        }
        if Self::VISIT_TYPES {
            match &sdef.fields {
                N::StructFields::Defined(_, fields) => {
                    for (_, _, (_, ty)) in fields {
                        self.visit_type(None, ty)
                    }
                }
                N::StructFields::Native(_) => (),
            }
        }
        self.pop_warning_filter_scope();
    }

    fn visit_enum_custom(
        &mut self,
        _module: ModuleIdent,
        _enum_name: DatatypeName,
        _edef: &N::EnumDefinition,
    ) -> bool {
        false
    }

    fn visit_enum(
        &mut self,
        module: ModuleIdent,
        enum_name: DatatypeName,
        edef: &N::EnumDefinition,
    ) {
        self.add_warning_filter_scope(edef.warning_filter.clone());
        if self.visit_enum_custom(module, enum_name, edef) {
            self.pop_warning_filter_scope();
            return;
        }
        for (vname, vdef) in edef.variants.key_cloned_iter() {
            self.visit_variant(&module, &enum_name, vname, vdef);
        }
        self.pop_warning_filter_scope();
    }

    fn visit_variant_custom(
        &mut self,
        _module: &ModuleIdent,
        _enum_name: &DatatypeName,
        _variant_name: VariantName,
        _vdef: &N::VariantDefinition,
    ) -> bool {
        false
    }

    fn visit_variant(
        &mut self,
        module: &ModuleIdent,
        enum_name: &DatatypeName,
        variant_name: VariantName,
        vdef: &N::VariantDefinition,
    ) {
        if self.visit_variant_custom(module, enum_name, variant_name, vdef) {
            return;
        }
        if Self::VISIT_TYPES {
            match &vdef.fields {
                N::VariantFields::Defined(_, fields) => {
                    for (_, _, (_, ty)) in fields {
                        self.visit_type(None, ty)
                    }
                }
                N::VariantFields::Empty => (),
            }
        }
    }

    fn visit_constant_custom(
        &mut self,
        _module: ModuleIdent,
        _constant_name: ConstantName,
        _cdef: &T::Constant,
    ) -> bool {
        false
    }

    fn visit_constant(
        &mut self,
        module: ModuleIdent,
        constant_name: ConstantName,
        cdef: &T::Constant,
    ) {
        self.add_warning_filter_scope(cdef.warning_filter.clone());
        if self.visit_constant_custom(module, constant_name, cdef) {
            self.pop_warning_filter_scope();
            return;
        }
        self.visit_exp(&cdef.value);
        self.pop_warning_filter_scope();
    }

    fn visit_function_custom(
        &mut self,
        _module: ModuleIdent,
        _function_name: FunctionName,
        _fdef: &T::Function,
    ) -> bool {
        false
    }

    fn visit_function(
        &mut self,
        module: ModuleIdent,
        function_name: FunctionName,
        fdef: &T::Function,
    ) {
        self.add_warning_filter_scope(fdef.warning_filter.clone());
        if self.visit_function_custom(module, function_name, fdef) {
            self.pop_warning_filter_scope();
            return;
        }
        if Self::VISIT_TYPES {
            fdef.signature
                .parameters
                .iter()
                .map(|(_, _, ty)| ty)
                .for_each(|ty| self.visit_type(None, ty));
            self.visit_type(None, &fdef.signature.return_type);
        }
        if let T::FunctionBody_::Defined(seq) = &fdef.body.value {
            self.visit_seq(seq);
        }
        self.pop_warning_filter_scope();
    }

    // -- TYPES --

    fn visit_type_custom(&mut self, _exp_loc: Option<Loc>, _ty: &N::Type) -> bool {
        false
    }

    /// Visit a type, including recursively. Note that this may be called manually even if
    /// `VISIT_TYPES` is set to `false`.
    #[growing_stack]
    fn visit_type(&mut self, exp_loc: Option<Loc>, ty: &N::Type) {
        if self.visit_type_custom(exp_loc, ty) {
            return;
        }
        match &ty.value {
            N::Type_::Unit => (),
            N::Type_::Ref(_, inner) => self.visit_type(exp_loc, inner),
            N::Type_::Param(_) => (),
            N::Type_::Apply(_, _, args) => args.iter().for_each(|ty| self.visit_type(exp_loc, ty)),
            N::Type_::Fun(args, ret) => {
                args.iter().for_each(|ty| self.visit_type(exp_loc, ty));
                self.visit_type(exp_loc, ret);
            }
            N::Type_::Var(_) => (),
            N::Type_::Anything => (),
            N::Type_::UnresolvedError => (),
        }
    }

    // -- USE FUNS --

    fn visit_use_funs_custom(&mut self, _use_funs: &N::UseFuns) -> bool {
        false
    }

    fn visit_use_funs(&mut self, use_funs: &N::UseFuns) {
        let _ = self.visit_use_funs_custom(use_funs);
        // Nothing to traverse in the other case
    }

    // -- SEQUENCES AND EXPRESSIONS --

    fn visit_seq(&mut self, (use_funs, seq): &T::Sequence) {
        if Self::VISIT_USE_FUNS {
            self.visit_use_funs(use_funs);
        }
        for s in seq {
            self.visit_seq_item(s);
        }
    }

    /// Custom visit for a sequence item. It will skip `visit_seq_item` if `visit_seq_item_custom`
    /// returns true.
    fn visit_seq_item_custom(&mut self, _seq_item: &T::SequenceItem) -> bool {
        false
    }

    fn visit_seq_item(&mut self, seq_item: &T::SequenceItem) {
        use T::SequenceItem_ as SI;
        if self.visit_seq_item_custom(seq_item) {
            return;
        }
        match &seq_item.value {
            SI::Seq(e) => self.visit_exp(e),
            SI::Declare(lvalues) if Self::VISIT_LVALUES => {
                self.visit_lvalue_list(&LValueKind::Bind, lvalues);
            }
            SI::Declare(_) => (),
            SI::Bind(lvalues, ty_ann, e) => {
                // visit the RHS first to better match control flow
                self.visit_exp(e);
                if Self::VISIT_LVALUES {
                    self.visit_lvalue_list(&LValueKind::Bind, lvalues);
                }
                if Self::VISIT_TYPES {
                    ty_ann
                        .iter()
                        .flatten()
                        .for_each(|ty| self.visit_type(Some(ty.loc), ty));
                }
            }
        }
    }

    /// Visit an lvalue list. Note that this may be called manually even if `VISIT_LVALUES` is set
    /// to `false`.
    fn visit_lvalue_list(&mut self, kind: &LValueKind, lvalues: &T::LValueList) {
        for lvalue in &lvalues.value {
            self.visit_lvalue(kind, lvalue);
        }
    }

    /// Custom visit for an lvalue. It will skip `visit_lvalue` if `visit_lvalue_custom` returns true.
    fn visit_lvalue_custom(&mut self, _kind: &LValueKind, _lvalue: &T::LValue) -> bool {
        false
    }

    /// Visit an lvalue, including recursively. Note that this may be called manually even if
    /// `VISIT_LVALUES` is set to `false`.
    #[growing_stack]
    fn visit_lvalue(&mut self, kind: &LValueKind, lvalue: &T::LValue) {
        if self.visit_lvalue_custom(kind, lvalue) {
            return;
        }
        match &lvalue.value {
            T::LValue_::Ignore => (),
            T::LValue_::Var {
                mut_: _,
                var: _,
                ty,
                unused_binding: _,
            } => {
                if Self::VISIT_TYPES {
                    self.visit_type(Some(lvalue.loc), ty);
                }
            }
            T::LValue_::UnpackVariant(_, _, _, tyargs, fields)
            | T::LValue_::BorrowUnpackVariant(_, _, _, _, tyargs, fields)
            | T::LValue_::Unpack(_, _, tyargs, fields)
            | T::LValue_::BorrowUnpack(_, _, _, tyargs, fields) => {
                if Self::VISIT_TYPES {
                    tyargs
                        .iter()
                        .for_each(|ty| self.visit_type(Some(lvalue.loc), ty));
                }
                for (_, _, (_, (ty, lvalue))) in fields.iter() {
                    if Self::VISIT_TYPES {
                        self.visit_type(Some(lvalue.loc), ty);
                    }
                    self.visit_lvalue(kind, lvalue);
                }
            }
        }
    }

    /// Custom visit for an expression. It will skip `visit_exp` if `visit_exp_custom` returns true.
    fn visit_exp_custom(&mut self, _exp: &T::Exp) -> bool {
        false
    }

    #[growing_stack]
    fn visit_exp(&mut self, exp: &T::Exp) {
        use T::UnannotatedExp_ as E;
        if self.visit_exp_custom(exp) {
            return;
        }
        if Self::VISIT_TYPES {
            self.visit_type(Some(exp.exp.loc), &exp.ty);
        }
        let sp!(exp_loc, uexp) = &exp.exp;
        let exp_loc = *exp_loc;
        match uexp {
            E::ModuleCall(c) => {
                if Self::VISIT_TYPES {
                    c.type_arguments
                        .iter()
                        .for_each(|ty| self.visit_type(Some(exp_loc), ty));
                    c.parameter_types
                        .iter()
                        .for_each(|ty| self.visit_type(Some(exp_loc), ty));
                }
                self.visit_exp(&c.arguments)
            }
            E::Builtin(bf, e) => {
                // visit the argument first to better match control flow
                self.visit_exp(e);
                use T::BuiltinFunction_ as BF;
                match &bf.value {
                    BF::Freeze(t) => {
                        if Self::VISIT_TYPES {
                            self.visit_type(Some(exp_loc), t)
                        }
                    }
                    BF::Assert(_) => (),
                }
            }
            E::Vector(_, _, ty, e) => {
                if Self::VISIT_TYPES {
                    self.visit_type(Some(exp_loc), ty);
                }
                self.visit_exp(e);
            }
            E::IfElse(e1, e2, e3) => {
                self.visit_exp(e1);
                self.visit_exp(e2);
                self.visit_exp(e3);
            }
            E::Match(esubject, arms) => {
                self.visit_exp(esubject);
                for sp!(_, arm) in arms.value.iter() {
                    if let Some(guard) = arm.guard.as_ref() {
                        self.visit_exp(guard)
                    }
                    self.visit_exp(&arm.rhs);
                }
            }
            E::VariantMatch(esubject, _, arms) => {
                self.visit_exp(esubject);
                for (_, earm) in arms.iter() {
                    self.visit_exp(earm);
                }
            }
            E::While(_, e1, e2) => {
                self.visit_exp(e1);
                self.visit_exp(e2);
            }
            E::Loop { body, .. } => self.visit_exp(body),
            E::NamedBlock(_, seq) => self.visit_seq(seq),
            E::Block(seq) => self.visit_seq(seq),
            E::Assign(lvalues, ty_ann, e) => {
                // visit the RHS first to better match control flow
                self.visit_exp(e);
                if Self::VISIT_LVALUES {
                    for lvalue in lvalues.value.iter() {
                        self.visit_lvalue(&LValueKind::Assign, lvalue);
                    }
                }
                if Self::VISIT_TYPES {
                    ty_ann
                        .iter()
                        .flatten()
                        .for_each(|ty| self.visit_type(Some(exp_loc), ty));
                }
            }
            E::Mutate(e1, e2) => {
                self.visit_exp(e1);
                self.visit_exp(e2);
            }
            E::Return(e) => self.visit_exp(e),
            E::Abort(e) => self.visit_exp(e),
            E::Give(_, e) => self.visit_exp(e),
            E::Dereference(e) => self.visit_exp(e),
            E::UnaryExp(_, e) => self.visit_exp(e),
            E::BinopExp(e1, _, ty, e2) => {
                if Self::VISIT_TYPES {
                    self.visit_type(Some(exp_loc), ty);
                }
                self.visit_exp(e1);
                self.visit_exp(e2);
            }
            E::Pack(_, _, tyargs, fields) | E::PackVariant(_, _, _, tyargs, fields) => {
                if Self::VISIT_TYPES {
                    tyargs
                        .iter()
                        .for_each(|ty| self.visit_type(Some(exp_loc), ty));
                }
                fields.iter().for_each(|(_, _, (_, (ty, e)))| {
                    if Self::VISIT_TYPES {
                        self.visit_type(Some(exp_loc), ty)
                    }
                    self.visit_exp(e);
                });
            }
            E::ExpList(list) => {
                for l in list {
                    match l {
                        T::ExpListItem::Single(e, ty) => {
                            self.visit_exp(e);
                            if Self::VISIT_TYPES {
                                self.visit_type(Some(exp_loc), ty)
                            }
                        }
                        T::ExpListItem::Splat(_, e, tys) => {
                            self.visit_exp(e);
                            if Self::VISIT_TYPES {
                                tys.iter().for_each(|ty| self.visit_type(Some(exp_loc), ty));
                            }
                        }
                    }
                }
            }
            E::Borrow(_, e, _) => self.visit_exp(e),
            E::TempBorrow(_, e) => self.visit_exp(e),
            E::Cast(e, ty) => {
                self.visit_exp(e);
                if Self::VISIT_TYPES {
                    self.visit_type(Some(exp_loc), ty)
                }
            }
            E::Annotate(e, ty) => {
                self.visit_exp(e);
                if Self::VISIT_TYPES {
                    self.visit_type(Some(exp_loc), ty)
                }
            }
            E::Unit { .. }
            | E::Value(_)
            | E::Move { .. }
            | E::Copy { .. }
            | E::Use(_)
            | E::Constant(..)
            | E::Continue(_)
            | E::BorrowLocal(..)
            | E::ErrorConstant { .. }
            | E::UnresolvedError => (),
        }
    }
}

impl<V: TypingVisitor + 'static> From<V> for TypingVisitorObj {
    fn from(value: V) -> Self {
        Box::new(value)
    }
}

impl<V: TypingVisitorConstructor> TypingVisitor for V {
    fn visit(&mut self, env: &mut CompilationEnv, program: &T::Program) {
        self.visit(env, program)
    }
}

//**************************************************************************************************
// Mut Vistor
//**************************************************************************************************

pub trait TypingMutVisitor {
    fn visit(&mut self, env: &mut CompilationEnv, program: &mut T::Program);
}

pub trait TypingMutVisitorConstructor {
    type Context<'a>: Sized + TypingMutVisitorContext;

    fn context<'a>(env: &'a mut CompilationEnv, program: &T::Program) -> Self::Context<'a>;

    fn visit(&mut self, env: &mut CompilationEnv, program: &mut T::Program) {
        let mut context = Self::context(env, program);
        context.visit(program);
    }
}

pub trait TypingMutVisitorContext {
    fn add_warning_filter_scope(&mut self, filter: WarningFilters);
    fn pop_warning_filter_scope(&mut self);

    /// Indicates if types should be visited during the traversal of other forms (struct and enum
    /// definitions, function signatures, expressions, etc.). This will not visit lvalue types
    /// unless VISIT_LVALUES is also enabled.
    const VISIT_TYPES: bool = false;

    /// Indicates if lvalues should be visited during the traversal of sequence forms.
    const VISIT_LVALUES: bool = false;

    /// Indicates if use_funs should be visited during the traversal.
    const VISIT_USE_FUNS: bool = false;

    /// By default, the visitor will visit all modules, and all functions and constants therein.
    /// For functions and constants, it will also visit their expressions. To change this behavior,
    /// consider enabling `VISIT_LVALUES`, VISIT_TYPES`, and `VISIT_USE_FUNS` or overwriting one of
    /// the `visit_<name>_custom` functions defined on this trait, as appropriate.
    fn visit(&mut self, program: &mut T::Program) {
        for (mident, mdef) in program.modules.key_cloned_iter_mut() {
            self.visit_module(mident, mdef);
        }
    }

    // -- MODULE DEFINITIONS --

    fn visit_module_custom(
        &mut self,
        _ident: ModuleIdent,
        _mdef: &mut T::ModuleDefinition,
    ) -> bool {
        false
    }

    fn visit_module(&mut self, ident: ModuleIdent, mdef: &mut T::ModuleDefinition) {
        self.add_warning_filter_scope(mdef.warning_filter.clone());
        if self.visit_module_custom(ident, mdef) {
            self.pop_warning_filter_scope();
            return;
        }
        for (struct_name, sdef) in mdef.structs.key_cloned_iter_mut() {
            self.visit_struct(ident, struct_name, sdef)
        }
        for (enum_name, edef) in mdef.enums.key_cloned_iter_mut() {
            self.visit_enum(ident, enum_name, edef)
        }
        for (constant_name, cdef) in mdef.constants.key_cloned_iter_mut() {
            self.visit_constant(ident, constant_name, cdef)
        }
        for (function_name, fdef) in mdef.functions.key_cloned_iter_mut() {
            self.visit_function(ident, function_name, fdef)
        }
        if Self::VISIT_USE_FUNS {
            self.visit_use_funs(&mut mdef.use_funs);
        }

        self.pop_warning_filter_scope();
    }

    // -- MODULE MEMBER DEFINITIONS --

    fn visit_struct_custom(
        &mut self,
        _module: ModuleIdent,
        _struct_name: DatatypeName,
        _sdef: &mut N::StructDefinition,
    ) -> bool {
        false
    }

    fn visit_struct(
        &mut self,
        module: ModuleIdent,
        struct_name: DatatypeName,
        sdef: &mut N::StructDefinition,
    ) {
        self.add_warning_filter_scope(sdef.warning_filter.clone());
        if self.visit_struct_custom(module, struct_name, sdef) {
            self.pop_warning_filter_scope();
            return;
        }
        if Self::VISIT_TYPES {
            match &mut sdef.fields {
                N::StructFields::Defined(_, fields) => {
                    for (_, _, (_, ty)) in fields {
                        self.visit_type(None, ty)
                    }
                }
                N::StructFields::Native(_) => (),
            }
        }
        self.pop_warning_filter_scope();
    }

    fn visit_enum_custom(
        &mut self,
        _module: ModuleIdent,
        _enum_name: DatatypeName,
        _edef: &mut N::EnumDefinition,
    ) -> bool {
        false
    }

    fn visit_enum(
        &mut self,
        module: ModuleIdent,
        enum_name: DatatypeName,
        edef: &mut N::EnumDefinition,
    ) {
        self.add_warning_filter_scope(edef.warning_filter.clone());
        if self.visit_enum_custom(module, enum_name, edef) {
            self.pop_warning_filter_scope();
            return;
        }
        for (vname, vdef) in edef.variants.key_cloned_iter_mut() {
            self.visit_variant(&module, &enum_name, vname, vdef);
        }
        self.pop_warning_filter_scope();
    }

    fn visit_variant_custom(
        &mut self,
        _module: &ModuleIdent,
        _enum_name: &DatatypeName,
        _variant_name: VariantName,
        _vdef: &mut N::VariantDefinition,
    ) -> bool {
        false
    }

    fn visit_variant(
        &mut self,
        module: &ModuleIdent,
        enum_name: &DatatypeName,
        variant_name: VariantName,
        vdef: &mut N::VariantDefinition,
    ) {
        if self.visit_variant_custom(module, enum_name, variant_name, vdef) {
            return;
        }
        if Self::VISIT_TYPES {
            match &mut vdef.fields {
                N::VariantFields::Defined(_, fields) => {
                    for (_, _, (_, ty)) in fields {
                        self.visit_type(None, ty)
                    }
                }
                N::VariantFields::Empty => (),
            }
        }
    }

    fn visit_constant_custom(
        &mut self,
        _module: ModuleIdent,
        _constant_name: ConstantName,
        _cdef: &mut T::Constant,
    ) -> bool {
        false
    }

    fn visit_constant(
        &mut self,
        module: ModuleIdent,
        constant_name: ConstantName,
        cdef: &mut T::Constant,
    ) {
        self.add_warning_filter_scope(cdef.warning_filter.clone());
        if self.visit_constant_custom(module, constant_name, cdef) {
            self.pop_warning_filter_scope();
            return;
        }
        self.visit_exp(&mut cdef.value);
        self.pop_warning_filter_scope();
    }

    fn visit_function_custom(
        &mut self,
        _module: ModuleIdent,
        _function_name: FunctionName,
        _fdef: &mut T::Function,
    ) -> bool {
        false
    }

    fn visit_function(
        &mut self,
        module: ModuleIdent,
        function_name: FunctionName,
        fdef: &mut T::Function,
    ) {
        self.add_warning_filter_scope(fdef.warning_filter.clone());
        if self.visit_function_custom(module, function_name, fdef) {
            self.pop_warning_filter_scope();
            return;
        }
        if Self::VISIT_TYPES {
            fdef.signature
                .parameters
                .iter_mut()
                .map(|(_, _, ty)| ty)
                .for_each(|ty| self.visit_type(None, ty));
            self.visit_type(None, &mut fdef.signature.return_type);
        }
        if let T::FunctionBody_::Defined(seq) = &mut fdef.body.value {
            self.visit_seq(seq);
        }
        self.pop_warning_filter_scope();
    }

    // -- TYPES --

    fn visit_type_custom(&mut self, _exp_loc: Option<Loc>, _ty: &mut N::Type) -> bool {
        false
    }

    /// Visit a type, including recursively. Note that this may be called manually even if
    /// `VISIT_TYPES` is set to `false`.
    #[growing_stack]
    fn visit_type(&mut self, exp_loc: Option<Loc>, ty: &mut N::Type) {
        if self.visit_type_custom(exp_loc, ty) {
            return;
        }
        match &mut ty.value {
            N::Type_::Unit => (),
            N::Type_::Ref(_, inner) => self.visit_type(exp_loc, inner),
            N::Type_::Param(_) => (),
            N::Type_::Apply(_, _, args) => {
                args.iter_mut().for_each(|ty| self.visit_type(exp_loc, ty))
            }
            N::Type_::Fun(args, ret) => {
                args.iter_mut().for_each(|ty| self.visit_type(exp_loc, ty));
                self.visit_type(exp_loc, ret);
            }
            N::Type_::Var(_) => (),
            N::Type_::Anything => (),
            N::Type_::UnresolvedError => (),
        }
    }

    // -- USE FUNS --

    fn visit_use_funs_custom(&mut self, _use_funs: &mut N::UseFuns) -> bool {
        false
    }

    fn visit_use_funs(&mut self, use_funs: &mut N::UseFuns) {
        let _ = self.visit_use_funs_custom(use_funs);
        // Nothing to traverse in the other case
    }

    // -- SEQUENCES AND EXPRESSIONS --

    fn visit_seq(&mut self, (use_funs, seq): &mut T::Sequence) {
        if Self::VISIT_USE_FUNS {
            self.visit_use_funs(use_funs);
        }
        for s in seq {
            self.visit_seq_item(s);
        }
    }

    /// Custom visit for a sequence item. It will skip `visit_seq_item` if `visit_seq_item_custom`
    /// returns true.
    fn visit_seq_item_custom(&mut self, _seq_item: &mut T::SequenceItem) -> bool {
        false
    }

    fn visit_seq_item(&mut self, seq_item: &mut T::SequenceItem) {
        use T::SequenceItem_ as SI;
        if self.visit_seq_item_custom(seq_item) {
            return;
        }
        match &mut seq_item.value {
            SI::Seq(e) => self.visit_exp(e),
            SI::Declare(lvalues) if Self::VISIT_LVALUES => {
                self.visit_lvalue_list(&LValueKind::Bind, lvalues);
            }
            SI::Declare(_) => (),
            SI::Bind(lvalues, ty_ann, e) => {
                // visit the RHS first to better match control flow
                self.visit_exp(e);
                if Self::VISIT_LVALUES {
                    self.visit_lvalue_list(&LValueKind::Bind, lvalues);
                }
                if Self::VISIT_TYPES {
                    ty_ann
                        .iter_mut()
                        .flatten()
                        .for_each(|ty| self.visit_type(Some(ty.loc), ty));
                }
            }
        }
    }

    /// Visit an lvalue list. Note that this may be called manually even if `VISIT_LVALUES` is set
    /// to `false`.
    fn visit_lvalue_list(&mut self, kind: &LValueKind, lvalues: &mut T::LValueList) {
        for lvalue in &mut lvalues.value {
            self.visit_lvalue(kind, lvalue);
        }
    }

    /// Custom visit for an lvalue. It will skip `visit_lvalue` if `visit_lvalue_custom` returns true.
    fn visit_lvalue_custom(&mut self, _kind: &LValueKind, _lvalue: &mut T::LValue) -> bool {
        false
    }

    /// Visit an lvalue, including recursively. Note that this may be called manually even if
    /// `VISIT_LVALUES` is set to `false`.
    #[growing_stack]
    fn visit_lvalue(&mut self, kind: &LValueKind, lvalue: &mut T::LValue) {
        if self.visit_lvalue_custom(kind, lvalue) {
            return;
        }
        match &mut lvalue.value {
            T::LValue_::Ignore => (),
            T::LValue_::Var {
                mut_: _,
                var: _,
                ty,
                unused_binding: _,
            } => {
                if Self::VISIT_TYPES {
                    self.visit_type(Some(lvalue.loc), ty);
                }
            }
            T::LValue_::UnpackVariant(_, _, _, tyargs, fields)
            | T::LValue_::BorrowUnpackVariant(_, _, _, _, tyargs, fields)
            | T::LValue_::Unpack(_, _, tyargs, fields)
            | T::LValue_::BorrowUnpack(_, _, _, tyargs, fields) => {
                if Self::VISIT_TYPES {
                    tyargs
                        .iter_mut()
                        .for_each(|ty| self.visit_type(Some(lvalue.loc), ty));
                }
                for (_, _, (_, (ty, lvalue))) in fields.iter_mut() {
                    if Self::VISIT_TYPES {
                        self.visit_type(Some(lvalue.loc), ty);
                    }
                    self.visit_lvalue(kind, lvalue);
                }
            }
        }
    }

    /// Custom visit for an expression. It will skip `visit_exp` if `visit_exp_custom` returns true.
    fn visit_exp_custom(&mut self, _exp: &mut T::Exp) -> bool {
        false
    }

    #[growing_stack]
    fn visit_exp(&mut self, exp: &mut T::Exp) {
        use T::UnannotatedExp_ as E;
        if self.visit_exp_custom(exp) {
            return;
        }
        if Self::VISIT_TYPES {
            self.visit_type(Some(exp.exp.loc), &mut exp.ty);
        }
        let sp!(exp_loc, uexp) = &mut exp.exp;
        let exp_loc = *exp_loc;
        match uexp {
            E::ModuleCall(c) => {
                if Self::VISIT_TYPES {
                    c.type_arguments
                        .iter_mut()
                        .for_each(|ty| self.visit_type(Some(exp_loc), ty));
                    c.parameter_types
                        .iter_mut()
                        .for_each(|ty| self.visit_type(Some(exp_loc), ty));
                }
                self.visit_exp(&mut c.arguments)
            }
            E::Builtin(bf, e) => {
                // visit the argument first to better match control flow
                self.visit_exp(e);
                use T::BuiltinFunction_ as BF;
                match &mut bf.value {
                    BF::Freeze(t) => {
                        if Self::VISIT_TYPES {
                            self.visit_type(Some(exp_loc), t)
                        }
                    }
                    BF::Assert(_) => (),
                }
            }
            E::Vector(_, _, ty, e) => {
                if Self::VISIT_TYPES {
                    self.visit_type(Some(exp_loc), ty);
                }
                self.visit_exp(e);
            }
            E::IfElse(e1, e2, e3) => {
                self.visit_exp(e1);
                self.visit_exp(e2);
                self.visit_exp(e3);
            }
            E::Match(esubject, arms) => {
                self.visit_exp(esubject);
                for sp!(_, arm) in arms.value.iter_mut() {
                    if let Some(guard) = arm.guard.as_mut() {
                        self.visit_exp(guard)
                    }
                    self.visit_exp(&mut arm.rhs);
                }
            }
            E::VariantMatch(esubject, _, arms) => {
                self.visit_exp(esubject);
                for (_, earm) in arms.iter_mut() {
                    self.visit_exp(earm);
                }
            }
            E::While(_, e1, e2) => {
                self.visit_exp(e1);
                self.visit_exp(e2);
            }
            E::Loop { body, .. } => self.visit_exp(body),
            E::NamedBlock(_, seq) => self.visit_seq(seq),
            E::Block(seq) => self.visit_seq(seq),
            E::Assign(lvalues, ty_ann, e) => {
                // visit the RHS first to better match control flow
                self.visit_exp(e);
                if Self::VISIT_LVALUES {
                    for lvalue in lvalues.value.iter_mut() {
                        self.visit_lvalue(&LValueKind::Assign, lvalue);
                    }
                }
                if Self::VISIT_TYPES {
                    ty_ann
                        .iter_mut()
                        .flatten()
                        .for_each(|ty| self.visit_type(Some(exp_loc), ty));
                }
            }
            E::Mutate(e1, e2) => {
                self.visit_exp(e1);
                self.visit_exp(e2);
            }
            E::Return(e) => self.visit_exp(e),
            E::Abort(e) => self.visit_exp(e),
            E::Give(_, e) => self.visit_exp(e),
            E::Dereference(e) => self.visit_exp(e),
            E::UnaryExp(_, e) => self.visit_exp(e),
            E::BinopExp(e1, _, ty, e2) => {
                if Self::VISIT_TYPES {
                    self.visit_type(Some(exp_loc), ty);
                }
                self.visit_exp(e1);
                self.visit_exp(e2);
            }
            E::Pack(_, _, tyargs, fields) | E::PackVariant(_, _, _, tyargs, fields) => {
                if Self::VISIT_TYPES {
                    tyargs
                        .iter_mut()
                        .for_each(|ty| self.visit_type(Some(exp_loc), ty));
                }
                fields.iter_mut().for_each(|(_, _, (_, (ty, e)))| {
                    if Self::VISIT_TYPES {
                        self.visit_type(Some(exp_loc), ty)
                    }
                    self.visit_exp(e);
                });
            }
            E::ExpList(list) => {
                for l in list {
                    match l {
                        T::ExpListItem::Single(e, ty) => {
                            self.visit_exp(e);
                            if Self::VISIT_TYPES {
                                self.visit_type(Some(exp_loc), ty)
                            }
                        }
                        T::ExpListItem::Splat(_, e, tys) => {
                            self.visit_exp(e);
                            if Self::VISIT_TYPES {
                                tys.iter_mut()
                                    .for_each(|ty| self.visit_type(Some(exp_loc), ty));
                            }
                        }
                    }
                }
            }
            E::Borrow(_, e, _) => self.visit_exp(e),
            E::TempBorrow(_, e) => self.visit_exp(e),
            E::Cast(e, ty) => {
                self.visit_exp(e);
                if Self::VISIT_TYPES {
                    self.visit_type(Some(exp_loc), ty)
                }
            }
            E::Annotate(e, ty) => {
                self.visit_exp(e);
                if Self::VISIT_TYPES {
                    self.visit_type(Some(exp_loc), ty)
                }
            }
            E::Unit { .. }
            | E::Value(_)
            | E::Move { .. }
            | E::Copy { .. }
            | E::Use(_)
            | E::Constant(..)
            | E::Continue(_)
            | E::BorrowLocal(..)
            | E::ErrorConstant { .. }
            | E::UnresolvedError => (),
        }
    }
}

impl<V: TypingMutVisitorConstructor> TypingMutVisitor for V {
    fn visit(&mut self, env: &mut CompilationEnv, program: &mut T::Program) {
        self.visit(env, program)
    }
}

//**************************************************************************************************
// util
//**************************************************************************************************

pub fn exp_satisfies<F>(e: &T::Exp, mut p: F) -> bool
where
    F: FnMut(&T::Exp) -> bool,
{
    exp_satisfies_(e, &mut p)
}

pub fn seq_satisfies<F>(seq: &T::Sequence, mut p: F) -> bool
where
    F: FnMut(&T::Exp) -> bool,
{
    seq_satisfies_(seq, &mut p)
}

pub fn exp_satisfies_list<F>(list: &[T::ExpListItem], mut p: F) -> bool
where
    F: FnMut(&T::Exp) -> bool,
{
    exp_list_satisfies_(list, &mut p)
}

#[growing_stack]
fn exp_satisfies_<F>(e: &T::Exp, p: &mut F) -> bool
where
    F: FnMut(&T::Exp) -> bool,
{
    use T::UnannotatedExp_ as E;
    if p(e) {
        return true;
    }
    match &e.exp.value {
        E::Unit { .. }
        | E::Value(_)
        | E::Move { .. }
        | E::Copy { .. }
        | E::Use(_)
        | E::Constant(..)
        | E::Continue(_)
        | E::BorrowLocal(..)
        | E::ErrorConstant { .. }
        | E::UnresolvedError => false,
        E::Builtin(_, e)
        | E::Vector(_, _, _, e)
        | E::Loop { body: e, .. }
        | E::Assign(_, _, e)
        | E::Return(e)
        | E::Abort(e)
        | E::Give(_, e)
        | E::Dereference(e)
        | E::UnaryExp(_, e)
        | E::Borrow(_, e, _)
        | E::TempBorrow(_, e)
        | E::Cast(e, _)
        | E::Annotate(e, _) => exp_satisfies_(e, p),
        E::While(_, e1, e2) | E::Mutate(e1, e2) | E::BinopExp(e1, _, _, e2) => {
            exp_satisfies_(e1, p) || exp_satisfies_(e2, p)
        }
        E::IfElse(e1, e2, e3) => {
            exp_satisfies_(e1, p) || exp_satisfies_(e2, p) || exp_satisfies_(e3, p)
        }
        E::ModuleCall(c) => exp_satisfies_(&c.arguments, p),
        E::Match(esubject, arms) => {
            exp_satisfies_(esubject, p)
                || arms
                    .value
                    .iter()
                    .any(|sp!(_, arm)| exp_satisfies_(&arm.rhs, p))
        }
        E::VariantMatch(esubject, _, arms) => {
            exp_satisfies_(esubject, p) || arms.iter().any(|(_, arm)| exp_satisfies_(arm, p))
        }

        E::NamedBlock(_, seq) | E::Block(seq) => seq_satisfies_(seq, p),

        E::Pack(_, _, _, fields) | E::PackVariant(_, _, _, _, fields) => fields
            .iter()
            .any(|(_, _, (_, (_, e)))| exp_satisfies_(e, p)),
        E::ExpList(list) => exp_list_satisfies_(list, p),
    }
}

fn seq_satisfies_<F>(seq: &T::Sequence, p: &mut F) -> bool
where
    F: FnMut(&T::Exp) -> bool,
{
    seq.1.iter().any(|item| match &item.value {
        T::SequenceItem_::Declare(_) => false,
        T::SequenceItem_::Seq(e) | T::SequenceItem_::Bind(_, _, e) => exp_satisfies_(e, p),
    })
}

fn exp_list_satisfies_<F>(list: &[T::ExpListItem], p: &mut F) -> bool
where
    F: FnMut(&T::Exp) -> bool,
{
    list.iter().any(|item| match item {
        T::ExpListItem::Single(e, _) | T::ExpListItem::Splat(_, e, _) => exp_satisfies_(e, p),
    })
}
