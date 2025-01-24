// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    debug_display, diag,
    diagnostics::{
        self,
        codes::{self, *},
        warning_filters::WarningFilters,
        Diagnostic, DiagnosticReporter, Diagnostics,
    },
    editions::FeatureGate,
    expansion::{
        ast::{self as E, AbilitySet, Ellipsis, ModuleIdent, Mutability, Visibility},
        name_validation::is_valid_datatype_or_constant_name as is_constant_name,
    },
    ice,
    naming::{
        ast::{self as N, BlockLabel, NominalBlockUsage, TParamID},
        fake_natives,
        syntax_methods::resolve_syntax_attributes,
    },
    parser::ast::{
        self as P, ConstantName, DatatypeName, Field, FunctionName, VariantName, MACRO_MODIFIER,
    },
    shared::{
        ide::{EllipsisMatchEntries, IDEAnnotation, IDEInfo},
        program_info::NamingProgramInfo,
        unique_map::UniqueMap,
        *,
    },
    FullyCompiledProgram,
};
use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use move_symbol_pool::Symbol;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

//**************************************************************************************************
// Resolver Types
//**************************************************************************************************

// -------------------------------------------------------------------------------------------------
// Module Definition Resolution Types
// -------------------------------------------------------------------------------------------------
// These type definitions hold the information about module members, which we can retain and reuse.
// These are used to build up the actual resolution types returned during name resolution.

#[derive(Debug, Clone)]
pub struct ResolvedModuleFunction {
    pub mident: ModuleIdent,
    pub name: FunctionName,
    pub tyarg_arity: usize,
    #[allow(unused)]
    pub arity: usize,
}

#[derive(Debug, Clone)]
pub struct ResolvedStruct {
    pub mident: ModuleIdent,
    pub name: DatatypeName,
    pub decl_loc: Loc,
    pub tyarg_arity: usize,
    pub field_info: FieldInfo,
}

#[derive(Debug, Clone)]
pub struct ResolvedEnum {
    pub mident: ModuleIdent,
    pub name: DatatypeName,
    pub decl_loc: Loc,
    pub tyarg_arity: usize,
    pub variants: UniqueMap<VariantName, ResolvedVariant>,
}

#[derive(Debug, Clone)]
pub struct ResolvedVariant {
    pub mident: ModuleIdent,
    pub enum_name: DatatypeName,
    pub tyarg_arity: usize,
    pub name: VariantName,
    pub decl_loc: Loc,
    pub field_info: FieldInfo,
}

#[derive(Debug, Clone)]
pub enum FieldInfo {
    Positional(usize),
    Named(BTreeSet<Field>),
    Empty,
}

#[derive(Debug, Clone)]
pub struct ResolvedConstant {
    pub mident: ModuleIdent,
    pub name: ConstantName,
    #[allow(unused)]
    pub decl_loc: Loc,
}

#[derive(Debug, Clone)]
pub struct ResolvedBuiltinFunction {
    pub fun: N::BuiltinFunction,
}

#[derive(Debug, Clone)]
pub enum ResolvedDatatype {
    Struct(Box<ResolvedStruct>),
    Enum(Box<ResolvedEnum>),
}

#[derive(Debug, Clone)]
pub enum ResolvedModuleMember {
    Datatype(ResolvedDatatype),
    Function(Box<ResolvedModuleFunction>),
    Constant(Box<ResolvedConstant>),
}

// -------------------------------------------------------------------------------------------------
// Resolution Result Types
// -------------------------------------------------------------------------------------------------
// These type definitions are the result of a resolution call, based on the type of thing you are
// trying to resolve.

#[derive(Debug, Clone)]
pub(super) enum ResolvedType {
    ModuleType(ResolvedDatatype),
    TParam(Loc, N::TParam),
    BuiltinType(N::BuiltinTypeName_),
    Hole, // '_' type
    Unbound,
}

#[derive(Debug, Clone)]
pub(super) enum ResolvedConstructor {
    Struct(Box<ResolvedStruct>),
    Variant(Box<ResolvedVariant>),
}

#[derive(Debug, Clone)]
pub(super) enum ResolvedCallSubject {
    Builtin(Box<ResolvedBuiltinFunction>),
    #[allow(unused)]
    Constructor(Box<ResolvedConstructor>),
    Function(Box<ResolvedModuleFunction>),
    Var(Box<N::Var>),
    Unbound,
}

#[derive(Debug, Clone)]
pub(super) enum ResolvedUseFunFunction {
    #[allow(unused)]
    Builtin(Box<ResolvedBuiltinFunction>),
    Module(Box<ResolvedModuleFunction>),
    Unbound,
}

#[derive(Debug, Clone)]
pub(super) enum ResolvedTerm {
    Constant(Box<ResolvedConstant>),
    Variant(Box<ResolvedVariant>),
    Var(Box<N::Var>),
    Unbound,
}

#[derive(Debug, Clone)]
pub(super) enum ResolvedPatternTerm {
    Constant(Box<ResolvedConstant>),
    Constructor(Box<ResolvedConstructor>),
    Unbound,
}

// -------------------------------------------------------------------------------------------------
// Block Types
// -------------------------------------------------------------------------------------------------

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
enum LoopType {
    While,
    Loop,
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
enum NominalBlockType {
    Loop(LoopType),
    Block,
    LambdaReturn,
    LambdaLoopCapture,
}

// -------------------------------------------------------------------------------------------------
// Resolution Flags
// -------------------------------------------------------------------------------------------------
// These are for determining what's gong on during resoluiton.

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
enum TypeAnnotation {
    StructField,
    VariantField,
    ConstantSignature,
    FunctionSignature,
    MacroSignature,
    Expression,
}

//************************************************
// impls
//************************************************

impl ResolvedType {
    /// Set the information for the module identifier and name to the ones provided. This allows
    /// name resolution to preserve location information and address names from the original name
    /// onto the resolved one.
    #[allow(dead_code)]
    fn set_locs(&mut self, mident: ModuleIdent, name_loc: Loc) {
        match self {
            ResolvedType::ModuleType(mtype) => mtype.set_name_info(mident, name_loc),
            ResolvedType::TParam(loc, _) => *loc = name_loc,
            ResolvedType::BuiltinType(_) => (),
            ResolvedType::Hole => (),
            ResolvedType::Unbound => (),
        }
    }
}

impl ResolvedDatatype {
    fn decl_loc(&self) -> Loc {
        match self {
            ResolvedDatatype::Struct(stype) => stype.decl_loc,
            ResolvedDatatype::Enum(etype) => etype.decl_loc,
        }
    }

    fn mident(&self) -> ModuleIdent {
        match self {
            ResolvedDatatype::Struct(stype) => stype.mident,
            ResolvedDatatype::Enum(etype) => etype.mident,
        }
    }

    fn name(&self) -> DatatypeName {
        match self {
            ResolvedDatatype::Struct(stype) => stype.name,
            ResolvedDatatype::Enum(etype) => etype.name,
        }
    }

    fn name_symbol(&self) -> Symbol {
        match self {
            ResolvedDatatype::Struct(stype) => stype.name.value(),
            ResolvedDatatype::Enum(etype) => etype.name.value(),
        }
    }

    fn datatype_kind_str(&self) -> String {
        match self {
            ResolvedDatatype::Struct(_) => "struct".to_string(),
            ResolvedDatatype::Enum(_) => "enum".to_string(),
        }
    }

    /// Set the information for the module identifier and name to the ones provided. This allows
    /// name resolution to preserve location information and address names from the original name
    /// onto the resolved one.
    fn set_name_info(&mut self, mident: ModuleIdent, name_loc: Loc) {
        match self {
            ResolvedDatatype::Struct(stype) => {
                stype.mident = mident;
                stype.name = stype.name.with_loc(name_loc);
            }
            ResolvedDatatype::Enum(etype) => {
                etype.mident = mident;
                etype.name = etype.name.with_loc(name_loc);
            }
        }
    }
}

impl FieldInfo {
    pub fn is_empty(&self) -> bool {
        matches!(self, FieldInfo::Empty)
    }

    pub fn is_positional(&self) -> bool {
        matches!(self, FieldInfo::Positional(_))
    }

    pub fn field_count(&self) -> usize {
        match self {
            FieldInfo::Positional(n) => *n,
            FieldInfo::Named(fields) => fields.len(),
            FieldInfo::Empty => 0,
        }
    }
}

impl ResolvedConstructor {
    fn type_arity(&self) -> usize {
        match self {
            ResolvedConstructor::Struct(stype) => stype.tyarg_arity,
            ResolvedConstructor::Variant(vtype) => vtype.tyarg_arity,
        }
    }

    fn field_info(&self) -> &FieldInfo {
        match self {
            ResolvedConstructor::Struct(stype) => &stype.field_info,
            ResolvedConstructor::Variant(vtype) => &vtype.field_info,
        }
    }

    fn type_name(&self) -> String {
        match self {
            ResolvedConstructor::Struct(s) => format!("{}::{}", s.mident, s.name),
            ResolvedConstructor::Variant(v) => format!("{}::{}", v.mident, v.enum_name),
        }
    }

    fn name_symbol(&self) -> Symbol {
        match self {
            ResolvedConstructor::Struct(stype) => stype.name.value(),
            ResolvedConstructor::Variant(vtype) => vtype.name.value(),
        }
    }
}

impl ResolvedVariant {
    /// Set the information for the module identifier, enum_name, and name to the ones provided.
    /// This allows name resolution to preserve location information and address names from the
    /// original name onto the resolved one.
    fn set_name_info(&mut self, mident: ModuleIdent, enum_name_loc: Loc, name_loc: Loc) {
        self.mident = mident;
        self.enum_name = self.enum_name.with_loc(enum_name_loc);
        self.name = self.name.with_loc(name_loc);
    }
}

impl ResolvedConstant {
    /// Set the information for the module identifier and name to the ones provided. This allows
    /// name resolution to preserve location information and address names from the original name
    /// onto the resolved one.
    fn set_name_info(&mut self, mident: ModuleIdent, name_loc: Loc) {
        self.mident = mident;
        self.name = self.name.with_loc(name_loc);
    }
}

impl ResolvedModuleFunction {
    /// Set the information for the module identifier and name to the ones provided. This allows
    /// name resolution to preserve location information and address names from the original name
    /// onto the resolved one.
    fn set_name_info(&mut self, mident: ModuleIdent, name_loc: Loc) {
        self.mident = mident;
        self.name = self.name.with_loc(name_loc);
    }
}

impl ResolvedModuleMember {
    /// Set the information for the module identifier and name to the ones provided. This allows
    /// name resolution to preserve location information and address names from the original name
    /// onto the resolved one.
    fn set_name_info(&mut self, mident: ModuleIdent, name_loc: Loc) {
        match self {
            ResolvedModuleMember::Datatype(member) => member.set_name_info(mident, name_loc),
            ResolvedModuleMember::Function(member) => member.set_name_info(mident, name_loc),
            ResolvedModuleMember::Constant(member) => member.set_name_info(mident, name_loc),
        }
    }

    fn mident(&self) -> ModuleIdent {
        match self {
            ResolvedModuleMember::Datatype(dt) => dt.mident(),
            ResolvedModuleMember::Function(fun) => fun.mident,
            ResolvedModuleMember::Constant(const_) => const_.mident,
        }
    }

    fn name_symbol(&self) -> Symbol {
        match self {
            ResolvedModuleMember::Datatype(dt) => dt.name_symbol(),
            ResolvedModuleMember::Function(fun) => fun.name.value(),
            ResolvedModuleMember::Constant(const_) => const_.name.value(),
        }
    }
}

impl std::fmt::Display for ResolvedModuleMember {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolvedModuleMember::Function(_) => write!(f, "function"),
            ResolvedModuleMember::Constant(_) => write!(f, "constant"),
            ResolvedModuleMember::Datatype(ty) => write!(f, "{}", ty.datatype_kind_str()),
        }
    }
}

//**************************************************************************************************
// Module Index
//**************************************************************************************************
// This indes is used for looking full paths up in name resolution.

pub type ModuleMembers = BTreeMap<ModuleIdent, BTreeMap<Symbol, ResolvedModuleMember>>;

pub fn build_member_map(
    env: &CompilationEnv,
    pre_compiled_lib: Option<Arc<FullyCompiledProgram>>,
    prog: &E::Program,
) -> ModuleMembers {
    // NB: This checks if the element is present, and doesn't replace it if so. This is congruent
    // with how top-level definitions are handled for alias resolution, where a new definition will
    // not overwrite the previous one.
    macro_rules! add_or_error {
        ($members:ident, $name:expr, $value:expr) => {{
            let name = $name.value();
            if $members.contains_key(&name) {
                assert!(env.has_errors());
            } else {
                $members.insert(name, $value);
            }
        }};
    }

    use ResolvedModuleMember as M;
    let all_modules = prog
        .modules
        .key_cloned_iter()
        .chain(pre_compiled_lib.iter().flat_map(|pre_compiled| {
            pre_compiled
                .expansion
                .modules
                .key_cloned_iter()
                .filter(|(mident, _m)| !prog.modules.contains_key(mident))
        }));
    let mut all_members = BTreeMap::new();
    for (mident, mdef) in all_modules {
        let mut members = BTreeMap::new();
        for (name, sdef) in mdef.structs.key_cloned_iter() {
            let tyarg_arity = sdef.type_parameters.len();
            let field_info = match &sdef.fields {
                E::StructFields::Positional(fields) => FieldInfo::Positional(fields.len()),
                E::StructFields::Named(f) => {
                    FieldInfo::Named(f.key_cloned_iter().map(|(k, _)| k).collect())
                }
                E::StructFields::Native(_) => FieldInfo::Empty,
            };
            let struct_def = ResolvedStruct {
                mident,
                name,
                decl_loc: name.loc(),
                tyarg_arity,
                field_info,
            };
            assert!(members
                .insert(
                    name.value(),
                    M::Datatype(ResolvedDatatype::Struct(Box::new(struct_def)))
                )
                .is_none())
        }
        for (enum_name, edef) in mdef.enums.key_cloned_iter() {
            let tyarg_arity = edef.type_parameters.len();
            let variants = edef.variants.clone().map(|name, v| {
                let field_info = match &v.fields {
                    E::VariantFields::Named(fields) => {
                        FieldInfo::Named(fields.key_cloned_iter().map(|(k, _)| k).collect())
                    }
                    E::VariantFields::Positional(tys) => FieldInfo::Positional(tys.len()),
                    E::VariantFields::Empty => FieldInfo::Empty,
                };
                ResolvedVariant {
                    mident,
                    enum_name,
                    tyarg_arity,
                    name,
                    decl_loc: v.loc,
                    field_info,
                }
            });
            let decl_loc = edef.loc;
            let enum_def = ResolvedEnum {
                mident,
                name: enum_name,
                decl_loc,
                tyarg_arity,
                variants,
            };
            add_or_error!(
                members,
                enum_name,
                M::Datatype(ResolvedDatatype::Enum(Box::new(enum_def)))
            );
        }
        // Functions and constants are shadowed by datatypes that share their names.
        for (name, fun) in mdef.functions.key_cloned_iter() {
            let tyarg_arity = fun.signature.type_parameters.len();
            let arity = fun.signature.parameters.len();
            let fun_def = ResolvedModuleFunction {
                mident,
                name,
                tyarg_arity,
                arity,
            };
            add_or_error!(members, name, M::Function(Box::new(fun_def)));
        }
        for (name, _) in mdef.constants.key_cloned_iter() {
            let const_def = ResolvedConstant {
                mident,
                name,
                decl_loc: name.loc(),
            };
            add_or_error!(members, name, M::Constant(Box::new(const_def)));
        }
        assert!(all_members.insert(mident, members).is_none());
    }
    all_members
}

//**************************************************************************************************
// Context
//**************************************************************************************************

pub(super) struct OuterContext {
    /// Nothing should ever use this directly, and should instead go through
    /// `resolve_module_access` because it preserves source location information.
    module_members: ModuleMembers,
    unscoped_types: BTreeMap<Symbol, ResolvedType>,
}

pub(super) struct Context<'outer, 'env> {
    pub env: &'env CompilationEnv,
    outer: &'outer OuterContext,
    reporter: DiagnosticReporter<'env>,
    unscoped_types: Vec<BTreeMap<Symbol, ResolvedType>>,
    current_module: ModuleIdent,
    local_scopes: Vec<BTreeMap<Symbol, u16>>,
    local_count: BTreeMap<Symbol, u16>,
    used_locals: BTreeSet<N::Var_>,
    nominal_blocks: Vec<(Option<Symbol>, BlockLabel, NominalBlockType)>,
    nominal_block_id: u16,
    /// Type parameters used in a function (they have to be cleared after processing each function).
    used_fun_tparams: BTreeSet<TParamID>,
    /// Indicates if the compiler is currently translating a function (set to true before starting
    /// to translate a function and to false after translation is over).
    translating_fun: bool,
    pub current_package: Option<Symbol>,
}

macro_rules! resolve_from_module_access {
    ($context:expr, $loc:expr, $mident:expr, $name:expr, $expected_pat:pat, $rhs:expr, $expected_kind:expr) => {{
        match $context.resolve_module_access(&Some($expected_kind), $loc, $mident, $name) {
            Some($expected_pat) => $rhs,
            Some(other) => {
                let diag =
                    make_invalid_module_member_kind_error($context, &$expected_kind, $loc, &other);
                $context.add_diag(diag);
                None
            }
            None => {
                assert!($context.env.has_errors());
                None
            }
        }
    }};
}

impl OuterContext {
    fn new(
        compilation_env: &CompilationEnv,
        pre_compiled_lib: Option<Arc<FullyCompiledProgram>>,
        prog: &E::Program,
    ) -> Self {
        use ResolvedType as RT;
        let module_members = build_member_map(compilation_env, pre_compiled_lib, prog);
        let unscoped_types = N::BuiltinTypeName_::all_names()
            .iter()
            .map(|s| {
                let b_ = RT::BuiltinType(N::BuiltinTypeName_::resolve(s.as_str()).unwrap());
                (*s, b_)
            })
            .collect();
        Self {
            module_members,
            unscoped_types,
        }
    }
}

impl<'outer, 'env> Context<'outer, 'env> {
    fn new(
        env: &'env CompilationEnv,
        outer: &'outer OuterContext,
        current_package: Option<Symbol>,
        current_module: ModuleIdent,
    ) -> Self {
        let unscoped_types = vec![outer.unscoped_types.clone()];
        let reporter = env.diagnostic_reporter_at_top_level();
        Self {
            env,
            outer,
            reporter,
            unscoped_types,
            current_module,
            local_scopes: vec![],
            local_count: BTreeMap::new(),
            nominal_blocks: vec![],
            nominal_block_id: 0,
            used_locals: BTreeSet::new(),
            used_fun_tparams: BTreeSet::new(),
            translating_fun: false,
            current_package,
        }
    }

    pub fn add_diag(&self, diag: Diagnostic) {
        self.reporter.add_diag(diag);
    }

    #[allow(unused)]
    pub fn add_diags(&self, diags: Diagnostics) {
        self.reporter.add_diags(diags);
    }

    #[allow(unused)]
    pub fn extend_ide_info(&self, info: IDEInfo) {
        self.reporter.extend_ide_info(info);
    }

    pub fn add_ide_annotation(&self, loc: Loc, info: IDEAnnotation) {
        self.reporter.add_ide_annotation(loc, info);
    }

    pub fn push_warning_filter_scope(&mut self, filters: WarningFilters) {
        self.reporter.push_warning_filter_scope(filters)
    }

    pub fn pop_warning_filter_scope(&mut self) {
        self.reporter.pop_warning_filter_scope()
    }

    pub fn check_feature(&self, package: Option<Symbol>, feature: FeatureGate, loc: Loc) -> bool {
        self.env
            .check_feature(&self.reporter, package, feature, loc)
    }

    fn valid_module(&mut self, m: &ModuleIdent) -> bool {
        let resolved = self.outer.module_members.contains_key(m);
        if !resolved {
            let diag = make_unbound_module_error(self, m.loc, m);
            self.add_diag(diag);
        }
        resolved
    }

    /// Main module access resolver. Everything for modules should go through this when possible,
    /// as it automatically preserves location information on symbols.
    fn resolve_module_access(
        &mut self,
        kind: &Option<ErrorKind>,
        loc: Loc,
        m: &ModuleIdent,
        n: &Name,
    ) -> Option<ResolvedModuleMember> {
        let Some(members) = self.outer.module_members.get(m) else {
            self.add_diag(make_unbound_module_error(self, m.loc, m));
            return None;
        };
        let result = members.get(&n.value);
        if result.is_none() {
            let diag = make_unbound_module_member_error(self, kind, loc, *m, n.value);
            self.add_diag(diag);
        }
        result.map(|inner| {
            let mut result = inner.clone();
            result.set_name_info(*m, n.loc);
            result
        })
    }

    fn resolve_module_type(
        &mut self,
        loc: Loc,
        m: &ModuleIdent,
        n: &Name,
        error_kind: ErrorKind,
    ) -> Option<Box<ResolvedDatatype>> {
        resolve_from_module_access!(
            self,
            loc,
            m,
            n,
            ResolvedModuleMember::Datatype(module_type),
            Some(Box::new(module_type)),
            error_kind
        )
    }

    fn resolve_module_function(
        &mut self,
        loc: Loc,
        m: &ModuleIdent,
        n: &Name,
    ) -> Option<Box<ResolvedModuleFunction>> {
        resolve_from_module_access!(
            self,
            loc,
            m,
            n,
            ResolvedModuleMember::Function(fun),
            Some(fun),
            ErrorKind::Function
        )
    }

    #[allow(dead_code)]
    fn resolve_module_constant(
        &mut self,
        loc: Loc,
        m: &ModuleIdent,
        n: &Name,
    ) -> Option<Box<ResolvedConstant>> {
        resolve_from_module_access!(
            self,
            loc,
            m,
            n,
            ResolvedModuleMember::Constant(const_),
            Some(const_),
            ErrorKind::Constant
        )
    }

    fn resolve_type_inner(
        &mut self,
        sp!(nloc, ma_): E::ModuleAccess,
        error_kind: ErrorKind,
    ) -> ResolvedType {
        use E::ModuleAccess_ as EN;
        match ma_ {
            EN::Name(sp!(_, n)) if n == symbol!("_") => {
                let current_package = self.current_package;
                self.check_feature(current_package, FeatureGate::TypeHoles, nloc);
                ResolvedType::Hole
            }
            EN::Name(n) => match self.resolve_unscoped_type(nloc, n) {
                ResolvedType::ModuleType(mut module_type) => {
                    module_type.set_name_info(self.current_module, nloc);
                    ResolvedType::ModuleType(module_type)
                }
                ty @ (ResolvedType::BuiltinType(_)
                | ResolvedType::TParam(_, _)
                | ResolvedType::Hole
                | ResolvedType::Unbound) => ty,
            },
            EN::ModuleAccess(m, n) | EN::Variant(sp!(_, (m, n)), _) => {
                let Some(module_type) = self.resolve_module_type(nloc, &m, &n, error_kind) else {
                    assert!(self.env.has_errors());
                    return ResolvedType::Unbound;
                };
                ResolvedType::ModuleType(*module_type)
            }
        }
    }

    pub fn resolve_type(&mut self, access: E::ModuleAccess) -> ResolvedType {
        self.resolve_type_inner(access, ErrorKind::Type)
    }

    pub fn resolve_type_for_constructor(&mut self, access: E::ModuleAccess) -> ResolvedType {
        self.resolve_type_inner(access, ErrorKind::Datatype)
    }

    fn resolve_unscoped_type(&mut self, loc: Loc, n: Name) -> ResolvedType {
        match self
            .unscoped_types
            .iter()
            .rev()
            .find_map(|unscoped_types| unscoped_types.get(&n.value))
        {
            None => {
                let diag = make_unbound_local_name_error(self, &ErrorKind::Type, loc, n);
                self.add_diag(diag);
                ResolvedType::Unbound
            }
            Some(rn) => rn.clone(),
        }
    }

    fn resolve_call_subject(&mut self, sp!(mloc, ma_): E::ModuleAccess) -> ResolvedCallSubject {
        use ErrorKind as EK;
        use E::ModuleAccess_ as EA;
        use N::BuiltinFunction_ as B;
        match ma_ {
            EA::ModuleAccess(m, n) => {
                match self.resolve_module_access(&Some(ErrorKind::Function), mloc, &m, &n) {
                    Some(ResolvedModuleMember::Function(fun)) => ResolvedCallSubject::Function(fun),
                    Some(ResolvedModuleMember::Datatype(ResolvedDatatype::Struct(struct_))) => {
                        ResolvedCallSubject::Constructor(Box::new(ResolvedConstructor::Struct(
                            struct_,
                        )))
                    }
                    Some(c @ ResolvedModuleMember::Constant(_)) => {
                        let diag =
                            make_invalid_module_member_kind_error(self, &EK::Function, mloc, &c);
                        self.add_diag(diag);
                        ResolvedCallSubject::Unbound
                    }
                    Some(e @ ResolvedModuleMember::Datatype(ResolvedDatatype::Enum(_))) => {
                        let mut diag =
                            make_invalid_module_member_kind_error(self, &EK::Function, mloc, &e);
                        diag.add_note(
                            "Enums cannot be instantiated directly. \
                                      Instead, you must instantiate a variant.",
                        );
                        self.add_diag(diag);
                        ResolvedCallSubject::Unbound
                    }
                    None => {
                        assert!(self.env.has_errors());
                        ResolvedCallSubject::Unbound
                    }
                }
            }
            EA::Name(n) if N::BuiltinFunction_::all_names().contains(&n.value) => {
                let fun_ = match n.value.as_str() {
                    B::FREEZE => B::Freeze(None),
                    B::ASSERT_MACRO => {
                        B::Assert(/* is_macro, set by caller */ None)
                    }
                    _ => {
                        let diag =
                            make_unbound_local_name_error(self, &EK::Function, n.loc, n.value);
                        self.add_diag(diag);
                        return ResolvedCallSubject::Unbound;
                    }
                };
                let fun = sp(mloc, fun_);
                let resolved = ResolvedBuiltinFunction { fun };
                ResolvedCallSubject::Builtin(Box::new(resolved))
            }
            EA::Name(n) => {
                let possibly_datatype_name = self
                    .env
                    .supports_feature(self.current_package, FeatureGate::PositionalFields)
                    && is_constant_name(&n.value);
                match self.resolve_local(
                    n.loc,
                    NameResolution::UnboundUnscopedName,
                    |n| {
                        if possibly_datatype_name {
                            format!("Unbound datatype or function '{}' in current scope", n)
                        } else {
                            format!("Unbound function '{}' in current scope", n)
                        }
                    },
                    n,
                ) {
                    None => {
                        assert!(self.env.has_errors());
                        ResolvedCallSubject::Unbound
                    }
                    Some(v) => ResolvedCallSubject::Var(Box::new(sp(n.loc, v.value))),
                }
            }
            EA::Variant(inner, _) => {
                let sloc = inner.loc;
                match self.resolve_datatype_constructor(sp(mloc, ma_), "construction") {
                    Some(variant @ ResolvedConstructor::Variant(_)) => {
                        ResolvedCallSubject::Constructor(Box::new(variant))
                    }
                    Some(ResolvedConstructor::Struct(struct_)) => {
                        self.add_diag(diag!(
                            NameResolution::NamePositionMismatch,
                            (sloc, "Invalid constructor. Expected an enum".to_string()),
                            (
                                struct_.decl_loc,
                                format!("But '{}' is an struct", struct_.name)
                            )
                        ));
                        ResolvedCallSubject::Unbound
                    }
                    None => {
                        assert!(self.env.has_errors());
                        ResolvedCallSubject::Unbound
                    }
                }
            }
        }
    }

    fn resolve_use_fun_function(
        &mut self,
        sp!(mloc, ma_): E::ModuleAccess,
    ) -> ResolvedUseFunFunction {
        use E::ModuleAccess_ as EA;
        use N::BuiltinFunction_ as B;
        match ma_ {
            EA::ModuleAccess(m, n) => match self.resolve_module_function(mloc, &m, &n) {
                None => {
                    assert!(self.env.has_errors());
                    ResolvedUseFunFunction::Unbound
                }
                Some(mut fun) => {
                    // Change the names to have the correct locations
                    fun.mident.loc = m.loc;
                    fun.name = fun.name.with_loc(n.loc);
                    ResolvedUseFunFunction::Module(fun)
                }
            },
            EA::Name(n) if N::BuiltinFunction_::all_names().contains(&n.value) => {
                let fun_ = match n.value.as_str() {
                    B::FREEZE => B::Freeze(None),
                    B::ASSERT_MACRO => {
                        B::Assert(/* is_macro, set by caller */ None)
                    }
                    _ => {
                        let diag =
                            make_unbound_local_name_error(self, &ErrorKind::Function, n.loc, n);
                        self.add_diag(diag);
                        return ResolvedUseFunFunction::Unbound;
                    }
                };
                let fun = sp(mloc, fun_);
                let resolved = ResolvedBuiltinFunction { fun };
                ResolvedUseFunFunction::Builtin(Box::new(resolved))
            }
            EA::Name(n) => {
                let diag = make_unbound_local_name_error(self, &ErrorKind::Function, n.loc, n);
                self.add_diag(diag);
                ResolvedUseFunFunction::Unbound
            }
            EA::Variant(_, _) => {
                self.add_diag(ice!((
                    mloc,
                    "Tried to resolve variant '{}' as a function in current scope"
                ),));
                ResolvedUseFunFunction::Unbound
            }
        }
    }

    fn resolve_datatype_constructor(
        &mut self,
        ma: E::ModuleAccess,
        verb: &str,
    ) -> Option<ResolvedConstructor> {
        use E::ModuleAccess_ as EN;
        match self.resolve_type_for_constructor(ma) {
            ResolvedType::Unbound => {
                assert!(self.env.has_errors());
                None
            }
            rt @ (ResolvedType::BuiltinType(_)
            | ResolvedType::TParam(_, _)
            | ResolvedType::Hole) => {
                let (rtloc, rtmsg) = match rt {
                    ResolvedType::TParam(loc, tp) => (
                        loc,
                        format!(
                            "But '{}' was declared as a type parameter here",
                            tp.user_specified_name
                        ),
                    ),
                    ResolvedType::BuiltinType(n) => {
                        (ma.loc, format!("But '{n}' is a builtin type"))
                    }
                    ResolvedType::Hole => (
                        ma.loc,
                        "The '_' is a placeholder for type inference".to_owned(),
                    ),
                    _ => unreachable!(),
                };
                let msg = if self
                    .env
                    .supports_feature(self.current_package, FeatureGate::Enums)
                {
                    format!("Invalid {}. Expected a datatype name", verb)
                } else {
                    format!("Invalid {}. Expected a struct name", verb)
                };
                self.add_diag(diag!(
                    NameResolution::NamePositionMismatch,
                    (ma.loc, msg),
                    (rtloc, rtmsg)
                ));
                None
            }
            ResolvedType::ModuleType(module_type) => {
                use ResolvedDatatype as D;
                match (&ma.value, module_type) {
                    (EN::Name(_) | EN::ModuleAccess(_, _), D::Struct(struct_type)) => {
                        Some(ResolvedConstructor::Struct(struct_type))
                    }
                    (EN::Variant(_, variant_name), D::Enum(enum_type)) => {
                        let vname = VariantName(*variant_name);
                        let Some(mut variant_info) = enum_type.variants.get(&vname).cloned() else {
                            let primary_msg = format!(
                                "Invalid {verb}. Variant '{variant_name}' is not part of this enum",
                            );
                            let decl_msg = format!("Enum '{}' is defined here", enum_type.name);
                            self.add_diag(diag!(
                                NameResolution::UnboundVariant,
                                (ma.loc, primary_msg),
                                (enum_type.decl_loc, decl_msg),
                            ));
                            return None;
                        };
                        // The `enum_type` had its locations updated by `resolve_type`.
                        variant_info.set_name_info(
                            enum_type.mident,
                            enum_type.name.loc(),
                            variant_name.loc,
                        );
                        Some(ResolvedConstructor::Variant(Box::new(variant_info)))
                    }
                    (EN::Name(_) | EN::ModuleAccess(_, _), D::Enum(enum_type)) => {
                        self.add_diag(diag!(
                            NameResolution::NamePositionMismatch,
                            (ma.loc, format!("Invalid {verb}. Expected a struct")),
                            (
                                enum_type.decl_loc,
                                format!("But '{}' is an enum", enum_type.name)
                            )
                        ));
                        None
                    }
                    (EN::Variant(sp!(sloc, _), _), D::Struct(stype)) => {
                        self.add_diag(diag!(
                            NameResolution::NamePositionMismatch,
                            (*sloc, format!("Invalid {verb}. Expected an enum")),
                            (stype.decl_loc, format!("But '{}' is an struct", stype.name))
                        ));
                        None
                    }
                }
            }
        }
    }

    fn resolve_term(&mut self, sp!(mloc, ma_): E::ModuleAccess) -> ResolvedTerm {
        match ma_ {
            E::ModuleAccess_::Name(name) if !is_constant_name(&name.value) => {
                match self.resolve_local(
                    mloc,
                    NameResolution::UnboundVariable,
                    |name| format!("Unbound variable '{name}'"),
                    name,
                ) {
                    None => {
                        debug_assert!(self.env.has_errors());
                        ResolvedTerm::Unbound
                    }
                    Some(mut nv) => {
                        nv.loc = mloc;
                        ResolvedTerm::Var(Box::new(nv))
                    }
                }
            }
            E::ModuleAccess_::Name(name) => {
                self.add_diag(diag!(
                    NameResolution::UnboundUnscopedName,
                    (mloc, format!("Unbound constant '{}'", name)),
                ));
                ResolvedTerm::Unbound
            }
            E::ModuleAccess_::ModuleAccess(m, n) => {
                match self.resolve_module_access(&Some(ErrorKind::ModuleMember), mloc, &m, &n) {
                    Some(entry) => match entry {
                        ResolvedModuleMember::Constant(const_) => ResolvedTerm::Constant(const_),
                        r @ (ResolvedModuleMember::Datatype(_)
                        | ResolvedModuleMember::Function(_)) => {
                            let mut diag = make_invalid_module_member_kind_error(
                                self,
                                &ErrorKind::Constant,
                                mloc,
                                &r,
                            );
                            match r {
                                ResolvedModuleMember::Datatype(ResolvedDatatype::Enum(etype)) => {
                                    let arity = arity_string(etype.tyarg_arity);
                                    if let Some((_, vname, ctor)) = etype.variants.iter().next() {
                                        if ctor.field_info.is_empty() {
                                            diag.add_note(format!(
                                                "Enum variants with no arguments must be \
                                            written as '{n}::{vname}{arity}'"
                                            ));
                                        } else if ctor.field_info.is_positional() {
                                            diag.add_note(format!(
                                                "Enum variants with positional arguments must be \
                                            written as '{n}::{vname}{arity}( ... )'"
                                            ));
                                        } else {
                                            diag.add_note(format!(
                                                "Enum variants with named arguments must be \
                                            written as '{n}::{vname}{arity} {{ ... }}'"
                                            ));
                                        }
                                    }
                                }
                                ResolvedModuleMember::Datatype(ResolvedDatatype::Struct(stype)) => {
                                    let arity = arity_string(stype.tyarg_arity);
                                    if stype.field_info.is_positional() {
                                        diag.add_note(format!(
                                            "Structs with positional arguments must be written as \
                                            '{n}{arity}( ... )'"
                                        ));
                                    } else {
                                        diag.add_note(format!(
                                            "Struct with named arguments must be written as \
                                            '{n}{arity} {{ ... }}'"
                                        ));
                                    }
                                }
                                ResolvedModuleMember::Function(fun) => {
                                    let arity = arity_string(fun.tyarg_arity);
                                    diag.add_note(format!(
                                        "Functions should be called as '{n}{arity}( ... )'"
                                    ));
                                }
                                ResolvedModuleMember::Constant(_) => (),
                            };
                            self.add_diag(diag);
                            ResolvedTerm::Unbound
                        }
                    },
                    None => {
                        assert!(self.env.has_errors());
                        ResolvedTerm::Unbound
                    }
                }
            }
            ma_ @ E::ModuleAccess_::Variant(_, _) => {
                self.check_feature(self.current_package, FeatureGate::Enums, mloc);
                let Some(result) = self.resolve_datatype_constructor(sp(mloc, ma_), "construction")
                else {
                    assert!(self.env.has_errors());
                    return ResolvedTerm::Unbound;
                };
                match result {
                    // TODO: this could be handed back to endure typing, similar to patterns below.
                    ResolvedConstructor::Struct(_) => {
                        assert!(self.env.has_errors());
                        ResolvedTerm::Unbound
                    }
                    ResolvedConstructor::Variant(variant) => ResolvedTerm::Variant(variant),
                }
            }
        }
    }

    fn resolve_pattern_term(&mut self, sp!(mloc, ma_): E::ModuleAccess) -> ResolvedPatternTerm {
        match ma_ {
            E::ModuleAccess_::Name(name) if !is_constant_name(&name.value) => {
                self.add_diag(ice!((mloc, "This should have become a binder")));
                ResolvedPatternTerm::Unbound
            }
            // If we have a name, try to resolve it in our module.
            E::ModuleAccess_::Name(name) => {
                let mut mident = self.current_module;
                mident.loc = mloc;
                let maccess = sp(mloc, E::ModuleAccess_::ModuleAccess(mident, name));
                self.resolve_pattern_term(maccess)
            }
            E::ModuleAccess_::ModuleAccess(m, n) => {
                match self.resolve_module_access(&Some(ErrorKind::PatternTerm), mloc, &m, &n) {
                    // carve out constants
                    Some(ResolvedModuleMember::Constant(const_)) => {
                        ResolvedPatternTerm::Constant(const_)
                    }
                    _ => match self.resolve_datatype_constructor(sp(mloc, ma_), "pattern") {
                        Some(ctor) => ResolvedPatternTerm::Constructor(Box::new(ctor)),
                        None => ResolvedPatternTerm::Unbound, // TODO: some cases here may be handled
                    },
                }
            }
            ma_ @ E::ModuleAccess_::Variant(_, _) => {
                let Some(ctor) = self.resolve_datatype_constructor(sp(mloc, ma_), "construction")
                else {
                    assert!(self.env.has_errors());
                    return ResolvedPatternTerm::Unbound;
                };
                ResolvedPatternTerm::Constructor(Box::new(ctor))
            }
        }
    }

    fn bind_type(&mut self, s: Symbol, rt: ResolvedType) {
        self.unscoped_types.last_mut().unwrap().insert(s, rt);
    }

    fn push_unscoped_types_scope(&mut self) {
        self.unscoped_types.push(BTreeMap::new())
    }

    fn pop_unscoped_types_scope(&mut self) {
        self.unscoped_types.pop().unwrap();
    }

    fn new_local_scope(&mut self) {
        self.local_scopes.push(BTreeMap::new());
    }

    fn close_local_scope(&mut self) {
        self.local_scopes.pop();
    }

    fn declare_local(&mut self, is_parameter: bool, sp!(vloc, name): Name) -> N::Var {
        let default = if is_parameter { 0 } else { 1 };
        let id = *self
            .local_count
            .entry(name)
            .and_modify(|c| *c += 1)
            .or_insert(default);
        self.local_scopes.last_mut().unwrap().insert(name, id);
        // all locals start at color zero
        // they will be incremented when substituted for macros
        let nvar_ = N::Var_ { name, id, color: 0 };
        sp(vloc, nvar_)
    }

    fn resolve_local<S: ToString>(
        &mut self,
        loc: Loc,
        code: diagnostics::codes::NameResolution,
        variable_msg: impl FnOnce(Symbol) -> S,
        sp!(vloc, name): Name,
    ) -> Option<N::Var> {
        let id_opt = self
            .local_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(&name).copied());
        match id_opt {
            None => {
                let msg = variable_msg(name);
                self.add_diag(diag!(code, (loc, msg)));
                None
            }
            Some(id) => {
                // all locals start at color zero
                // they will be incremented when substituted for macros
                let nvar_ = N::Var_ { name, id, color: 0 };
                self.used_locals.insert(nvar_);
                Some(sp(vloc, nvar_))
            }
        }
    }

    // Like resolve_local, but with an ICE on failure because these are precomputed and should
    // always exist. This also does not mark usage, as this is ostensibly the binding form.
    // This is so that we can walk through or-patterns and reuse the bindings for both sides.
    fn resolve_pattern_binder(&mut self, loc: Loc, sp!(vloc, name): Name) -> Option<N::Var> {
        let id_opt = self.local_scopes.last().unwrap().get(&name).copied();
        match id_opt {
            None => {
                let msg = format!("Failed to resolve pattern binder {}", name);
                self.add_diag(ice!((loc, msg)));
                None
            }
            Some(id) => {
                let nvar_ = N::Var_ { name, id, color: 0 };
                Some(sp(vloc, nvar_))
            }
        }
    }

    fn enter_nominal_block(
        &mut self,
        loc: Loc,
        name: Option<P::BlockLabel>,
        name_type: NominalBlockType,
    ) {
        debug_assert!(
            self.nominal_blocks.len() < 100,
            "Nominal block list exceeded 100."
        );
        let id = self.nominal_block_id;
        self.nominal_block_id += 1;
        let name = name.map(|n| n.value());
        let block_label = block_label(loc, name, id);
        self.nominal_blocks.push((name, block_label, name_type));
    }

    fn current_loop(&mut self, loc: Loc, usage: NominalBlockUsage) -> Option<BlockLabel> {
        let Some((_name, label, name_type)) =
            self.nominal_blocks.iter().rev().find(|(_, _, name_type)| {
                matches!(
                    name_type,
                    NominalBlockType::Loop(_) | NominalBlockType::LambdaLoopCapture
                )
            })
        else {
            let msg = format!(
                "Invalid usage of '{usage}'. \
                '{usage}' can only be used inside a loop body or lambda",
            );
            self.add_diag(diag!(TypeSafety::InvalidLoopControl, (loc, msg)));
            return None;
        };
        if *name_type == NominalBlockType::LambdaLoopCapture {
            // lambdas capture break/continue even though it is not yet supported
            let msg =
                format!("Invalid '{usage}'. This usage is not yet supported for lambdas or macros");
            let mut diag = diag!(
                TypeSafety::InvalidLoopControl,
                (loc, msg),
                (label.label.loc, "Inside this lambda")
            );
            // suggest adding a label to the loop
            let most_recent_loop_opt =
                self.nominal_blocks
                    .iter()
                    .rev()
                    .find_map(|(name, label, name_type)| {
                        if let NominalBlockType::Loop(loop_type) = name_type {
                            Some((name, label, *loop_type))
                        } else {
                            None
                        }
                    });
            if let Some((name, loop_label, loop_type)) = most_recent_loop_opt {
                let msg = if let Some(loop_label) = name {
                    format!(
                        "To '{usage}' to this loop, specify the label, \
                        e.g. `{usage} '{loop_label}`",
                    )
                } else {
                    format!(
                        "To '{usage}' to this loop, add a label, \
                        e.g. `'label: {loop_type}` and `{usage} 'label`",
                    )
                };
                diag.add_secondary_label((loop_label.label.loc, msg));
            }
            self.add_diag(diag);
            return None;
        }
        Some(*label)
    }

    fn current_continue(&mut self, loc: Loc) -> Option<BlockLabel> {
        self.current_loop(loc, NominalBlockUsage::Continue)
    }

    fn current_break(&mut self, loc: Loc) -> Option<BlockLabel> {
        self.current_loop(loc, NominalBlockUsage::Break)
    }

    fn current_return(&self, _loc: Loc) -> Option<BlockLabel> {
        self.nominal_blocks
            .iter()
            .rev()
            .find(|(_, _, name_type)| matches!(name_type, NominalBlockType::LambdaReturn))
            .map(|(_, label, _)| *label)
    }

    fn resolve_nominal_label(
        &mut self,
        usage: NominalBlockUsage,
        label: P::BlockLabel,
    ) -> Option<BlockLabel> {
        let loc = label.loc();
        let name = label.value();
        let label_opt = self
            .nominal_blocks
            .iter()
            .rev()
            .find(|(block_name, _, _)| block_name.is_some_and(|n| n == name))
            .map(|(_, label, block_type)| (label, block_type));
        if let Some((label, block_type)) = label_opt {
            let block_type = *block_type;
            if block_type.is_acceptable_usage(usage) {
                Some(*label)
            } else {
                let msg = format!("Invalid usage of '{usage}' with a {block_type} block label",);
                let mut diag = diag!(NameResolution::InvalidLabel, (loc, msg));
                diag.add_note(match block_type {
                    NominalBlockType::Loop(_) => {
                        "Loop labels may only be used with 'break' and 'continue', \
                        not 'return'"
                    }
                    NominalBlockType::Block => {
                        "Named block labels may only be used with 'return', \
                        not 'break' or 'continue'."
                    }
                    NominalBlockType::LambdaReturn | NominalBlockType::LambdaLoopCapture => {
                        "Lambda block labels may only be used with 'return' or 'break', \
                        not 'continue'."
                    }
                });
                self.add_diag(diag);
                None
            }
        } else {
            let msg = format!("Invalid {usage}. Unbound label '{name}");
            self.add_diag(diag!(NameResolution::UnboundLabel, (loc, msg)));
            None
        }
    }

    fn exit_nominal_block(&mut self) -> (BlockLabel, NominalBlockType) {
        let (_name, label, name_type) = self.nominal_blocks.pop().unwrap();
        (label, name_type)
    }
}

fn block_label(loc: Loc, name: Option<Symbol>, id: u16) -> BlockLabel {
    let is_implicit = name.is_none();
    let name = name.unwrap_or(BlockLabel::IMPLICIT_LABEL_SYMBOL);
    let var_ = N::Var_ { name, id, color: 0 };
    let label = sp(loc, var_);
    BlockLabel { label, is_implicit }
}

impl NominalBlockType {
    // loops can have break or continue
    // blocks can have return
    // lambdas can have return or break
    fn is_acceptable_usage(self, usage: NominalBlockUsage) -> bool {
        match (self, usage) {
            (NominalBlockType::Loop(_), NominalBlockUsage::Break)
            | (NominalBlockType::Loop(_), NominalBlockUsage::Continue)
            | (NominalBlockType::Block, NominalBlockUsage::Return)
            | (NominalBlockType::LambdaReturn, NominalBlockUsage::Return)
            | (NominalBlockType::LambdaLoopCapture, NominalBlockUsage::Break)
            | (NominalBlockType::LambdaLoopCapture, NominalBlockUsage::Continue) => true,
            (NominalBlockType::Loop(_), NominalBlockUsage::Return)
            | (NominalBlockType::Block, NominalBlockUsage::Break)
            | (NominalBlockType::Block, NominalBlockUsage::Continue)
            | (NominalBlockType::LambdaReturn, NominalBlockUsage::Break)
            | (NominalBlockType::LambdaReturn, NominalBlockUsage::Continue)
            | (NominalBlockType::LambdaLoopCapture, NominalBlockUsage::Return) => false,
        }
    }
}

impl std::fmt::Display for LoopType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoopType::While => write!(f, "while"),
            LoopType::Loop => write!(f, "loop"),
        }
    }
}

impl std::fmt::Display for NominalBlockType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                NominalBlockType::Loop(_) => "loop",
                NominalBlockType::Block => "named",
                NominalBlockType::LambdaReturn | NominalBlockType::LambdaLoopCapture => "lambda",
            }
        )
    }
}

//**************************************************************************************************
// Error Reporting
//**************************************************************************************************
// TODO: use this through more of the file when possible.

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
enum ErrorKind {
    Type,
    Constructor,
    Constant,
    Function,
    Term,
    Variable,
    Module,
    ModuleMember,
    ApplyNamed,
    PatternTerm,
    Datatype,
}

impl ErrorKind {
    fn kind_name(&self, context: &Context, single_name: bool) -> &str {
        match self {
            ErrorKind::Type => "type",
            ErrorKind::Constructor
                if !context
                    .env
                    .supports_feature(context.current_package, FeatureGate::Enums) =>
            {
                "struct"
            }
            ErrorKind::Constructor => "struct or enum variant",
            ErrorKind::Datatype
                if !context
                    .env
                    .supports_feature(context.current_package, FeatureGate::Enums) =>
            {
                "struct"
            }
            ErrorKind::Datatype => "struct or enum",
            ErrorKind::Function => "function",
            ErrorKind::Constant if single_name => "local or constant",
            ErrorKind::Constant => "constant",
            ErrorKind::ModuleMember => "module member",
            ErrorKind::Variable => "variable",
            ErrorKind::Term if single_name => "variable or constant",
            ErrorKind::Term => "local, constant, or enum variant (of no arguments)",
            ErrorKind::Module => "module",
            ErrorKind::ApplyNamed => "struct or enum variant",
            ErrorKind::PatternTerm if single_name => "variable or constant",
            ErrorKind::PatternTerm => "loca, constant, or enum variant (of no arguments)",
        }
    }

    fn unbound_error_code(&self, single_name: bool) -> codes::NameResolution {
        use codes::NameResolution as NR;
        match self {
            ErrorKind::Type => NR::UnboundType,
            ErrorKind::Constructor if single_name => NR::UnboundUnscopedName,
            ErrorKind::Constructor => NR::UnboundModuleMember,
            ErrorKind::Datatype if single_name => NR::UnboundUnscopedName,
            ErrorKind::Datatype => NR::UnboundModuleMember,
            ErrorKind::Function if single_name => NR::UnboundUnscopedName,
            ErrorKind::Function => NR::UnboundModuleMember,
            ErrorKind::Constant if single_name => NR::InvalidPosition,
            ErrorKind::Constant => NR::UnboundModuleMember,
            ErrorKind::Term if single_name => NR::UnboundVariable,
            ErrorKind::Term => NR::UnboundModuleMember,
            ErrorKind::ModuleMember => NR::UnboundModuleMember,
            ErrorKind::Variable => NR::UnboundVariable,
            ErrorKind::Module => NR::UnboundModule,
            ErrorKind::ApplyNamed => NR::UnboundModuleMember,
            ErrorKind::PatternTerm if single_name => NR::UnboundVariable,
            ErrorKind::PatternTerm => NR::UnboundModuleMember,
        }
    }

    fn invalid_form_error_code(&self, _single_name: bool) -> codes::NameResolution {
        use codes::NameResolution as NR;
        match self {
            ErrorKind::PatternTerm => NR::InvalidPattern,
            ErrorKind::Type
            | ErrorKind::Constructor
            | ErrorKind::Constant
            | ErrorKind::Function
            | ErrorKind::Term
            | ErrorKind::Variable
            | ErrorKind::Module
            | ErrorKind::ModuleMember
            | ErrorKind::ApplyNamed
            | ErrorKind::Datatype => NR::InvalidPosition,
        }
    }
}

fn make_unbound_name_error_msg(
    context: &Context,
    expected: &ErrorKind,
    is_single_name: bool,
    name: impl std::fmt::Display,
) -> String {
    format!(
        "Unbound {} '{name}'",
        expected.kind_name(context, is_single_name)
    )
}

fn make_unbound_module_error(
    context: &Context,
    loc: Loc,
    mident: impl std::fmt::Display,
) -> Diagnostic {
    let msg = make_unbound_name_error_msg(context, &ErrorKind::Module, true, mident);
    diag!(
        ErrorKind::Module.unbound_error_code(/* is_single_name */ true),
        (loc, msg)
    )
}

fn make_unbound_local_name_error(
    context: &Context,
    expected: &ErrorKind,
    loc: Loc,
    name: impl std::fmt::Display,
) -> Diagnostic {
    let base_msg = make_unbound_name_error_msg(context, expected, true, name);
    let msg = format!("{base_msg} in current scope");
    diag!(
        expected.unbound_error_code(/* is_single_name */ true),
        (loc, msg)
    )
}

fn make_unbound_module_member_error(
    context: &Context,
    expected: &Option<ErrorKind>,
    loc: Loc,
    mident: ModuleIdent,
    name: impl std::fmt::Display,
) -> Diagnostic {
    let expected = expected.as_ref().unwrap_or(&ErrorKind::ModuleMember);
    let same_module = context.current_module == mident;
    let (prefix, postfix) = if same_module {
        ("", " in current scope".to_string())
    } else {
        ("Invalid module access. ", format!(" in module '{mident}'"))
    };
    let msg = format!(
        "{prefix}{}{postfix}",
        make_unbound_name_error_msg(context, expected, same_module, name)
    );
    diag!(
        expected.unbound_error_code(/* is_single_name */ false),
        (loc, msg)
    )
}

fn make_invalid_module_member_kind_error(
    context: &Context,
    expected: &ErrorKind,
    loc: Loc,
    actual: &ResolvedModuleMember,
) -> Diagnostic {
    let mident = actual.mident();
    let same_module = context.current_module == mident;
    let (prefix, postfix) = if same_module {
        ("", " in current scope".to_string())
    } else {
        ("Invalid module access. ", format!(" in module '{mident}'"))
    };
    let msg = format!(
        "{prefix}Expected a {}, but found {} '{}'{postfix}",
        expected.kind_name(context, same_module),
        actual,
        actual.name_symbol()
    );
    diag!(
        expected.invalid_form_error_code(/* is_single_name */ false),
        (loc, msg)
    )
}

#[allow(dead_code)]
fn arity_string(arity: usize) -> &'static str {
    match arity {
        0 => "",
        1 => "<T>",
        _ => "<T0,...>",
    }
}

//**************************************************************************************************
// Entry
//**************************************************************************************************

pub fn program(
    compilation_env: &CompilationEnv,
    pre_compiled_lib: Option<Arc<FullyCompiledProgram>>,
    prog: E::Program,
) -> N::Program {
    let outer_context = OuterContext::new(compilation_env, pre_compiled_lib.clone(), &prog);
    let E::Program {
        warning_filters_table,
        modules: emodules,
    } = prog;
    let modules = modules(compilation_env, &outer_context, emodules);
    let mut inner = N::Program_ { modules };
    let mut info = NamingProgramInfo::new(pre_compiled_lib, &inner);
    super::resolve_use_funs::program(compilation_env, &mut info, &mut inner);
    N::Program {
        info,
        warning_filters_table,
        inner,
    }
}

fn modules(
    env: &CompilationEnv,
    outer: &OuterContext,
    modules: UniqueMap<ModuleIdent, E::ModuleDefinition>,
) -> UniqueMap<ModuleIdent, N::ModuleDefinition> {
    modules.map(|ident, mdef| module(env, outer, ident, mdef))
}

fn module(
    env: &CompilationEnv,
    outer: &OuterContext,
    ident: ModuleIdent,
    mdef: E::ModuleDefinition,
) -> N::ModuleDefinition {
    let E::ModuleDefinition {
        doc,
        loc,
        warning_filter,
        package_name,
        attributes,
        target_kind,
        use_funs: euse_funs,
        friends: efriends,
        structs: estructs,
        enums: eenums,
        functions: efunctions,
        constants: econstants,
    } = mdef;
    let context = &mut Context::new(env, outer, package_name, ident);
    context.push_warning_filter_scope(warning_filter);
    let mut use_funs = use_funs(context, euse_funs);
    let mut syntax_methods = N::SyntaxMethods::new();
    let friends = efriends.filter_map(|mident, f| friend(context, mident, f));
    let struct_names = estructs
        .key_cloned_iter()
        .map(|(k, _)| k)
        .collect::<BTreeSet<_>>();
    let enum_names = eenums
        .key_cloned_iter()
        .map(|(k, _)| k)
        .collect::<BTreeSet<_>>();
    let enum_struct_intersection = enum_names
        .intersection(&struct_names)
        .collect::<BTreeSet<_>>();
    let structs = estructs.map(|name, s| {
        context.push_unscoped_types_scope();
        let s = struct_def(context, name, s);
        context.pop_unscoped_types_scope();
        s
    });
    // simply for compilation to continue in the presence of errors, we remove the duplicates
    let enums = eenums.filter_map(|name, e| {
        context.push_unscoped_types_scope();
        let result = if enum_struct_intersection.contains(&name) {
            None
        } else {
            Some(enum_def(context, name, e))
        };
        context.pop_unscoped_types_scope();
        result
    });
    let functions = efunctions.map(|name, f| {
        context.push_unscoped_types_scope();
        let f = function(context, &mut syntax_methods, ident, name, f);
        context.pop_unscoped_types_scope();
        f
    });
    let constants = econstants.map(|name, c| {
        context.push_unscoped_types_scope();
        let c = constant(context, name, c);
        context.pop_unscoped_types_scope();
        c
    });
    // Silence unused use fun warnings if a module has macros.
    // For public macros, the macro will pull in the use fun, and we will which case we will be
    //   unable to tell if it is used or not
    // For private macros, we duplicate the scope of the module and when resolving the method
    //   fail to mark the outer scope as used (instead we only mark the modules scope cloned
    //   into the macro)
    // TODO we should approximate this by just checking for the name, regardless of the type
    let has_macro = functions.iter().any(|(_, _, f)| f.macro_.is_some());
    if has_macro {
        mark_all_use_funs_as_used(&mut use_funs);
    }
    context.pop_warning_filter_scope();
    N::ModuleDefinition {
        doc,
        loc,
        warning_filter,
        package_name,
        attributes,
        target_kind,
        use_funs,
        syntax_methods,
        friends,
        structs,
        enums,
        constants,
        functions,
    }
}

//**************************************************************************************************
// Use Funs
//**************************************************************************************************

fn use_funs(context: &mut Context, eufs: E::UseFuns) -> N::UseFuns {
    let E::UseFuns {
        explicit: eexplicit,
        implicit: eimplicit,
    } = eufs;
    let mut resolved = N::ResolvedUseFuns::new();
    let resolved_vec: Vec<_> = eexplicit
        .into_iter()
        .flat_map(|e| explicit_use_fun(context, e))
        .collect();
    for (tn, method, nuf) in resolved_vec {
        let methods = resolved.entry(tn.clone()).or_default();
        let nuf_loc = nuf.loc;
        if let Err((_, prev)) = methods.add(method, nuf) {
            let msg = format!("Duplicate 'use fun' for '{}.{}'", tn, method);
            context.add_diag(diag!(
                Declarations::DuplicateItem,
                (nuf_loc, msg),
                (prev, "Previously declared here"),
            ))
        }
    }
    N::UseFuns {
        color: 0, // used for macro substitution
        resolved,
        implicit_candidates: eimplicit,
    }
}

fn explicit_use_fun(
    context: &mut Context,
    e: E::ExplicitUseFun,
) -> Option<(N::TypeName, Name, N::UseFun)> {
    let E::ExplicitUseFun {
        doc,
        loc,
        attributes,
        is_public,
        function,
        ty,
        method,
    } = e;
    let m_f_opt = match context.resolve_use_fun_function(function) {
        ResolvedUseFunFunction::Module(mf) => {
            let ResolvedModuleFunction {
                mident,
                name,
                tyarg_arity: _,
                arity: _,
            } = *mf;
            Some((mident, name))
        }
        ResolvedUseFunFunction::Builtin(_) => {
            let msg = "Invalid 'use fun'. Cannot use a builtin function as a method";
            context.add_diag(diag!(Declarations::InvalidUseFun, (loc, msg)));
            None
        }
        ResolvedUseFunFunction::Unbound => {
            assert!(context.env.has_errors());
            None
        }
    };
    let ty_loc = ty.loc;

    // check use fun scope first to avoid some borrow pain nastiness
    let tn_opt = match context.resolve_type(ty) {
        rt @ (ResolvedType::ModuleType(_) | ResolvedType::BuiltinType(_))
            if check_use_fun_scope(context, &loc, &is_public, &rt) =>
        {
            rt
        }
        ResolvedType::ModuleType(_) | ResolvedType::BuiltinType(_) => {
            assert!(context.env.has_errors());
            ResolvedType::Unbound
        }
        ty @ (ResolvedType::TParam(_, _) | ResolvedType::Hole | ResolvedType::Unbound) => ty,
    };
    let tn_opt = match tn_opt {
        ResolvedType::BuiltinType(bt_) => Some(N::TypeName_::Builtin(sp(ty.loc, bt_))),
        ResolvedType::ModuleType(mt) => Some(N::TypeName_::ModuleType(mt.mident(), mt.name())),
        ResolvedType::Unbound => {
            assert!(context.env.has_errors());
            None
        }
        ResolvedType::Hole => {
            let msg = "Invalid 'use fun'. Cannot associate a method with an inferred type";
            let tmsg = "The '_' type is a placeholder for type inference";
            context.add_diag(diag!(
                Declarations::InvalidUseFun,
                (loc, msg),
                (ty_loc, tmsg)
            ));
            None
        }
        ResolvedType::TParam(tloc, tp) => {
            let msg = "Invalid 'use fun'. Cannot associate a method with a type parameter";
            let tmsg = format!(
                "But '{}' was declared as a type parameter here",
                tp.user_specified_name
            );
            context.add_diag(diag!(
                Declarations::InvalidUseFun,
                (loc, msg,),
                (tloc, tmsg)
            ));
            None
        }
    };
    let tn_ = tn_opt?;
    let tn = sp(ty.loc, tn_);
    let target_function = m_f_opt?;
    let use_fun = N::UseFun {
        doc,
        loc,
        attributes,
        is_public,
        tname: tn.clone(),
        target_function,
        kind: N::UseFunKind::Explicit,
        used: is_public.is_some(), // suppress unused warning for public use funs
    };
    Some((tn, method, use_fun))
}

fn check_use_fun_scope(
    context: &mut Context,
    use_fun_loc: &Loc,
    is_public: &Option<Loc>,
    rtype: &ResolvedType,
) -> bool {
    let Some(pub_loc) = is_public else {
        return true;
    };
    let current_module = context.current_module;
    let Err(def_loc_opt) = use_fun_module_defines(context, use_fun_loc, &current_module, rtype)
    else {
        return true;
    };

    let msg = "Invalid 'use fun'. Cannot publicly associate a function with a \
        type defined in another module";
    let pub_msg = format!(
        "Declared '{}' here. Consider removing to make a local 'use fun' instead",
        Visibility::PUBLIC
    );
    let mut diag = diag!(
        Declarations::InvalidUseFun,
        (*use_fun_loc, msg),
        (*pub_loc, pub_msg)
    );
    if let Some(def_loc) = def_loc_opt {
        diag.add_secondary_label((def_loc, "Type defined in another module here"));
    }
    context.add_diag(diag);
    false
}

fn use_fun_module_defines(
    context: &mut Context,
    use_fun_loc: &Loc,
    specified: &ModuleIdent,
    rtype: &ResolvedType,
) -> Result<(), Option<Loc>> {
    match rtype {
        ResolvedType::ModuleType(mtype) => {
            if specified == &mtype.mident() {
                Ok(())
            } else {
                Err(Some(mtype.decl_loc()))
            }
        }
        ResolvedType::BuiltinType(b_) => {
            let definer_opt = context.env.primitive_definer(*b_);
            match definer_opt {
                None => Err(None),
                Some(d) => {
                    if d == specified {
                        Ok(())
                    } else {
                        Err(Some(d.loc))
                    }
                }
            }
        }
        ResolvedType::TParam(_, _) | ResolvedType::Hole | ResolvedType::Unbound => {
            context.add_diag(ice!((
                *use_fun_loc,
                "Tried to validate use fun for invalid type"
            )));
            Ok(())
        }
    }
}

fn mark_all_use_funs_as_used(use_funs: &mut N::UseFuns) {
    let N::UseFuns {
        color: _,
        resolved,
        implicit_candidates,
    } = use_funs;
    for methods in resolved.values_mut() {
        for (_, _, uf) in methods {
            uf.used = true;
        }
    }
    for (_, _, uf) in implicit_candidates {
        match &mut uf.kind {
            E::ImplicitUseFunKind::UseAlias { used } => *used = true,
            E::ImplicitUseFunKind::FunctionDeclaration => (),
        }
    }
}

//**************************************************************************************************
// Friends
//**************************************************************************************************

fn friend(context: &mut Context, mident: ModuleIdent, friend: E::Friend) -> Option<E::Friend> {
    let current_mident = &context.current_module;
    if mident.value.address != current_mident.value.address {
        // NOTE: in alignment with the bytecode verifier, this constraint is a policy decision
        // rather than a technical requirement. The compiler, VM, and bytecode verifier DO NOT
        // rely on the assumption that friend modules must reside within the same account address.
        let msg = "Cannot declare modules out of the current address as a friend";
        context.add_diag(diag!(
            Declarations::InvalidFriendDeclaration,
            (friend.loc, "Invalid friend declaration"),
            (mident.loc, msg),
        ));
        None
    } else if &mident == current_mident {
        context.add_diag(diag!(
            Declarations::InvalidFriendDeclaration,
            (friend.loc, "Invalid friend declaration"),
            (mident.loc, "Cannot declare the module itself as a friend"),
        ));
        None
    } else if context.valid_module(&mident) {
        Some(friend)
    } else {
        assert!(context.env.has_errors());
        None
    }
}

//**************************************************************************************************
// Functions
//**************************************************************************************************

fn function(
    context: &mut Context,
    syntax_methods: &mut N::SyntaxMethods,
    module: ModuleIdent,
    name: FunctionName,
    ef: E::Function,
) -> N::Function {
    let E::Function {
        doc,
        warning_filter,
        index,
        attributes,
        loc,
        visibility,
        macro_,
        entry,
        signature,
        body,
    } = ef;
    assert!(!context.translating_fun);
    assert!(context.local_count.is_empty());
    assert!(context.local_scopes.is_empty());
    assert!(context.nominal_block_id == 0);
    assert!(context.used_fun_tparams.is_empty());
    assert!(context.used_locals.is_empty());
    context.push_warning_filter_scope(warning_filter);
    context.local_scopes = vec![BTreeMap::new()];
    context.local_count = BTreeMap::new();
    context.translating_fun = true;
    let case = if macro_.is_some() {
        TypeAnnotation::MacroSignature
    } else {
        TypeAnnotation::FunctionSignature
    };
    let signature = function_signature(context, case, signature);
    let body = function_body(context, body);

    if !matches!(body.value, N::FunctionBody_::Native) {
        for tparam in &signature.type_parameters {
            if !context.used_fun_tparams.contains(&tparam.id) {
                let sp!(loc, n) = tparam.user_specified_name;
                let msg = format!("Unused type parameter '{}'.", n);
                context.add_diag(diag!(UnusedItem::FunTypeParam, (loc, msg)))
            }
        }
    }

    let mut f = N::Function {
        doc,
        warning_filter,
        index,
        attributes,
        loc,
        visibility,
        macro_,
        entry,
        signature,
        body,
    };
    resolve_syntax_attributes(context, syntax_methods, &module, &name, &f);
    fake_natives::function(&context.reporter, module, name, &f);
    let used_locals = std::mem::take(&mut context.used_locals);
    remove_unused_bindings_function(context, &used_locals, &mut f);
    context.local_count = BTreeMap::new();
    context.local_scopes = vec![];
    context.nominal_block_id = 0;
    context.used_fun_tparams = BTreeSet::new();
    context.used_locals = BTreeSet::new();
    context.pop_warning_filter_scope();
    context.translating_fun = false;
    f
}

fn function_signature(
    context: &mut Context,
    case: TypeAnnotation,
    sig: E::FunctionSignature,
) -> N::FunctionSignature {
    let type_parameters = fun_type_parameters(context, sig.type_parameters);

    let mut declared = UniqueMap::new();
    let parameters = sig
        .parameters
        .into_iter()
        .map(|(mut mut_, param, param_ty)| {
            let is_underscore = param.is_underscore();
            if is_underscore {
                check_mut_underscore(context, Some(mut_));
                mut_ = Mutability::Imm;
            };
            if param.is_syntax_identifier() {
                if let Mutability::Mut(mutloc) = mut_ {
                    let msg = format!(
                        "Invalid 'mut' parameter. \
                        '{}' parameters cannot be declared as mutable",
                        MACRO_MODIFIER
                    );
                    let mut diag = diag!(NameResolution::InvalidMacroParameter, (mutloc, msg));
                    diag.add_note(ASSIGN_SYNTAX_IDENTIFIER_NOTE);
                    context.add_diag(diag);
                    mut_ = Mutability::Imm;
                }
            }
            if let Err((param, prev_loc)) = declared.add(param, ()) {
                if !is_underscore {
                    let msg = format!("Duplicate parameter with name '{}'", param);
                    context.add_diag(diag!(
                        Declarations::DuplicateItem,
                        (param.loc(), msg),
                        (prev_loc, "Previously declared here"),
                    ))
                }
            }
            let is_parameter = true;
            let nparam = context.declare_local(is_parameter, param.0);
            let nparam_ty = type_(context, case, param_ty);
            (mut_, nparam, nparam_ty)
        })
        .collect();
    let return_type = type_(context, case, sig.return_type);
    N::FunctionSignature {
        type_parameters,
        parameters,
        return_type,
    }
}

fn function_body(context: &mut Context, sp!(loc, b_): E::FunctionBody) -> N::FunctionBody {
    match b_ {
        E::FunctionBody_::Native => sp(loc, N::FunctionBody_::Native),
        E::FunctionBody_::Defined(es) => sp(loc, N::FunctionBody_::Defined(sequence(context, es))),
    }
}

const ASSIGN_SYNTAX_IDENTIFIER_NOTE: &str = "'macro' parameters are substituted without \
    being evaluated. There is no local variable to assign to";

//**************************************************************************************************
// Structs
//**************************************************************************************************

fn struct_def(
    context: &mut Context,
    _name: DatatypeName,
    sdef: E::StructDefinition,
) -> N::StructDefinition {
    let E::StructDefinition {
        doc,
        warning_filter,
        index,
        attributes,
        loc,
        abilities,
        type_parameters,
        fields,
    } = sdef;
    context.push_warning_filter_scope(warning_filter);
    let type_parameters = datatype_type_parameters(context, type_parameters);
    let fields = struct_fields(context, fields);
    context.pop_warning_filter_scope();
    N::StructDefinition {
        doc,
        warning_filter,
        index,
        loc,
        attributes,
        abilities,
        type_parameters,
        fields,
    }
}

fn positional_field_name(loc: Loc, idx: usize) -> Field {
    Field::add_loc(loc, format!("{idx}").into())
}

fn struct_fields(context: &mut Context, efields: E::StructFields) -> N::StructFields {
    match efields {
        E::StructFields::Native(loc) => N::StructFields::Native(loc),
        E::StructFields::Named(em) => N::StructFields::Defined(
            false,
            em.map(|_f, (idx, (doc, t))| {
                (idx, (doc, type_(context, TypeAnnotation::StructField, t)))
            }),
        ),
        E::StructFields::Positional(tys) => {
            let fields = tys
                .into_iter()
                .map(|(doc, ty)| (doc, type_(context, TypeAnnotation::StructField, ty)))
                .enumerate()
                .map(|(idx, (doc, ty))| {
                    let field_name = positional_field_name(ty.loc, idx);
                    (field_name, (idx, (doc, ty)))
                });
            N::StructFields::Defined(true, UniqueMap::maybe_from_iter(fields).unwrap())
        }
    }
}

//**************************************************************************************************
// Enums
//**************************************************************************************************

fn enum_def(
    context: &mut Context,
    _name: DatatypeName,
    edef: E::EnumDefinition,
) -> N::EnumDefinition {
    let E::EnumDefinition {
        doc,
        warning_filter,
        index,
        attributes,
        loc,
        abilities,
        type_parameters,
        variants,
    } = edef;
    context.push_warning_filter_scope(warning_filter);
    let type_parameters = datatype_type_parameters(context, type_parameters);
    let variants = enum_variants(context, variants);
    context.pop_warning_filter_scope();
    N::EnumDefinition {
        doc,
        warning_filter,
        index,
        loc,
        attributes,
        abilities,
        type_parameters,
        variants,
    }
}

fn enum_variants(
    context: &mut Context,
    evariants: UniqueMap<VariantName, E::VariantDefinition>,
) -> UniqueMap<VariantName, N::VariantDefinition> {
    let variants = evariants
        .into_iter()
        .map(|(key, defn)| (key, variant_def(context, defn)));
    UniqueMap::maybe_from_iter(variants).unwrap()
}

fn variant_def(context: &mut Context, variant: E::VariantDefinition) -> N::VariantDefinition {
    let E::VariantDefinition {
        doc,
        loc,
        index,
        fields,
    } = variant;

    N::VariantDefinition {
        doc,
        index,
        loc,
        fields: variant_fields(context, fields),
    }
}

fn variant_fields(context: &mut Context, efields: E::VariantFields) -> N::VariantFields {
    match efields {
        E::VariantFields::Empty => N::VariantFields::Empty,
        E::VariantFields::Named(em) => N::VariantFields::Defined(
            false,
            em.map(|_f, (idx, (doc, t))| {
                (idx, (doc, type_(context, TypeAnnotation::VariantField, t)))
            }),
        ),
        E::VariantFields::Positional(tys) => {
            let fields = tys
                .into_iter()
                .map(|(doc, ty)| (doc, type_(context, TypeAnnotation::VariantField, ty)))
                .enumerate()
                .map(|(idx, (doc, ty))| {
                    let field_name = positional_field_name(ty.loc, idx);
                    (field_name, (idx, (doc, ty)))
                });
            N::VariantFields::Defined(true, UniqueMap::maybe_from_iter(fields).unwrap())
        }
    }
}

//**************************************************************************************************
// Constants
//**************************************************************************************************

fn constant(context: &mut Context, _name: ConstantName, econstant: E::Constant) -> N::Constant {
    let E::Constant {
        doc,
        warning_filter,
        index,
        attributes,
        loc,
        signature: esignature,
        value: evalue,
    } = econstant;
    assert!(context.local_scopes.is_empty());
    assert!(context.local_count.is_empty());
    assert!(context.used_locals.is_empty());
    context.push_warning_filter_scope(warning_filter);
    context.local_scopes = vec![BTreeMap::new()];
    let signature = type_(context, TypeAnnotation::ConstantSignature, esignature);
    let value = *exp(context, Box::new(evalue));
    context.local_scopes = vec![];
    context.local_count = BTreeMap::new();
    context.used_locals = BTreeSet::new();
    context.nominal_block_id = 0;
    context.pop_warning_filter_scope();
    N::Constant {
        doc,
        warning_filter,
        index,
        attributes,
        loc,
        signature,
        value,
    }
}

//**************************************************************************************************
// Types
//**************************************************************************************************

fn fun_type_parameters(
    context: &mut Context,
    type_parameters: Vec<(Name, AbilitySet)>,
) -> Vec<N::TParam> {
    let mut unique_tparams = UniqueMap::new();
    type_parameters
        .into_iter()
        .map(|(name, abilities)| type_parameter(context, &mut unique_tparams, name, abilities))
        .collect()
}

fn datatype_type_parameters(
    context: &mut Context,
    type_parameters: Vec<E::DatatypeTypeParameter>,
) -> Vec<N::DatatypeTypeParameter> {
    let mut unique_tparams = UniqueMap::new();
    type_parameters
        .into_iter()
        .map(|param| {
            let is_phantom = param.is_phantom;
            let param = type_parameter(context, &mut unique_tparams, param.name, param.constraints);
            N::DatatypeTypeParameter { param, is_phantom }
        })
        .collect()
}

fn type_parameter(
    context: &mut Context,
    unique_tparams: &mut UniqueMap<Name, ()>,
    name: Name,
    abilities: AbilitySet,
) -> N::TParam {
    let id = N::TParamID::next();
    let user_specified_name = name;
    let tp = N::TParam {
        id,
        user_specified_name,
        abilities,
    };
    let loc = name.loc;
    context.bind_type(name.value, ResolvedType::TParam(loc, tp.clone()));
    if let Err((name, old_loc)) = unique_tparams.add(name, ()) {
        let msg = format!("Duplicate type parameter declared with name '{}'", name);
        context.add_diag(diag!(
            Declarations::DuplicateItem,
            (loc, msg),
            (old_loc, "Type parameter previously defined here"),
        ))
    }
    tp
}

fn types_opt(
    context: &mut Context,
    case: TypeAnnotation,
    tys: Option<Vec<E::Type>>,
) -> Option<Vec<N::Type>> {
    tys.map(|tys| types(context, case, tys))
}

fn types(context: &mut Context, case: TypeAnnotation, tys: Vec<E::Type>) -> Vec<N::Type> {
    tys.into_iter().map(|t| type_(context, case, t)).collect()
}

fn type_(context: &mut Context, case: TypeAnnotation, sp!(loc, ety_): E::Type) -> N::Type {
    use ResolvedType as RT;
    use E::Type_ as ET;
    use N::{TypeName_ as NN, Type_ as NT};
    let ty_ = match ety_ {
        ET::Unit => NT::Unit,
        ET::Multiple(tys) => NT::multiple_(
            loc,
            tys.into_iter().map(|t| type_(context, case, t)).collect(),
        ),
        ET::Ref(mut_, inner) => NT::Ref(mut_, Box::new(type_(context, case, *inner))),
        ET::UnresolvedError => {
            assert!(context.env.has_errors());
            NT::UnresolvedError
        }
        ET::Apply(ma, tys) => {
            let original_loc = ma.loc;
            match context.resolve_type(ma) {
                RT::Unbound => {
                    assert!(context.env.has_errors());
                    NT::UnresolvedError
                }
                RT::Hole => {
                    let case_str_opt = match case {
                        TypeAnnotation::StructField => {
                            Some(("Struct fields", " or consider adding a new type parameter"))
                        }
                        TypeAnnotation::VariantField => Some((
                            "Enum variant fields",
                            " or consider adding a new type parameter",
                        )),
                        TypeAnnotation::ConstantSignature => Some(("Constants", "")),
                        TypeAnnotation::FunctionSignature => {
                            Some(("Functions", " or consider adding a new type parameter"))
                        }
                        TypeAnnotation::MacroSignature | TypeAnnotation::Expression => None,
                    };
                    if let Some((case_str, help_str)) = case_str_opt {
                        let msg = format!(
                                  "Invalid usage of a placeholder for type inference '_'. \
                          {case_str} require fully specified types. Replace '_' with a specific type{help_str}"
                              );
                        let mut diag = diag!(NameResolution::InvalidTypeAnnotation, (loc, msg));
                        if let TypeAnnotation::FunctionSignature = case {
                            diag.add_note("Only 'macro' functions can use '_' in their signatures");
                        }
                        context.add_diag(diag);
                        NT::UnresolvedError
                    } else {
                        // replaced with a type variable during type instantiation
                        NT::Anything
                    }
                }
                RT::BuiltinType(bn_) => {
                    let name_f = || format!("{}", &bn_);
                    let arity = bn_.tparam_constraints(loc).len();
                    let tys = types(context, case, tys);
                    let tys = check_type_instantiation_arity(context, loc, name_f, tys, arity);
                    NT::builtin_(sp(ma.loc, bn_), tys)
                }
                RT::TParam(_, tp) => {
                    if !tys.is_empty() {
                        context.add_diag(diag!(
                            NameResolution::NamePositionMismatch,
                            (loc, "Generic type parameters cannot take type arguments"),
                        ));
                        NT::UnresolvedError
                    } else {
                        if context.translating_fun {
                            context.used_fun_tparams.insert(tp.id);
                        }
                        NT::Param(tp)
                    }
                }
                RT::ModuleType(mt) => {
                    let (tn, arity) = match mt {
                        ResolvedDatatype::Struct(stype) => {
                            let tn = sp(original_loc, NN::ModuleType(stype.mident, stype.name));
                            let arity = stype.tyarg_arity;
                            (tn, arity)
                        }
                        ResolvedDatatype::Enum(etype) => {
                            let tn = sp(original_loc, NN::ModuleType(etype.mident, etype.name));
                            let arity = etype.tyarg_arity;
                            (tn, arity)
                        }
                    };
                    let tys = types(context, case, tys);
                    let name_f = || format!("{}", tn);
                    let tys = check_type_instantiation_arity(context, loc, name_f, tys, arity);
                    NT::Apply(None, tn, tys)
                }
            }
        }
        ET::Fun(tys, ty) => {
            let tys = types(context, case, tys);
            let ty = Box::new(type_(context, case, *ty));
            NT::Fun(tys, ty)
        }
    };
    sp(loc, ty_)
}

fn types_opt_with_instantiation_arity_check<F: FnOnce() -> String>(
    context: &mut Context,
    case: TypeAnnotation,
    loc: Loc,
    name_f: F,
    ty_args: Option<Vec<E::Type>>,
    arity: usize,
) -> Option<Vec<N::Type>> {
    ty_args.map(|etys| {
        let tys = types(context, case, etys);
        check_type_instantiation_arity(context, loc, name_f, tys, arity)
    })
}

fn check_type_instantiation_arity<F: FnOnce() -> String>(
    context: &mut Context,
    loc: Loc,
    name_f: F,
    mut ty_args: Vec<N::Type>,
    arity: usize,
) -> Vec<N::Type> {
    let args_len = ty_args.len();
    if args_len != arity {
        let diag_code = if args_len > arity {
            NameResolution::TooManyTypeArguments
        } else {
            NameResolution::TooFewTypeArguments
        };
        let msg = format!(
            "Invalid instantiation of '{}'. Expected {} type argument(s) but got {}",
            name_f(),
            arity,
            args_len
        );
        context.add_diag(diag!(diag_code, (loc, msg)));
    }

    while ty_args.len() > arity {
        ty_args.pop();
    }

    while ty_args.len() < arity {
        ty_args.push(sp(loc, N::Type_::UnresolvedError))
    }

    ty_args
}

//**************************************************************************************************
// Exp
//**************************************************************************************************

#[growing_stack]
fn sequence(context: &mut Context, (euse_funs, seq): E::Sequence) -> N::Sequence {
    context.new_local_scope();
    let nuse_funs = use_funs(context, euse_funs);
    let nseq = seq.into_iter().map(|s| sequence_item(context, s)).collect();
    context.close_local_scope();
    (nuse_funs, nseq)
}

#[growing_stack]
fn sequence_item(context: &mut Context, sp!(loc, ns_): E::SequenceItem) -> N::SequenceItem {
    use E::SequenceItem_ as ES;
    use N::SequenceItem_ as NS;

    let s_ = match ns_ {
        ES::Seq(e) => NS::Seq(exp(context, e)),
        ES::Declare(b, ty_opt) => {
            let bind_opt = bind_list(context, b);
            let tys = ty_opt.map(|t| type_(context, TypeAnnotation::Expression, t));
            match bind_opt {
                None => {
                    assert!(context.env.has_errors());
                    NS::Seq(Box::new(sp(loc, N::Exp_::UnresolvedError)))
                }
                Some(bind) => NS::Declare(bind, tys),
            }
        }
        ES::Bind(b, e) => {
            let e = exp(context, e);
            let bind_opt = bind_list(context, b);
            match bind_opt {
                None => {
                    assert!(context.env.has_errors());
                    NS::Seq(Box::new(sp(loc, N::Exp_::UnresolvedError)))
                }
                Some(bind) => NS::Bind(bind, e),
            }
        }
    };
    sp(loc, s_)
}

fn call_args(context: &mut Context, sp!(loc, es): Spanned<Vec<E::Exp>>) -> Spanned<Vec<N::Exp>> {
    sp(loc, exps(context, es))
}

fn exps(context: &mut Context, es: Vec<E::Exp>) -> Vec<N::Exp> {
    es.into_iter().map(|e| *exp(context, Box::new(e))).collect()
}

#[growing_stack]
fn exp(context: &mut Context, e: Box<E::Exp>) -> Box<N::Exp> {
    use E::Exp_ as EE;
    use N::Exp_ as NE;
    let sp!(eloc, e_) = *e;
    let ne_ = match e_ {
        EE::Unit { trailing } => NE::Unit { trailing },
        EE::Value(val) => NE::Value(val),
        EE::Name(ma, tyargs_opt) => {
            match context.resolve_term(ma) {
                ResolvedTerm::Constant(const_) => {
                    exp_types_opt_with_arity_check(
                        context,
                        ma.loc,
                        || "Constants cannot take type arguments".to_string(),
                        eloc,
                        tyargs_opt,
                        0,
                    );
                    N::Exp_::Constant(const_.mident, const_.name)
                }
                ResolvedTerm::Variant(vtype) => {
                    let tys_opt = types_opt_with_instantiation_arity_check(
                        context,
                        TypeAnnotation::Expression,
                        eloc,
                        || format!("{}::{}", &vtype.mident, &vtype.enum_name),
                        tyargs_opt,
                        vtype.tyarg_arity,
                    );
                    check_constructor_form(
                        context,
                        eloc,
                        ConstructorForm::None,
                        "instantiation",
                        &ResolvedConstructor::Variant(vtype.clone()),
                    );
                    NE::PackVariant(
                        vtype.mident,
                        vtype.enum_name,
                        vtype.name,
                        tys_opt,
                        UniqueMap::new(),
                    )
                }
                ResolvedTerm::Var(var) => {
                    exp_types_opt_with_arity_check(
                        context,
                        ma.loc,
                        || "Variables cannot take type arguments".to_string(),
                        eloc,
                        tyargs_opt,
                        0,
                    );
                    NE::Var(*var)
                }
                ResolvedTerm::Unbound => {
                    // Just for the errors
                    types_opt(context, TypeAnnotation::Expression, tyargs_opt);
                    assert!(context.env.has_errors());
                    NE::UnresolvedError
                }
            }
        }

        EE::IfElse(eb, et, ef_opt) => NE::IfElse(
            exp(context, eb),
            exp(context, et),
            ef_opt.map(|ef| exp(context, ef)),
        ),
        // EE::Match(esubject, sp!(_aloc, arms)) if arms.is_empty() => {
        //     exp(context, esubject); // for error effect
        //     let msg = "Invalid 'match' form. 'match' must have at least one arm";
        //     context
        //         .env
        //         .add_diag(diag!(Syntax::InvalidMatch, (eloc, msg)));
        //     NE::UnresolvedError
        // }
        EE::Match(esubject, sp!(aloc, arms)) => NE::Match(
            exp(context, esubject),
            sp(
                aloc,
                arms.into_iter()
                    .map(|arm| match_arm(context, arm))
                    .collect(),
            ),
        ),
        EE::While(name_opt, eb, el) => {
            let cond = exp(context, eb);
            context.enter_nominal_block(eloc, name_opt, NominalBlockType::Loop(LoopType::While));
            let body = exp(context, el);
            let (label, name_type) = context.exit_nominal_block();
            assert_eq!(name_type, NominalBlockType::Loop(LoopType::While));
            NE::While(label, cond, body)
        }
        EE::Loop(name_opt, el) => {
            context.enter_nominal_block(eloc, name_opt, NominalBlockType::Loop(LoopType::Loop));
            let body = exp(context, el);
            let (label, name_type) = context.exit_nominal_block();
            assert_eq!(name_type, NominalBlockType::Loop(LoopType::Loop));
            NE::Loop(label, body)
        }
        EE::Block(Some(name), eseq) => {
            context.enter_nominal_block(eloc, Some(name), NominalBlockType::Block);
            let seq = sequence(context, eseq);
            let (label, name_type) = context.exit_nominal_block();
            assert_eq!(name_type, NominalBlockType::Block);
            NE::Block(N::Block {
                name: Some(label),
                from_macro_argument: None,
                seq,
            })
        }
        EE::Block(None, eseq) => NE::Block(N::Block {
            name: None,
            from_macro_argument: None,
            seq: sequence(context, eseq),
        }),
        EE::Lambda(elambda_binds, ety_opt, body) => {
            context.new_local_scope();
            let nlambda_binds_opt = lambda_bind_list(context, elambda_binds);
            let return_type = ety_opt.map(|t| type_(context, TypeAnnotation::Expression, t));
            context.enter_nominal_block(eloc, None, NominalBlockType::LambdaLoopCapture);
            context.enter_nominal_block(eloc, None, NominalBlockType::LambdaReturn);
            let body = exp(context, body);
            context.close_local_scope();
            let (return_label, return_name_type) = context.exit_nominal_block();
            assert_eq!(return_name_type, NominalBlockType::LambdaReturn);
            let (_, loop_name_type) = context.exit_nominal_block();
            assert_eq!(loop_name_type, NominalBlockType::LambdaLoopCapture);
            match nlambda_binds_opt {
                None => {
                    assert!(context.env.has_errors());
                    N::Exp_::UnresolvedError
                }
                Some(parameters) => NE::Lambda(N::Lambda {
                    parameters,
                    return_type,
                    return_label,
                    use_fun_color: 0, // used in macro expansion
                    body,
                }),
            }
        }

        EE::Assign(a, e) => {
            let na_opt = assign_list(context, a);
            let ne = exp(context, e);
            match na_opt {
                None => {
                    assert!(context.env.has_errors());
                    NE::UnresolvedError
                }
                Some(na) => NE::Assign(na, ne),
            }
        }
        EE::FieldMutate(edotted, er) => {
            let ndot_opt = dotted(context, *edotted);
            let ner = exp(context, er);
            match ndot_opt {
                None => {
                    assert!(context.env.has_errors());
                    NE::UnresolvedError
                }
                Some(ndot) => NE::FieldMutate(ndot, ner),
            }
        }
        EE::Mutate(el, er) => {
            let nel = exp(context, el);
            let ner = exp(context, er);
            NE::Mutate(nel, ner)
        }

        EE::Abort(Some(es)) => NE::Abort(exp(context, es)),
        EE::Abort(None) => {
            context.check_feature(context.current_package, FeatureGate::CleverAssertions, eloc);
            let abort_const_expr = sp(
                eloc,
                N::Exp_::ErrorConstant {
                    line_number_loc: eloc,
                },
            );
            NE::Abort(Box::new(abort_const_expr))
        }
        EE::Return(Some(block_name), es) => {
            let out_rhs = exp(context, es);
            context
                .resolve_nominal_label(NominalBlockUsage::Return, block_name)
                .map(|name| NE::Give(NominalBlockUsage::Return, name, out_rhs))
                .unwrap_or_else(|| NE::UnresolvedError)
        }
        EE::Return(None, es) => {
            let out_rhs = exp(context, es);
            if let Some(return_name) = context.current_return(eloc) {
                NE::Give(NominalBlockUsage::Return, return_name, out_rhs)
            } else {
                NE::Return(out_rhs)
            }
        }
        EE::Break(name_opt, rhs) => {
            let out_rhs = exp(context, rhs);
            if let Some(loop_name) = name_opt {
                context
                    .resolve_nominal_label(NominalBlockUsage::Break, loop_name)
                    .map(|name| NE::Give(NominalBlockUsage::Break, name, out_rhs))
                    .unwrap_or_else(|| NE::UnresolvedError)
            } else {
                context
                    .current_break(eloc)
                    .map(|name| NE::Give(NominalBlockUsage::Break, name, out_rhs))
                    .unwrap_or_else(|| NE::UnresolvedError)
            }
        }
        EE::Continue(name_opt) => {
            if let Some(loop_name) = name_opt {
                context
                    .resolve_nominal_label(NominalBlockUsage::Continue, loop_name)
                    .map(NE::Continue)
                    .unwrap_or_else(|| NE::UnresolvedError)
            } else {
                context
                    .current_continue(eloc)
                    .map(NE::Continue)
                    .unwrap_or_else(|| NE::UnresolvedError)
            }
        }

        EE::Dereference(e) => NE::Dereference(exp(context, e)),
        EE::UnaryExp(uop, e) => NE::UnaryExp(uop, exp(context, e)),

        e_ @ EE::BinopExp(..) => {
            process_binops!(
                (P::BinOp, Loc),
                Box<N::Exp>,
                Box::new(sp(eloc, e_)),
                e,
                *e,
                sp!(loc, EE::BinopExp(lhs, op, rhs)) => { (lhs, (op, loc), rhs) },
                { exp(context, e) },
                value_stack,
                (bop, loc) => {
                    let el = value_stack.pop().expect("ICE binop naming issue");
                    let er = value_stack.pop().expect("ICE binop naming issue");
                    Box::new(sp(loc, NE::BinopExp(el, bop, er)))
                }
            )
            .value
        }

        EE::Pack(ma, etys_opt, efields) => {
            // Process fields for errors either way.
            let fields = efields.map(|_, (idx, e)| (idx, *exp(context, Box::new(e))));
            let Some(ctor) = context.resolve_datatype_constructor(ma, "construction") else {
                assert!(context.env.has_errors());
                return Box::new(sp(eloc, NE::UnresolvedError));
            };
            let tys_opt = types_opt_with_instantiation_arity_check(
                context,
                TypeAnnotation::Expression,
                eloc,
                || ctor.type_name(),
                etys_opt,
                ctor.type_arity(),
            );
            check_constructor_form(
                context,
                eloc,
                ConstructorForm::Braces,
                "instantiation",
                &ctor,
            );
            match ctor {
                ResolvedConstructor::Struct(stype) => {
                    NE::Pack(stype.mident, stype.name, tys_opt, fields)
                }
                ResolvedConstructor::Variant(vtype) => {
                    NE::PackVariant(vtype.mident, vtype.enum_name, vtype.name, tys_opt, fields)
                }
            }
        }
        EE::ExpList(es) => {
            assert!(es.len() > 1);
            NE::ExpList(exps(context, es))
        }

        EE::ExpDotted(case, edot) => match dotted(context, *edot) {
            None => {
                assert!(context.env.has_errors());
                NE::UnresolvedError
            }
            Some(ndot) => NE::ExpDotted(case, ndot),
        },

        EE::Cast(e, t) => NE::Cast(
            exp(context, e),
            type_(context, TypeAnnotation::Expression, t),
        ),
        EE::Annotate(e, t) => NE::Annotate(
            exp(context, e),
            type_(context, TypeAnnotation::Expression, t),
        ),

        EE::Call(ma, is_macro, tys_opt, rhs) => {
            resolve_call(context, eloc, ma, is_macro, tys_opt, rhs)
        }
        EE::MethodCall(edot, dot_loc, n, is_macro, tys_opt, rhs) => match dotted(context, *edot) {
            None => {
                assert!(context.env.has_errors());
                NE::UnresolvedError
            }
            Some(d) => {
                let ty_args = tys_opt.map(|tys| types(context, TypeAnnotation::Expression, tys));
                let nes = call_args(context, rhs);
                if is_macro.is_some() {
                    context.check_feature(context.current_package, FeatureGate::MacroFuns, eloc);
                }
                NE::MethodCall(d, dot_loc, n, is_macro, ty_args, nes)
            }
        },
        EE::Vector(vec_loc, tys_opt, rhs) => {
            let nes = call_args(context, rhs);
            let ty_opt = exp_types_opt_with_arity_check(
                context,
                vec_loc,
                || "Invalid 'vector' instantation".to_string(),
                eloc,
                tys_opt,
                1,
            )
            .map(|mut v| {
                assert!(v.len() == 1);
                v.pop().unwrap()
            });
            NE::Vector(vec_loc, ty_opt, nes)
        }

        EE::UnresolvedError => {
            assert!(context.env.has_errors());
            NE::UnresolvedError
        }
        // `Name` matches name variants only allowed in specs (we handle the allowed ones above)
        e @ (EE::Index(..) | EE::Quant(..)) => {
            let mut diag = ice!((
                eloc,
                "ICE compiler should not have parsed this form as a specification"
            ));
            diag.add_note(format!("Compiler parsed: {}", debug_display!(e)));
            context.add_diag(diag);
            NE::UnresolvedError
        }
    };
    Box::new(sp(eloc, ne_))
}

fn dotted(context: &mut Context, edot: E::ExpDotted) -> Option<N::ExpDotted> {
    let sp!(loc, edot_) = edot;
    let nedot_ = match edot_ {
        E::ExpDotted_::Exp(e) => {
            let ne = exp(context, e);
            match &ne.value {
                N::Exp_::UnresolvedError => return None,
                N::Exp_::Var(n) if n.value.is_syntax_identifier() => {
                    let mut diag = diag!(
                        NameResolution::NamePositionMismatch,
                        (n.loc, "Macro parameters are not allowed to appear in paths")
                    );
                    diag.add_note(format!(
                        "To use a macro parameter as a value in a path expression, first bind \
                            it to a local variable, e.g. 'let {0} = ${0};'",
                        &n.value.name.to_string()[1..]
                    ));
                    diag.add_note(
                        "Macro parameters are always treated as value expressions, and are not \
                        modified by path operations.\n\
                        Path operations include 'move', 'copy', '&', '&mut', and field references",
                    );
                    context.add_diag(diag);
                    N::ExpDotted_::Exp(Box::new(sp(ne.loc, N::Exp_::UnresolvedError)))
                }
                _ => N::ExpDotted_::Exp(ne),
            }
        }
        E::ExpDotted_::Dot(d, loc, f) => {
            N::ExpDotted_::Dot(Box::new(dotted(context, *d)?), loc, Field(f))
        }
        E::ExpDotted_::DotUnresolved(loc, d) => {
            N::ExpDotted_::DotAutocomplete(loc, Box::new(dotted(context, *d)?))
        }
        E::ExpDotted_::Index(inner, args) => {
            let args = call_args(context, args);
            let inner = Box::new(dotted(context, *inner)?);
            N::ExpDotted_::Index(inner, args)
        }
    };
    Some(sp(loc, nedot_))
}

enum ConstructorForm {
    None,
    Parens,
    Braces,
}

fn check_constructor_form(
    context: &mut Context,
    loc: Loc,
    form: ConstructorForm,
    position: &str,
    ty: &ResolvedConstructor,
) {
    use ConstructorForm as CF;
    use ResolvedConstructor as RC;
    const NAMED_UPCASE: &str = "Named";
    const NAMED: &str = "named";
    const EMPTY_UPCASE: &str = "Empty";
    const EMPTY: &str = "empty";
    const POSNL_UPCASE: &str = "Positional";
    const POSNL: &str = "positional";

    fn defn_loc_error(name: &str) -> String {
        format!("'{name}' is declared here")
    }

    macro_rules! invalid_inst_msg {
        ($ty:expr, $upcase:ident, $kind:ident) => {{
            let ty = $ty;
            let upcase = $upcase;
            let kind = $kind;
            format!(
                "Invalid {ty} {position}. \
                {upcase} {ty} declarations require {kind} {position}s"
            )
        }};
    }
    macro_rules! posnl_note {
        () => {
            format!("{POSNL_UPCASE} {position}s take arguments using '()'")
        };
    }
    macro_rules! named_note {
        () => {
            format!("{NAMED_UPCASE} {position}s take arguments using '{{ }}'")
        };
    }

    let name = ty.name_symbol();
    match ty {
        RC::Struct(stype) => match form {
            CF::None => {
                let (form_upcase, form) = if stype.field_info.is_positional() {
                    (POSNL_UPCASE, POSNL)
                } else {
                    (NAMED_UPCASE, NAMED)
                };
                let msg = invalid_inst_msg!("struct", form_upcase, form);
                let mut diag = diag!(
                    NameResolution::PositionalCallMismatch,
                    (loc, msg),
                    (stype.decl_loc, defn_loc_error(&name)),
                );
                if stype.field_info.is_positional() {
                    diag.add_note(posnl_note!());
                } else {
                    diag.add_note(named_note!());
                }
                context.add_diag(diag);
            }
            CF::Parens if stype.field_info.is_positional() => (),
            CF::Parens => {
                let msg = invalid_inst_msg!("struct", NAMED_UPCASE, NAMED);
                let diag = diag!(
                    NameResolution::PositionalCallMismatch,
                    (loc, &msg),
                    (stype.decl_loc, defn_loc_error(&name)),
                );
                context.add_diag(diag);
            }
            CF::Braces if stype.field_info.is_positional() => {
                let msg = invalid_inst_msg!("struct", POSNL_UPCASE, POSNL);
                let diag = diag!(
                    NameResolution::PositionalCallMismatch,
                    (loc, &msg),
                    (stype.decl_loc, defn_loc_error(&name)),
                );
                context.add_diag(diag);
            }
            CF::Braces => (),
        },
        RC::Variant(variant) => {
            let vloc = variant.decl_loc;
            let vfields = &variant.field_info;
            match form {
                CF::None if vfields.is_empty() => (),
                CF::None => {
                    let (form_upcase, form) = if vfields.is_positional() {
                        (POSNL_UPCASE, POSNL)
                    } else {
                        (NAMED_UPCASE, NAMED)
                    };
                    let msg = invalid_inst_msg!("variant", form_upcase, form);
                    let mut diag = diag!(
                        NameResolution::PositionalCallMismatch,
                        (loc, &msg),
                        (vloc, defn_loc_error(&name)),
                    );
                    if vfields.is_positional() {
                        diag.add_note(posnl_note!());
                    } else {
                        diag.add_note(named_note!());
                    }
                    context.add_diag(diag);
                }
                CF::Parens if vfields.is_empty() => {
                    let msg = invalid_inst_msg!("variant", EMPTY_UPCASE, EMPTY);
                    let mut diag = diag!(
                        NameResolution::PositionalCallMismatch,
                        (loc, msg),
                        (vloc, defn_loc_error(&name)),
                    );
                    diag.add_note(format!("Remove '()' arguments from this {position}"));
                    context.add_diag(diag);
                }
                CF::Parens if vfields.is_positional() => (),
                CF::Parens => {
                    let msg = invalid_inst_msg!("variant", NAMED_UPCASE, NAMED);
                    let mut diag = diag!(
                        NameResolution::PositionalCallMismatch,
                        (loc, &msg),
                        (vloc, defn_loc_error(&name)),
                    );
                    diag.add_note(named_note!());
                    context.add_diag(diag);
                }
                CF::Braces if vfields.is_empty() => {
                    let msg = invalid_inst_msg!("variant", EMPTY_UPCASE, EMPTY);
                    let mut diag = diag!(
                        NameResolution::PositionalCallMismatch,
                        (loc, msg),
                        (vloc, defn_loc_error(&name)),
                    );
                    diag.add_note(format!("Remove '{{ }}' arguments from this {position}"));
                    context.add_diag(diag);
                }
                CF::Braces if vfields.is_positional() => {
                    let msg = invalid_inst_msg!("variant", POSNL_UPCASE, POSNL);
                    let mut diag = diag!(
                        NameResolution::PositionalCallMismatch,
                        (loc, &msg),
                        (vloc, defn_loc_error(&name)),
                    );
                    diag.add_note(posnl_note!());
                    context.add_diag(diag);
                }
                CF::Braces => (),
            }
        }
    }
}

//************************************************
// Match Arms and Patterns
//************************************************

fn match_arm(context: &mut Context, sp!(aloc, arm): E::MatchArm) -> N::MatchArm {
    let E::MatchArm_ {
        pattern,
        guard,
        rhs,
    } = arm;

    let pat_binders = unique_pattern_binders(context, &pattern);

    context.new_local_scope();
    // NB: we just checked the binders for duplicates and listed them all, so now we just need to
    // set up the map and recur down everything.
    let binders: Vec<(Mutability, N::Var)> = pat_binders
        .clone()
        .into_iter()
        .map(|(mut_, binder)| {
            (
                mut_,
                context.declare_local(/* is_parameter */ false, binder.0),
            )
        })
        .collect::<Vec<_>>();

    // Guards are a little tricky: we need them to have similar binders, but they must be different
    // because they may be typed differently than the actual binders (as they are always immutable
    // references). So we push a new scope with new binders paired with the pattern ones, process the
    // guard, and then update the usage of the old binders to account for guard usage.
    context.new_local_scope();
    let guard_binder_pairs: Vec<(N::Var, N::Var)> = binders
        .clone()
        .into_iter()
        .map(|(_, pat_var)| {
            let guard_var = context.declare_local(
                /* is_parameter */ false,
                sp(pat_var.loc, pat_var.value.name),
            );
            (pat_var, guard_var)
        })
        .collect::<Vec<_>>();
    // Next we process the guard to mark guard usage for the guard variables.
    let guard = guard.map(|guard| exp(context, guard));

    // Next we compute the used guard variables, and add the pattern/guard pairs to the guard
    // binders. We assume we don't need to mark unused guard bindings as used (to avoid incorrect
    // unused errors) because we will never check their usage in the unused-checking pass.
    //
    // We also need to mark usage for the pattern variables we do use, but we postpone that until
    // after we handle the right-hand side.
    let mut guard_binders = UniqueMap::new();
    for (pat_var, guard_var) in guard_binder_pairs {
        if context.used_locals.contains(&guard_var.value) {
            guard_binders
                .add(pat_var, guard_var)
                .expect("ICE guard pattern issue");
        }
    }
    context.close_local_scope();

    // Then we visit the right-hand side to mark binder usage there, then compute all the pattern
    // binders used in the right-hand side. Since we didn't mark pattern variables as used by the
    // guard yet, this allows us to record exactly those pattern variables used in the right-hand
    // side so that we can avoid binding them later.
    let rhs = exp(context, rhs);
    let rhs_binders: BTreeSet<N::Var> = binders
        .iter()
        .filter(|(_, binder)| context.used_locals.contains(&binder.value))
        .map(|(_, binder)| *binder)
        .collect();

    // Now we mark usage for the guard-used pattern variables.
    for (pat_var, _) in guard_binders.key_cloned_iter() {
        context.used_locals.insert(pat_var.value);
    }

    // Finally we handle the pattern, replacing unused variables with wildcards
    let pattern = *match_pattern(context, Box::new(pattern));

    context.close_local_scope();

    let arm = N::MatchArm_ {
        pattern,
        binders,
        guard,
        guard_binders,
        rhs_binders,
        rhs,
    };
    sp(aloc, arm)
}

fn unique_pattern_binders(
    context: &mut Context,
    pattern: &E::MatchPattern,
) -> Vec<(Mutability, P::Var)> {
    use E::MatchPattern_ as EP;

    fn report_duplicate(context: &mut Context, var: P::Var, locs: &[(Mutability, Loc)]) {
        assert!(locs.len() > 1, "ICE pattern duplicate detection error");
        let (_, first_loc) = locs.first().unwrap();
        let mut diag = diag!(
            NameResolution::InvalidPattern,
            (*first_loc, format!("binder '{}' is defined here", var))
        );
        for (_, loc) in locs.iter().skip(1) {
            diag.add_secondary_label((*loc, "and repeated here"));
        }
        diag.add_note("A pattern variable must be unique, and must appear once in each or-pattern alternative.");
        context.add_diag(diag);
    }

    enum OrPosn {
        Left,
        Right,
    }

    fn report_mismatched_or(context: &mut Context, posn: OrPosn, var: &P::Var, other_loc: Loc) {
        let (primary_side, secondary_side) = match posn {
            OrPosn::Left => ("left", "right"),
            OrPosn::Right => ("right", "left"),
        };
        let primary_msg = format!("{} or-pattern binds variable {}", primary_side, var);
        let secondary_msg = format!("{} or-pattern does not", secondary_side);
        let mut diag = diag!(NameResolution::InvalidPattern, (var.loc(), primary_msg));
        diag.add_secondary_label((other_loc, secondary_msg));
        diag.add_note("Both sides of an or-pattern must bind the same variables.");
        context.add_diag(diag);
    }

    fn report_mismatched_or_mutability(
        context: &mut Context,
        mutable_loc: Loc,
        immutable_loc: Loc,
        var: &P::Var,
        posn: OrPosn,
    ) {
        let (primary_side, secondary_side) = match posn {
            OrPosn::Left => ("left", "right"),
            OrPosn::Right => ("right", "left"),
        };
        let primary_msg = format!("{} or-pattern binds variable {} mutably", primary_side, var);
        let secondary_msg = format!("{} or-pattern binds it immutably", secondary_side);
        let mut diag = diag!(NameResolution::InvalidPattern, (mutable_loc, primary_msg));
        diag.add_secondary_label((immutable_loc, secondary_msg));
        diag.add_note(
            "Both sides of an or-pattern must bind the same variables with the same mutability.",
        );
        context.add_diag(diag);
    }

    type Bindings = BTreeMap<P::Var, Vec<(Mutability, Loc)>>;

    fn report_duplicates_and_combine(
        context: &mut Context,
        all_bindings: Vec<Bindings>,
    ) -> Bindings {
        match all_bindings.len() {
            0 => BTreeMap::new(),
            1 => all_bindings[0].clone(),
            _ => {
                let mut out_bindings = all_bindings[0].clone();
                let mut duplicates = BTreeSet::new();
                for bindings in all_bindings.into_iter().skip(1) {
                    for (key, mut locs) in bindings {
                        if out_bindings.contains_key(&key) {
                            duplicates.insert(key);
                        }
                        out_bindings.entry(key).or_default().append(&mut locs);
                    }
                }
                for key in duplicates {
                    report_duplicate(context, key, out_bindings.get(&key).unwrap());
                }
                out_bindings
            }
        }
    }

    fn check_duplicates(context: &mut Context, sp!(ploc, pattern): &E::MatchPattern) -> Bindings {
        match pattern {
            EP::Binder(_, var) if var.is_underscore() => BTreeMap::new(),
            EP::Binder(mut_, var) => [(*var, vec![(*mut_, *ploc)])].into_iter().collect(),
            EP::At(var, inner) => {
                let mut bindings: Bindings = BTreeMap::new();
                if !var.is_underscore() {
                    bindings
                        .entry(*var)
                        .or_default()
                        .push((Mutability::Imm, *ploc));
                }
                let new_bindings = check_duplicates(context, inner);
                bindings = report_duplicates_and_combine(context, vec![bindings, new_bindings]);
                bindings
            }
            EP::PositionalConstructor(_, _, sp!(_, patterns)) => {
                let bindings = patterns
                    .iter()
                    .filter_map(|pat| match pat {
                        E::Ellipsis::Binder(p) => Some(check_duplicates(context, p)),
                        E::Ellipsis::Ellipsis(_) => None,
                    })
                    .collect();
                report_duplicates_and_combine(context, bindings)
            }
            EP::NamedConstructor(_, _, fields, _) => {
                let mut bindings = vec![];
                for (_, _, (_, pat)) in fields {
                    bindings.push(check_duplicates(context, pat));
                }
                report_duplicates_and_combine(context, bindings)
            }
            EP::Or(left, right) => {
                let mut left_bindings = check_duplicates(context, left);
                let mut right_bindings = check_duplicates(context, right);
                for (key, mut_and_locs) in left_bindings.iter_mut() {
                    if !right_bindings.contains_key(key) {
                        report_mismatched_or(context, OrPosn::Left, key, right.loc);
                    } else {
                        let lhs_mutability = mut_and_locs.first().map(|(m, _)| *m).unwrap();
                        let rhs_mutability = right_bindings
                            .get(key)
                            .map(|mut_and_locs| mut_and_locs.first().map(|(m, _)| *m).unwrap())
                            .unwrap();
                        match (lhs_mutability, rhs_mutability) {
                            // LHS variable mutable, RHS variable immutable
                            (Mutability::Mut(lhs_loc), Mutability::Imm) => {
                                report_mismatched_or_mutability(
                                    context,
                                    lhs_loc,
                                    right.loc,
                                    key,
                                    OrPosn::Left,
                                );
                                // Mutabilities are mismatched so update them to all be mutable to
                                // avoid further errors further down the line.
                                if let Some(mut_and_locs) = right_bindings.get_mut(key) {
                                    for m in mut_and_locs
                                        .iter_mut()
                                        .filter(|(m, _)| matches!(m, Mutability::Imm))
                                    {
                                        m.0 = Mutability::Mut(lhs_loc);
                                    }
                                }
                            }
                            (Mutability::Imm, Mutability::Mut(rhs_loc)) => {
                                // RHS variable mutable, LHS variable immutable
                                report_mismatched_or_mutability(
                                    context,
                                    rhs_loc,
                                    key.loc(),
                                    key,
                                    OrPosn::Right,
                                );
                                // Mutabilities are mismatched so update them to all be mutable to
                                // avoid further errors further down the line.
                                for m in mut_and_locs
                                    .iter_mut()
                                    .filter(|(m, _)| matches!(m, Mutability::Imm))
                                {
                                    m.0 = Mutability::Mut(rhs_loc);
                                }
                            }
                            _ => (),
                        }
                    }
                }

                let right_keys = right_bindings.keys().copied().collect::<Vec<_>>();
                for key in right_keys {
                    let lhs_entry = left_bindings.get_mut(&key);
                    let rhs_entry = right_bindings.remove(&key);
                    match (lhs_entry, rhs_entry) {
                        (Some(left_locs), Some(mut right_locs)) => {
                            left_locs.append(&mut right_locs);
                        }
                        (None, Some(right_locs)) => {
                            report_mismatched_or(context, OrPosn::Right, &key, left.loc);
                            left_bindings.insert(key, right_locs);
                        }
                        (_, None) => panic!("ICE pattern key missing"),
                    }
                }
                left_bindings
            }
            EP::ModuleAccessName(_, _) | EP::Literal(_) | EP::ErrorPat => BTreeMap::new(),
        }
    }

    check_duplicates(context, pattern)
        .into_iter()
        .map(|(var, vs)| (vs.first().map(|x| x.0).unwrap(), var))
        .collect::<Vec<_>>()
}

fn expand_positional_ellipsis<T>(
    context: &mut Context,
    missing: isize,
    args: Vec<E::Ellipsis<Spanned<T>>>,
    replacement: impl Fn(Loc) -> Spanned<T>,
) -> Vec<(Field, (usize, Spanned<T>))> {
    args.into_iter()
        .flat_map(|p| match p {
            E::Ellipsis::Binder(p) => vec![p],
            E::Ellipsis::Ellipsis(eloc) => {
                let result = (0..=missing).map(|_| replacement(eloc)).collect::<Vec<_>>();
                if context.env.ide_mode() {
                    let entries = (0..=missing).map(|_| "_".into()).collect::<Vec<_>>();
                    let info = EllipsisMatchEntries::Positional(entries);
                    let info = ide::IDEAnnotation::EllipsisMatchEntries(Box::new(info));
                    context.add_ide_annotation(eloc, info);
                }
                result
            }
        })
        .enumerate()
        .map(|(idx, p)| {
            let field = Field::add_loc(p.loc, format!("{idx}").into());
            (field, (idx, p))
        })
        .collect()
}

fn expand_named_ellipsis<T>(
    context: &mut Context,
    field_info: &FieldInfo,
    head_loc: Loc,
    ellipsis_loc: Loc,
    args: &mut UniqueMap<Field, (usize, Spanned<T>)>,
    replacement: impl Fn(Loc) -> Spanned<T>,
) {
    let mut fields = match field_info {
        FieldInfo::Empty => BTreeSet::new(),
        FieldInfo::Named(fields) => fields.clone(),
        FieldInfo::Positional(num_fields) => (0..*num_fields)
            .map(|i| Field::add_loc(head_loc, format!("{i}").into()))
            .collect(),
    };

    for (k, _) in args.key_cloned_iter() {
        fields.remove(&k);
    }

    if context.env.ide_mode() {
        let entries = fields.iter().map(|field| field.value()).collect::<Vec<_>>();
        let info = EllipsisMatchEntries::Named(entries);
        let info = ide::IDEAnnotation::EllipsisMatchEntries(Box::new(info));
        context.add_ide_annotation(ellipsis_loc, info);
    }

    let start_idx = args.len();
    for (i, f) in fields.into_iter().enumerate() {
        args.add(
            Field(sp(ellipsis_loc, f.value())),
            (start_idx + i, replacement(ellipsis_loc)),
        )
        .unwrap();
    }
}

fn match_pattern(context: &mut Context, in_pat: Box<E::MatchPattern>) -> Box<N::MatchPattern> {
    use E::MatchPattern_ as EP;
    use N::MatchPattern_ as NP;

    let sp!(ploc, pat_) = *in_pat;

    let pat_: N::MatchPattern_ = match pat_ {
        EP::PositionalConstructor(name, etys_opt, args) => {
            let Some(ctor) = context.resolve_datatype_constructor(name, "pattern") else {
                assert!(context.env.has_errors());
                return Box::new(sp(ploc, NP::ErrorPat));
            };

            let tys_opt = types_opt_with_instantiation_arity_check(
                context,
                TypeAnnotation::Expression,
                ploc,
                || ctor.type_name(),
                etys_opt,
                ctor.type_arity(),
            );

            check_constructor_form(context, ploc, ConstructorForm::Parens, "pattern", &ctor);

            let field_info = ctor.field_info();
            let n_pats = args
                .value
                .into_iter()
                .map(|ellipsis| match ellipsis {
                    Ellipsis::Binder(pat) => {
                        Ellipsis::Binder(*match_pattern(context, Box::new(pat)))
                    }
                    Ellipsis::Ellipsis(loc) => Ellipsis::Ellipsis(loc),
                })
                .collect::<Vec<_>>();
            // NB: We may have more args than fields! Since we allow `..` to be zero-or-more
            // wildcards.
            let missing = (field_info.field_count() as isize) - n_pats.len() as isize;
            let args =
                expand_positional_ellipsis(context, missing, n_pats, |eloc| sp(eloc, NP::Wildcard));
            let args = UniqueMap::maybe_from_iter(args.into_iter()).expect("ICE naming failed");

            match ctor {
                ResolvedConstructor::Struct(stype) => {
                    NP::Struct(stype.mident, stype.name, tys_opt, args)
                }
                ResolvedConstructor::Variant(vtype) => {
                    NP::Variant(vtype.mident, vtype.enum_name, vtype.name, tys_opt, args)
                }
            }
        }
        EP::NamedConstructor(name, etys_opt, args, ellipsis) => {
            let Some(ctor) = context.resolve_datatype_constructor(name, "pattern") else {
                assert!(context.env.has_errors());
                return Box::new(sp(ploc, NP::ErrorPat));
            };
            let tys_opt = types_opt_with_instantiation_arity_check(
                context,
                TypeAnnotation::Expression,
                ploc,
                || ctor.type_name(),
                etys_opt,
                ctor.type_arity(),
            );

            check_constructor_form(context, ploc, ConstructorForm::Braces, "pattern", &ctor);

            let field_info = ctor.field_info();
            let mut args = args.map(|_, (idx, p)| (idx, *match_pattern(context, Box::new(p))));
            // If we have an ellipsis fill in any missing patterns
            if let Some(ellipsis_loc) = ellipsis {
                expand_named_ellipsis(context, field_info, ploc, ellipsis_loc, &mut args, |eloc| {
                    sp(eloc, NP::Wildcard)
                });
            }

            match ctor {
                ResolvedConstructor::Struct(stype) => {
                    NP::Struct(stype.mident, stype.name, tys_opt, args)
                }
                ResolvedConstructor::Variant(vtype) => {
                    NP::Variant(vtype.mident, vtype.enum_name, vtype.name, tys_opt, args)
                }
            }
        }
        EP::ModuleAccessName(name, etys_opt) => {
            match context.resolve_pattern_term(name) {
                ResolvedPatternTerm::Constant(const_) => {
                    if etys_opt.is_some() {
                        context.add_diag(diag!(
                            NameResolution::TooManyTypeArguments,
                            (ploc, "Constants in patterns do not take type arguments")
                        ));
                    }
                    NP::Constant(const_.mident, const_.name)
                }
                ResolvedPatternTerm::Constructor(ctor) => {
                    let tys_opt = types_opt_with_instantiation_arity_check(
                        context,
                        TypeAnnotation::Expression,
                        ploc,
                        || ctor.type_name(),
                        etys_opt,
                        ctor.type_arity(),
                    );
                    match *ctor {
                        ResolvedConstructor::Struct(stype) => {
                            // No need to chck is_empty / is_positional because typing will report the errors.
                            NP::Struct(stype.mident, stype.name, tys_opt, UniqueMap::new())
                        }
                        ResolvedConstructor::Variant(vtype) => {
                            // No need to chck is_empty / is_positional because typing will report the errors.
                            NP::Variant(
                                vtype.mident,
                                vtype.enum_name,
                                vtype.name,
                                tys_opt,
                                UniqueMap::new(),
                            )
                        }
                    }
                }
                ResolvedPatternTerm::Unbound => {
                    assert!(context.env.has_errors());
                    NP::ErrorPat
                }
            }
        }
        EP::Binder(_, binder) if binder.is_underscore() => NP::Wildcard,
        EP::Binder(mut_, binder) => {
            if let Some(binder) = context.resolve_pattern_binder(binder.loc(), binder.0) {
                NP::Binder(mut_, binder, false)
            } else {
                assert!(context.env.has_errors());
                NP::ErrorPat
            }
        }
        EP::ErrorPat => NP::ErrorPat,
        EP::Literal(v) => NP::Literal(v),
        EP::Or(lhs, rhs) => NP::Or(match_pattern(context, lhs), match_pattern(context, rhs)),
        EP::At(binder, body) => {
            if let Some(binder) = context.resolve_pattern_binder(binder.loc(), binder.0) {
                NP::At(
                    binder,
                    /* unused_binding */ false,
                    match_pattern(context, body),
                )
            } else {
                assert!(context.env.has_errors());
                match_pattern(context, body).value
            }
        }
    };
    Box::new(sp(ploc, pat_))
}

//************************************************
// LValues
//************************************************

#[derive(Clone, Copy)]
enum LValueCase {
    Bind,
    Assign,
}

fn lvalue(
    context: &mut Context,
    seen_locals: &mut UniqueMap<Name, ()>,
    case: LValueCase,
    sp!(loc, l_): E::LValue,
) -> Option<N::LValue> {
    use LValueCase as C;
    use E::LValue_ as EL;
    use N::LValue_ as NL;
    let nl_ = match l_ {
        EL::Var(mut_, sp!(_, E::ModuleAccess_::Name(n)), None) => {
            let v = P::Var(n);
            if v.is_underscore() {
                check_mut_underscore(context, mut_);
                NL::Ignore
            } else {
                if let Err((var, prev_loc)) = seen_locals.add(n, ()) {
                    let (primary, secondary) = match case {
                        C::Bind => {
                            let msg = format!(
                                "Duplicate declaration for local '{}' in a given 'let'",
                                &var
                            );
                            ((var.loc, msg), (prev_loc, "Previously declared here"))
                        }
                        C::Assign => {
                            let msg = format!(
                                "Duplicate usage of local '{}' in a given assignment",
                                &var
                            );
                            ((var.loc, msg), (prev_loc, "Previously assigned here"))
                        }
                    };
                    context.add_diag(diag!(Declarations::DuplicateItem, primary, secondary));
                }
                if v.is_syntax_identifier() {
                    debug_assert!(
                        matches!(case, C::Assign),
                        "ICE this should fail during parsing"
                    );
                    let msg = format!(
                        "Cannot assign to argument for parameter '{}'. \
                        Arguments must be used in value positions",
                        v.0
                    );
                    let mut diag = diag!(TypeSafety::CannotExpandMacro, (loc, msg));
                    diag.add_note(ASSIGN_SYNTAX_IDENTIFIER_NOTE);
                    context.add_diag(diag);
                    return None;
                }
                let nv = match case {
                    C::Bind => {
                        let is_parameter = false;
                        context.declare_local(is_parameter, n)
                    }
                    C::Assign => context.resolve_local(
                        loc,
                        NameResolution::UnboundVariable,
                        |name| format!("Invalid assignment. Unbound variable '{name}'"),
                        n,
                    )?,
                };
                NL::Var {
                    mut_,
                    var: nv,
                    // set later
                    unused_binding: false,
                }
            }
        }
        EL::Unpack(tn, etys_opt, efields) => {
            let msg = match case {
                C::Bind => "deconstructing binding",
                C::Assign => "deconstructing assignment",
            };
            let stype = match context.resolve_datatype_constructor(tn, "left-hand side") {
                Some(ctor @ ResolvedConstructor::Struct(_)) => {
                    check_constructor_form(
                        context,
                        loc,
                        match efields {
                            E::FieldBindings::Named(_, _) => ConstructorForm::Braces,
                            E::FieldBindings::Positional(_) => ConstructorForm::Parens,
                        },
                        "deconstruction",
                        &ctor,
                    );
                    let ResolvedConstructor::Struct(stype) = ctor else {
                        unreachable!()
                    };
                    stype
                }
                Some(ResolvedConstructor::Variant(variant)) => {
                    context.add_diag(diag!(
                        NameResolution::NamePositionMismatch,
                        (tn.loc, format!("Invalid {}. Expected a struct", msg)),
                        (
                            variant.enum_name.loc(),
                            format!("But '{}' is an enum", variant.enum_name)
                        )
                    ));
                    return None;
                }
                None => {
                    assert!(context.env.has_errors());
                    return None;
                }
            };
            let tys_opt = types_opt_with_instantiation_arity_check(
                context,
                TypeAnnotation::Expression,
                loc,
                || format!("{}::{}", &stype.mident, &stype.name),
                etys_opt,
                stype.tyarg_arity,
            );
            let make_ignore = |loc| {
                let var = sp(loc, Symbol::from("_"));
                let name = E::ModuleAccess::new(loc, E::ModuleAccess_::Name(var));
                sp(loc, E::LValue_::Var(None, name, None))
            };
            let efields = match efields {
                E::FieldBindings::Named(mut efields, ellipsis) => {
                    if let Some(ellipsis_loc) = ellipsis {
                        expand_named_ellipsis(
                            context,
                            &stype.field_info,
                            loc,
                            ellipsis_loc,
                            &mut efields,
                            make_ignore,
                        );
                    }

                    efields
                }
                E::FieldBindings::Positional(lvals) => {
                    let fields = stype.field_info.field_count();
                    let missing = (fields as isize) - lvals.len() as isize;

                    let expanded_lvals =
                        expand_positional_ellipsis(context, missing, lvals, make_ignore);
                    UniqueMap::maybe_from_iter(expanded_lvals.into_iter()).unwrap()
                }
            };

            let nfields =
                UniqueMap::maybe_from_opt_iter(efields.into_iter().map(|(k, (idx, inner))| {
                    Some((k, (idx, lvalue(context, seen_locals, case, inner)?)))
                }))?;
            NL::Unpack(
                stype.mident,
                stype.name,
                tys_opt,
                nfields.expect("ICE fields were already unique"),
            )
        }
        e @ EL::Var(_, _, _) => {
            let mut diag = ice!((
                loc,
                "ICE compiler should not have parsed this form as a specification"
            ));
            diag.add_note(format!("Compiler parsed: {}", debug_display!(e)));
            context.add_diag(diag);
            NL::Ignore
        }
    };
    Some(sp(loc, nl_))
}

fn check_mut_underscore(context: &mut Context, mut_: Option<Mutability>) {
    // no error if not a mut declaration
    let Some(Mutability::Mut(loc)) = mut_ else {
        return;
    };
    let msg = "Invalid 'mut' declaration. 'mut' is applied to variables and cannot be applied to the '_' pattern";
    context.add_diag(diag!(NameResolution::InvalidMut, (loc, msg)));
}

fn bind_list(context: &mut Context, ls: E::LValueList) -> Option<N::LValueList> {
    lvalue_list(context, &mut UniqueMap::new(), LValueCase::Bind, ls)
}

fn lambda_bind_list(
    context: &mut Context,
    sp!(loc, elambda): E::LambdaLValues,
) -> Option<N::LambdaLValues> {
    let nlambda = elambda
        .into_iter()
        .map(|(pbs, ty_opt)| {
            let bs = bind_list(context, pbs)?;
            let ety = ty_opt.map(|t| type_(context, TypeAnnotation::Expression, t));
            Some((bs, ety))
        })
        .collect::<Option<_>>()?;
    Some(sp(loc, nlambda))
}

fn assign_list(context: &mut Context, ls: E::LValueList) -> Option<N::LValueList> {
    lvalue_list(context, &mut UniqueMap::new(), LValueCase::Assign, ls)
}

fn lvalue_list(
    context: &mut Context,
    seen_locals: &mut UniqueMap<Name, ()>,
    case: LValueCase,
    sp!(loc, b_): E::LValueList,
) -> Option<N::LValueList> {
    use N::LValue_ as NL;
    Some(sp(
        loc,
        b_.into_iter()
            .map(|inner| {
                let inner_loc = inner.loc;
                lvalue(context, seen_locals, case, inner).unwrap_or_else(|| {
                    assert!(context.env.has_errors());
                    sp(inner_loc, NL::Error)
                })
            })
            .collect::<Vec<_>>(),
    ))
}

//**************************************************************************************************
// Resolvers
//**************************************************************************************************

fn resolve_call(
    context: &mut Context,
    call_loc: Loc,
    fun_name: E::ModuleAccess,
    is_macro: Option<Loc>,
    in_tyargs_opt: Option<Vec<E::Type>>,
    in_args: Spanned<Vec<E::Exp>>,
) -> N::Exp_ {
    use N::BuiltinFunction_ as B;

    let subject_loc = fun_name.loc;
    let mut args = call_args(context, in_args);

    match context.resolve_call_subject(fun_name) {
        ResolvedCallSubject::Function(mf) => {
            let ResolvedModuleFunction {
                mident,
                name,
                tyarg_arity: _,
                arity: _,
            } = *mf;
            // TODO This is a weird place to check this feature gate.
            if let Some(mloc) = is_macro {
                context.check_feature(context.current_package, FeatureGate::MacroFuns, mloc);
            }
            // TODO. We could check arities here, but we don't; type dones that, instead.
            let tyargs_opt = types_opt(context, TypeAnnotation::Expression, in_tyargs_opt);
            N::Exp_::ModuleCall(mident, name, is_macro, tyargs_opt, args)
        }
        ResolvedCallSubject::Builtin(bf) => {
            let builtin_ = match &bf.fun.value {
                B::Freeze(_) => {
                    check_is_not_macro(context, is_macro, B::FREEZE);
                    let tyargs_opt = exp_types_opt_with_arity_check(
                        context,
                        subject_loc,
                        || format!("Invalid call to builtin function: '{}'", B::FREEZE),
                        call_loc,
                        in_tyargs_opt,
                        1,
                    );
                    match tyargs_opt.as_deref() {
                        Some([ty]) => B::Freeze(Some(ty.clone())),
                        Some(_tys) => {
                            context.add_diag(ice!((call_loc, "Builtin tyarg arity failure")));
                            return N::Exp_::UnresolvedError;
                        }
                        None => B::Freeze(None),
                    }
                }
                B::Assert(_) => {
                    if is_macro.is_none() {
                        let dep_msg = format!(
                            "'{}' function syntax has been deprecated and will be removed",
                            B::ASSERT_MACRO
                        );
                        // TODO make this a tip/hint?
                        let help_msg = format!(
                            "Replace with '{0}!'. '{0}' has been replaced with a '{0}!' built-in \
                            macro so that arguments are no longer eagerly evaluated",
                            B::ASSERT_MACRO
                        );
                        let mut diag =
                            diag!(Uncategorized::DeprecatedWillBeRemoved, (call_loc, dep_msg),);
                        diag.add_note(help_msg);
                        context.add_diag(diag);
                    }
                    exp_types_opt_with_arity_check(
                        context,
                        subject_loc,
                        || format!("Invalid call to builtin function: '{}'", B::ASSERT_MACRO),
                        call_loc,
                        in_tyargs_opt,
                        0,
                    );
                    // If no abort code is given for the assert, we add in the abort code as the
                    // bitset-line-number if `CleverAssertions` is set.
                    if args.value.len() == 1 && is_macro.is_some() {
                        context.check_feature(
                            context.current_package,
                            FeatureGate::CleverAssertions,
                            subject_loc,
                        );
                        args.value.push(sp(
                            call_loc,
                            N::Exp_::ErrorConstant {
                                line_number_loc: subject_loc,
                            },
                        ));
                    }
                    B::Assert(is_macro)
                }
            };
            N::Exp_::Builtin(sp(subject_loc, builtin_), args)
        }
        ResolvedCallSubject::Constructor(_) => {
            context.check_feature(
                context.current_package,
                FeatureGate::PositionalFields,
                call_loc,
            );
            report_invalid_macro(context, is_macro, "Datatypes");
            let Some(ctor) = context.resolve_datatype_constructor(fun_name, "construction") else {
                assert!(context.env.has_errors());
                return N::Exp_::UnresolvedError;
            };
            let tyargs_opt = exp_types_opt_with_arity_check(
                context,
                subject_loc,
                || "Invalid call to constructor".to_string(),
                call_loc,
                in_tyargs_opt,
                ctor.type_arity(),
            );
            check_constructor_form(
                context,
                call_loc,
                ConstructorForm::Parens,
                "instantiation",
                &ctor,
            );
            let fields =
                UniqueMap::maybe_from_iter(args.value.into_iter().enumerate().map(|(idx, e)| {
                    let field = Field::add_loc(e.loc, format!("{idx}").into());
                    (field, (idx, e))
                }))
                .unwrap();
            match ctor {
                ResolvedConstructor::Struct(stype) => {
                    N::Exp_::Pack(stype.mident, stype.name, tyargs_opt, fields)
                }
                ResolvedConstructor::Variant(vtype) => N::Exp_::PackVariant(
                    vtype.mident,
                    vtype.enum_name,
                    vtype.name,
                    tyargs_opt,
                    fields,
                ),
            }
        }
        ResolvedCallSubject::Var(var) => {
            context.check_feature(context.current_package, FeatureGate::Lambda, call_loc);

            check_is_not_macro(context, is_macro, &var.value.name);
            let tyargs_opt = types_opt(context, TypeAnnotation::Expression, in_tyargs_opt);
            if tyargs_opt.is_some() {
                context.add_diag(diag!(
                    NameResolution::TooManyTypeArguments,
                    (
                        subject_loc,
                        "Invalid lambda call. Expected zero type arguments"
                    ),
                ));
            }
            // If this variable refers to a local (num > 0) or it isn't syntax, error.
            if !var.value.is_syntax_identifier() {
                let name = var.value.name;
                let msg = format!(
                    "Unexpected invocation of parameter or local '{name}'. \
                                     Non-syntax variables cannot be invoked as functions",
                );
                let note = format!(
                    "Only macro syntax variables, e.g. '${name}', \
                            may be invoked as functions."
                );
                let mut diag = diag!(TypeSafety::InvalidCallTarget, (var.loc, msg));
                diag.add_note(note);
                context.add_diag(diag);
                N::Exp_::UnresolvedError
            } else if var.value.id != 0 {
                let msg = format!(
                    "Unexpected invocation of non-parameter variable '{}'. \
                                     Only lambda-typed syntax parameters may be invoked",
                    var.value.name
                );
                context.add_diag(diag!(TypeSafety::InvalidCallTarget, (var.loc, msg)));
                N::Exp_::UnresolvedError
            } else {
                N::Exp_::VarCall(sp(subject_loc, var.value), args)
            }
        }
        ResolvedCallSubject::Unbound => N::Exp_::UnresolvedError,
    }
}

//**************************************************************************************************
// General helpers
//**************************************************************************************************

fn check_is_not_macro(context: &mut Context, is_macro: Option<Loc>, name: &str) {
    if let Some(mloc) = is_macro {
        let msg = format!(
            "Unexpected macro invocation. '{}' cannot be invoked as a \
                   macro",
            name
        );
        context.add_diag(diag!(TypeSafety::InvalidCallTarget, (mloc, msg)));
    }
}

fn report_invalid_macro(context: &mut Context, is_macro: Option<Loc>, kind: &str) {
    if let Some(mloc) = is_macro {
        let msg = format!(
            "Unexpected macro invocation. {} cannot be invoked as macros",
            kind
        );
        context.add_diag(diag!(NameResolution::PositionalCallMismatch, (mloc, msg)));
    }
}

fn exp_types_opt_with_arity_check(
    context: &mut Context,
    msg_loc: Loc,
    fmsg: impl Fn() -> String,
    tyarg_error_loc: Loc,
    tyargs_opt: Option<Vec<E::Type>>,
    arity: usize,
) -> Option<Vec<N::Type>> {
    let tyargs_opt = tyargs_opt.map(|etys| types(context, TypeAnnotation::Expression, etys));
    let Some(mut args) = tyargs_opt else {
        return None;
    };
    let args_len = args.len();
    if args_len != arity {
        let diag_code = if args_len > arity {
            NameResolution::TooManyTypeArguments
        } else {
            NameResolution::TooFewTypeArguments
        };
        let msg = fmsg();
        let targs_msg = format!("Expected {} type argument(s) but got {}", arity, args_len);
        context.add_diag(diag!(
            diag_code,
            (msg_loc, msg),
            (tyarg_error_loc, targs_msg)
        ));
    }

    while args.len() > arity {
        args.pop();
    }

    while args.len() < arity {
        args.push(sp(tyarg_error_loc, N::Type_::UnresolvedError));
    }

    Some(args)
}

//**************************************************************************************************
// Unused locals
//**************************************************************************************************

fn remove_unused_bindings_function(
    context: &mut Context,
    used: &BTreeSet<N::Var_>,
    f: &mut N::Function,
) {
    match &mut f.body.value {
        N::FunctionBody_::Defined(seq) => remove_unused_bindings_seq(context, used, seq),
        // no warnings for natives
        N::FunctionBody_::Native => return,
    }
    for (_, v, _) in &mut f.signature.parameters {
        if !used.contains(&v.value) {
            report_unused_local(context, v);
        }
    }
}

fn remove_unused_bindings_seq(
    context: &mut Context,
    used: &BTreeSet<N::Var_>,
    seq: &mut N::Sequence,
) {
    for sp!(_, item_) in &mut seq.1 {
        match item_ {
            N::SequenceItem_::Seq(e) => remove_unused_bindings_exp(context, used, e),
            N::SequenceItem_::Declare(lvalues, _) => {
                // unused bindings will be reported as unused assignments
                remove_unused_bindings_lvalues(context, used, lvalues)
            }
            N::SequenceItem_::Bind(lvalues, e) => {
                remove_unused_bindings_lvalues(context, used, lvalues);
                remove_unused_bindings_exp(context, used, e)
            }
        }
    }
}

fn remove_unused_bindings_lvalues(
    context: &mut Context,
    used: &BTreeSet<N::Var_>,
    sp!(_, lvalues): &mut N::LValueList,
) {
    for lvalue in lvalues {
        remove_unused_bindings_lvalue(context, used, lvalue)
    }
}

fn remove_unused_bindings_lvalue(
    context: &mut Context,
    used: &BTreeSet<N::Var_>,
    sp!(_, lvalue_): &mut N::LValue,
) {
    match lvalue_ {
        N::LValue_::Ignore => (),
        N::LValue_::Error => (),
        N::LValue_::Var {
            var,
            unused_binding,
            ..
        } if used.contains(&var.value) => {
            debug_assert!(!*unused_binding);
        }
        N::LValue_::Var {
            var,
            unused_binding,
            ..
        } => {
            debug_assert!(!*unused_binding);
            report_unused_local(context, var);
            *unused_binding = true;
        }
        N::LValue_::Unpack(_, _, _, lvalues) => {
            for (_, _, (_, lvalue)) in lvalues {
                remove_unused_bindings_lvalue(context, used, lvalue)
            }
        }
    }
}

fn remove_unused_bindings_exp(
    context: &mut Context,
    used: &BTreeSet<N::Var_>,
    sp!(_, e_): &mut N::Exp,
) {
    match e_ {
        N::Exp_::Value(_)
        | N::Exp_::Var(_)
        | N::Exp_::Constant(_, _)
        | N::Exp_::Continue(_)
        | N::Exp_::Unit { .. }
        | N::Exp_::ErrorConstant { .. }
        | N::Exp_::UnresolvedError => (),
        N::Exp_::Return(e)
        | N::Exp_::Abort(e)
        | N::Exp_::Dereference(e)
        | N::Exp_::UnaryExp(_, e)
        | N::Exp_::Cast(e, _)
        | N::Exp_::Assign(_, e)
        | N::Exp_::Loop(_, e)
        | N::Exp_::Give(_, _, e)
        | N::Exp_::Annotate(e, _) => remove_unused_bindings_exp(context, used, e),
        N::Exp_::IfElse(econd, et, ef_opt) => {
            remove_unused_bindings_exp(context, used, econd);
            remove_unused_bindings_exp(context, used, et);
            if let Some(ef) = ef_opt {
                remove_unused_bindings_exp(context, used, ef);
            }
        }
        N::Exp_::Match(esubject, arms) => {
            remove_unused_bindings_exp(context, used, esubject);
            for arm in &mut arms.value {
                remove_unused_bindings_pattern(context, used, &mut arm.value.pattern);
                if let Some(guard) = arm.value.guard.as_mut() {
                    remove_unused_bindings_exp(context, used, guard)
                }
                remove_unused_bindings_exp(context, used, &mut arm.value.rhs);
            }
        }
        N::Exp_::While(_, econd, ebody) => {
            remove_unused_bindings_exp(context, used, econd);
            remove_unused_bindings_exp(context, used, ebody)
        }
        N::Exp_::Block(N::Block {
            name: _,
            from_macro_argument: _,
            seq,
        }) => remove_unused_bindings_seq(context, used, seq),
        N::Exp_::Lambda(N::Lambda {
            parameters: sp!(_, parameters),
            return_label: _,
            return_type: _,
            use_fun_color: _,
            body,
        }) => {
            for (lvs, _) in parameters {
                remove_unused_bindings_lvalues(context, used, lvs)
            }
            remove_unused_bindings_exp(context, used, body)
        }
        N::Exp_::FieldMutate(ed, e) => {
            remove_unused_bindings_exp_dotted(context, used, ed);
            remove_unused_bindings_exp(context, used, e)
        }
        N::Exp_::Mutate(el, er) | N::Exp_::BinopExp(el, _, er) => {
            remove_unused_bindings_exp(context, used, el);
            remove_unused_bindings_exp(context, used, er)
        }
        N::Exp_::Pack(_, _, _, fields) => {
            for (_, _, (_, e)) in fields {
                remove_unused_bindings_exp(context, used, e)
            }
        }
        N::Exp_::PackVariant(_, _, _, _, fields) => {
            for (_, _, (_, e)) in fields {
                remove_unused_bindings_exp(context, used, e)
            }
        }

        N::Exp_::Builtin(_, sp!(_, es))
        | N::Exp_::Vector(_, _, sp!(_, es))
        | N::Exp_::ModuleCall(_, _, _, _, sp!(_, es))
        | N::Exp_::VarCall(_, sp!(_, es))
        | N::Exp_::ExpList(es) => {
            for e in es {
                remove_unused_bindings_exp(context, used, e)
            }
        }
        N::Exp_::MethodCall(ed, _, _, _, _, sp!(_, es)) => {
            remove_unused_bindings_exp_dotted(context, used, ed);
            for e in es {
                remove_unused_bindings_exp(context, used, e)
            }
        }

        N::Exp_::ExpDotted(_, ed) => remove_unused_bindings_exp_dotted(context, used, ed),
    }
}

fn remove_unused_bindings_exp_dotted(
    context: &mut Context,
    used: &BTreeSet<N::Var_>,
    sp!(_, ed_): &mut N::ExpDotted,
) {
    match ed_ {
        N::ExpDotted_::Exp(e) => remove_unused_bindings_exp(context, used, e),
        N::ExpDotted_::Dot(ed, _, _) | N::ExpDotted_::DotAutocomplete(_, ed) => {
            remove_unused_bindings_exp_dotted(context, used, ed)
        }
        N::ExpDotted_::Index(ed, sp!(_, es)) => {
            for e in es {
                remove_unused_bindings_exp(context, used, e);
            }
            remove_unused_bindings_exp_dotted(context, used, ed)
        }
    }
}

fn remove_unused_bindings_pattern(
    context: &mut Context,
    used: &BTreeSet<N::Var_>,
    sp!(_, pat_): &mut N::MatchPattern,
) {
    use N::MatchPattern_ as NP;
    match pat_ {
        NP::Constant(_, _) | NP::Literal(_) | NP::Wildcard | NP::ErrorPat => (),
        NP::Variant(_, _, _, _, fields) => {
            for (_, _, (_, pat)) in fields {
                remove_unused_bindings_pattern(context, used, pat)
            }
        }
        NP::Struct(_, _, _, fields) => {
            for (_, _, (_, pat)) in fields {
                remove_unused_bindings_pattern(context, used, pat)
            }
        }
        NP::Binder(_, var, unused_binding) => {
            if !used.contains(&var.value) {
                report_unused_local(context, var);
                *unused_binding = true;
            }
        }
        NP::Or(lhs, rhs) => {
            remove_unused_bindings_pattern(context, used, lhs);
            remove_unused_bindings_pattern(context, used, rhs);
        }
        NP::At(var, unused_binding, inner) => {
            if !used.contains(&var.value) {
                report_unused_local(context, var);
                *unused_binding = true;
                remove_unused_bindings_pattern(context, used, inner);
            } else {
                remove_unused_bindings_pattern(context, used, &mut *inner);
            }
        }
    }
}

fn report_unused_local(context: &mut Context, sp!(loc, unused_): &N::Var) {
    if unused_.starts_with_underscore() || !unused_.is_valid() {
        return;
    }
    let N::Var_ { name, id, color } = unused_;
    debug_assert!(*color == 0);
    let is_parameter = *id == 0;
    let kind = if is_parameter {
        "parameter"
    } else {
        "local variable"
    };
    let msg = format!(
        "Unused {kind} '{name}'. Consider removing or prefixing with an underscore: '_{name}'",
    );
    context.add_diag(diag!(UnusedItem::Variable, (*loc, msg)));
}
