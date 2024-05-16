// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/// Name resolution. This is driven by the trait PathResolver, which works over
/// a DefnContext and resolves according to the rules of the selected resolver (to preserve legacy
/// behavior versus newer resolution behaviors).
use crate::{
    diag,
    diagnostics::{self, codes::NameResolution, Diagnostic},
    editions::{create_feature_error, Edition, FeatureGate},
    expansion::{
        ast::{self as E, AbilitySet, Address, ModuleIdent, ModuleIdent_},
        valid_names::{check_valid_address_name, is_valid_datatype_or_constant_name},
    },
    ice, ice_assert,
    naming::{
        address::make_address,
        alias_map_builder::{AliasEntry, AliasMapBuilder, NameSpace},
        aliases::{NameMap, NameSet},
        ast as N, legacy_aliases,
    },
    parser::{
        ast::{
            self as P, DatatypeName, Field, ModuleName, NameAccess, NameAccessChain,
            NameAccessChain_, NamePath, PathEntry, Type, VariantName,
        },
        syntax::make_loc,
    },
    shared::{string_utils::a_article_prefix, unique_map::UniqueMap, *},
    FullyCompiledProgram,
};

use move_ir_types::location::{sp, Loc, Spanned};
use move_symbol_pool::Symbol;

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::naming::{
    alias_map_builder::UnnecessaryAlias,
    aliases::NameMapKind,
    ast::{BlockLabel, BuiltinFunction, NominalBlockUsage},
};

//**************************************************************************************************
// Resolution Results
//**************************************************************************************************

#[derive(Clone)]
pub struct ResolvedMemberFunction {
    pub module: ModuleIdent,
    pub name: Name,
    pub tyarg_arity: usize,
    pub arity: usize,
}

#[derive(Clone)]
pub struct ResolvedStruct {
    pub module: ModuleIdent,
    pub name: Name,
    pub decl_loc: Loc,
    pub tyarg_arity: usize,
    pub field_info: FieldInfo,
}

#[derive(Clone)]
pub struct ResolvedEnum {
    pub module: ModuleIdent,
    pub name: Name,
    pub decl_loc: Loc,
    pub tyarg_arity: usize,
    pub variants: UniqueMap<VariantName, ResolvedVariant>,
}

#[derive(Clone)]
pub struct ResolvedVariant {
    pub module: ModuleIdent,
    pub enum_name: DatatypeName,
    pub tyarg_arity: usize,
    pub name: Name,
    pub decl_loc: Loc,
    pub field_info: FieldInfo,
}

#[derive(Clone)]
pub enum FieldInfo {
    Positional(usize),
    Named(BTreeSet<Field>),
    Empty,
}

#[derive(Clone)]
pub struct ResolvedConstant {
    pub module: ModuleIdent,
    pub name: Name,
    pub decl_loc: Loc,
}

#[derive(Clone)]
pub enum ResolvedDatatype {
    Struct(ResolvedStruct),
    Enum(ResolvedEnum),
}

#[derive(Clone)]
pub enum ResolvedDefinition {
    BuiltinFun(BuiltinFunction),
    BuiltinType(N::BuiltinTypeName_),
    Constant(ResolvedConstant),
    Datatype(ResolvedDatatype),
    Function(ResolvedMemberFunction),
    TypeParam(Loc, N::TParam),
    Variant(ResolvedVariant),
}

#[derive(Clone)]
pub enum ResolvedName {
    Address(E::Address),
    Module(E::ModuleIdent),
    Definition(ResolvedDefinition),
    Variable(N::Var),
}

pub struct AccessChainResult<T> {
    pub result: T,
    pub ptys_opt: Option<Spanned<Vec<P::Type>>>,
    pub is_macro: Option<Loc>,
}

//************************************************
// impls and helpers
//************************************************

impl ResolvedDefinition {
    pub fn kind(&self) -> String {
        match self {
            ResolvedDefinition::Function(_) => "function".to_string(),
            ResolvedDefinition::Datatype(_) => "datatype".to_string(),
            ResolvedDefinition::Constant(_) => "constant".to_string(),
            ResolvedDefinition::TypeParam(_, _) => "type parameter".to_string(),
            ResolvedDefinition::BuiltinFun(_) => "function".to_string(),
            ResolvedDefinition::BuiltinType(_) => "type".to_string(),
            ResolvedDefinition::Variant(_) => "variant".to_string(),
        }
    }
}

impl ResolvedName {
    pub fn kind(&self) -> String {
        match self {
            ResolvedName::Address(_) => "address".to_string(),
            ResolvedName::Module(_) => "module".to_string(),
            ResolvedName::Definition(defn) => defn.kind(),
            ResolvedName::Variable(v) if v.value.id == 0 => "parameter".to_string(),
            ResolvedName::Variable(v) => "local variable".to_string(),
        }
    }

    pub fn to_resolved_type(
        self,
        env: &mut CompilationEnv,
        error_loc: Loc,
    ) -> Option<ResolvedType> {
        use ResolvedDefinition as RD;
        match self {
            ResolvedName::Definition(RD::BuiltinType(btype)) => Ok(ResolvedType::ModuleType(btype)),
            ResolvedName::Definition(RD::Datatype(datatype)) => {
                Ok(ResolvedType::ModuleType(datatype))
            }
            ResolvedName::Definition(RD::TypeParam(loc, tparam)) => {
                Ok(ResolvedType::TParam(loc, tparam))
            }
            ResolvedName::Address(_)
            | ResolvedName::Module(_)
            | ResolvedName::Variable(_)
            | ResolvedName::Definition(
                RD::BuiltinFun(_) | RD::Constant(_) | RD::Function(_) | RD::Variant(_),
            ) => Err(unexpected_access_error(
                error_loc,
                self.kind(),
                Access::Type,
            )),
        }
    }

    pub fn to_resolved_term(
        self,
        env: &mut CompilationEnv,
        error_loc: Loc,
    ) -> Option<ResolvedTerm> {
        use ResolvedDefinition as RD;
        match self {
            ResolvedName::Definition(RD::Constant(c)) => Some(ResolvedTerm::Constant(c)),
            ResolvedName::Definition(RD::Variant(v)) => Some(ResolvedTerm::Variant(v)),
            ResolvedName::Variable(x) => Some(ResolvedTerm::Variable(x)),
            ResolvedName::Definition(
                RD::BuiltinFun(_)
                | RD::BuiltinType(_)
                | RD::Datatype(_)
                | RD::Function(_)
                | RD::TypeParam(_, _),
            )
            | ResolvedName::Address(_)
            | ResolvedName::Module(_) => {
                env.add_diag(unexpected_access_error(
                    error_loc,
                    self.kind(),
                    Access::Term,
                ));
                None
            }
        }
    }

    pub fn to_resolved_call_subject(
        self,
        env: &mut CompilationEnv,
        error_loc: Loc,
    ) -> Option<ResolvedCallSubject> {
        use ResolvedDefinition as RD;
        match self {
            ResolvedName::Definition(RD::BuiltinFun(bfun)) => {
                Some(ResolvedCallSubject::Builtin(bfun))
            }
            ResolvedName::Definition(RD::Datatype(ResolvedDatatype::Struct(struct_))) => {
                Some(ResolvedCallSubject::Struct(struct_))
            }
            ResolvedName::Definition(RD::Function(fun)) => {
                Some(ResolvedCallSubject::MemberFunction(fun))
            }
            ResolvedName::Definition(RD::Variant(v)) => Some(ResolvedCallSubject::Variant(v)),
            ResolvedName::Variable(x) => Ok(ResolvedTerm::Variable(x)),
            ResolvedName::Definition(
                RD::Constant(_) | RD::BuiltinType(_) | RD::Datatype(_) | RD::TypeParam(_, _),
            )
            | ResolvedName::Address(_)
            | ResolvedName::Module(_) => {
                env.add_diag(unexpected_access_error(
                    error_loc,
                    self.kind(),
                    Access::ApplyPositional,
                ));
                None
            }
        }
    }

    pub fn to_resolved_constructor(
        self,
        env: &mut CompilationEnv,
        error_loc: Loc,
    ) -> Option<ResolvedConstructor> {
        use ResolvedDefinition as RD;
        match self {
            ResolvedName::Definition(RD::Datatype(ResolvedDatatype::Struct(struct_))) => {
                Some(ResolvedConstructor::Struct(struct_))
            }
            ResolvedName::Definition(RD::Variant(variant)) => {
                Some(ResolvedConstructor::Variant(variant))
            }
            ResolvedName::Definition(
                RD::BuiltinType(_)
                | RD::BuiltinFun(_)
                | RD::Constant(_)
                | RD::Datatype(ResolvedDatatype::Enum(_))
                | RD::Function(_)
                | RD::TypeParam(_, _),
            )
            | ResolvedName::Address(_)
            | ResolvedName::Module(_)
            | ResolvedName::Variable(_) => {
                env.add_diag(unexpected_access_error(
                    error_loc,
                    self.kind(),
                    Access::ApplyNamed,
                ));
                None
            }
        }
    }
}

impl Into<ResolvedName> for ResolvedDefinition {
    fn into(self) -> ResolvedName {
        ResolvedName::Definition(self)
    }
}

impl Into<ResolvedName> for E::ModuleIdent {
    fn into(self) -> ResolvedName {
        ResolvedName::Module(self)
    }
}

impl Into<ResolvedName> for E::Address {
    fn into(self) -> ResolvedName {
        ResolvedName::Address(self)
    }
}

impl Into<ResolvedName> for N::Var {
    fn into(self) -> ResolvedName {
        ResolvedName::Variable(self)
    }
}

macro_rules! access_result {
    ($result:pat, $ptys_opt:pat, $is_macro:pat) => {
        AccessChainResult {
            result: $result,
            ptys_opt: $ptys_opt,
            is_macro: $is_macro,
        }
    };
}

pub(crate) use access_result;

fn make_access_result<T>(
    result: T,
    ptys_opt: Option<Spanned<Vec<P::Type>>>,
    is_macro: Option<Loc>,
) -> AccessChainResult<T> {
    AccessChainResult {
        result,
        ptys_opt,
        is_macro,
    }
}

//**************************************************************************************************
// Module Index
//**************************************************************************************************

pub type ModuleMembers = BTreeMap<ModuleIdent, BTreeMap<Name, ResolvedDefinition>>;

pub fn build_member_map(
    pre_compiled_lib: Option<Arc<FullyCompiledProgram>>,
    prog: &E::Program,
) -> (BTreeSet<ModuleIdent>, ModuleMembers) {
    use ResolvedDefinition as D;
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
    let mut module_names = BTreeSet::new();
    let mut all_members = BTreeMap::new();
    for (module, mdef) in all_modules {
        module_names.insert(module);
        let mut members = BTreeMap::new();
        for (nloc, name, fun) in mdef.functions.iter() {
            let tyarg_arity = fun.signature.type_parameters.len();
            let arity = fun.signature.parameters.len();
            let fun_def = ResolvedMemberFunction {
                module,
                name,
                tyarg_arity,
                arity,
            };
            assert!(members.insert(name, D::Function(fun_def)).is_none());
        }
        for (decl_loc, name, _) in mdef.constants.iter() {
            let const_def = ResolvedConstant {
                module,
                name,
                decl_loc,
            };
            assert!(members.insert(name, D::Constant(const_def)).is_none());
        }
        for (decl_loc, name, sdef) in mdef.structs.iter() {
            let tyarg_arity = sdef.type_parameters.len();
            let field_info = match &sdef.fields {
                E::StructFields::Positional(fields) => FieldInfo::Positional(fields.len()),
                E::StructFields::Named(f) => {
                    FieldInfo::Named(f.key_cloned_iter().map(|(k, _)| k).collect())
                }
                E::StructFields::Native(_) => FieldInfo::Empty,
            };
            let struct_def = ResolvedStruct {
                module,
                name,
                decl_loc,
                tyarg_arity,
                field_info,
            };
            assert!(members.insert(name, D::Struct(struct_def)).is_none());
        }
        for (decl_loc, enum_name, edef) in mdef.enums.iter() {
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
                    module,
                    enum_name,
                    tyarg_arity,
                    name: name.value(),
                    decl_loc: v.loc,
                    field_info,
                }
            });
            let decl_loc = edef.loc;
            let enum_def = ResolvedEnum {
                module,
                name: enum_name,
                decl_loc,
                tyarg_arity,
                variants,
            };
            assert!(members.insert(enum_name, D::Enum(enum_def)).is_none());
        }
        all_members.insert(module, members);
    }
    (module_names, all_members);
}

//**************************************************************************************************
// Base Resolver Definitions
//**************************************************************************************************

pub(super) enum ResolvedType {
    ModuleType(Box<ResolvedDatatype>),
    TParam(Loc, N::TParam),
    BuiltinType(N::BuiltinTypeName_),
    Hole, // '_' type
}

pub(super) enum ResolvedTerm {
    Constant(ResolvedConstant),
    Variable(N::Var),
    Variant(ResolvedVariant),
}

/// Similar to a ResolvedTerm, but with different resolution rules around variables and support for
/// wildcards.
pub(super) enum ResolvedPatternName {
    Constant(ResolvedConstant),
    Variable(N::Var),
    Variant(ResolvedVariant),
    Wildcard,
}

/// A resolved LValue. This Can contain variables, unbound names (such as when a new binding is
/// occuring), and wildcards.
pub(super) enum ResolvedLValueName {
    Variable(N::Var),
    UnresolvedName(Name),
    Wildcard,
}

pub(super) enum ResolvedCallSubject {
    Builtin(BuiltinFunction),
    MemberFunction(ResolvedMemberFunction),
    Struct(ResolvedStruct),
    Variable(N::Var),
    Variant(ResolvedVariant),
}

pub(super) enum ResolvedConstructor {
    Variant(ResolvedVariant),
    Struct(ResolvedStruct),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Type,
    ApplyNamed,
    ApplyPositional,
    Term,
    PatternBinder,
    Module,
}

#[derive(Debug, PartialEq, Eq)]
enum AccessChainNameResult {
    ResolvedName(Loc, ResolvedName),
    UnresolvedName(Loc, Name),
    ResolutionFailure(Box<AccessChainNameResult>, AccessChainFailure),
    LegacyError(Diagnostic),
}

#[derive(Debug, PartialEq, Eq)]
enum AccessChainFailure {
    UnresolvedAlias(Name),
    UnknownModuleDefinition(Address, Name),
    UnknownMemberDefinition(Access, ModuleIdent, Name),
    UnknownVariantDefinition(ResolvedEnum, Name),
    InvalidKind(String),
}

macro_rules! path_entry {
    ($name:pat, $tyargs:pat, $is_macro:pat) => {
        PathEntry {
            name: $name,
            tyargs: $tyargs,
            is_macro: $is_macro,
        }
    };
}

macro_rules! single_entry {
    ($name:pat, $tyargs:pat, $is_macro:pat) => {
        P::NameAccessChain_::Single(path_entry!($name, $tyargs, $is_macro))
    };
}

pub trait NameResolver {
    /// Get the compilation env
    fn env(&mut self) -> &mut CompilationEnv;

    /// Resolve a `NameAccessChain` to an `AccessChainResult<AccessChainNameResult>`, possibly
    /// containing an error.
    fn resolve_name_access_chain(
        access: Access,
        name: NameAccessChain,
    ) -> AccessChainResult<AccessChainNameResult>;

    /// Resolve a module `m`
    fn resolve_module(&mut self, name: NameAccessChain) -> Option<ModuleIdent>;

    /// Indicate is a given module identifier is defined in the list of known module identifiers.
    fn validate_mident(&mut self, mident: &ModuleIdent);

    // -- ALIAS SCOPES  --

    fn new_alias_map_builder(&mut self, kind: NameMapKind) -> AliasMapBuilder;

    /// Push a new innermost alias scope
    fn push_alias_scope(
        &mut self,
        loc: Loc,
        new_scope: AliasMapBuilder,
    ) -> Result<Vec<UnnecessaryAlias>, Box<Diagnostic>>;

    /// Push a number of type parameters onto the alias information in the path expander. They are
    /// never resolved, but are tracked to apply appropriate shadowing.
    fn push_type_parameters(&mut self, tparams: Vec<(Name, N::TParam)>);

    /// Pop the innermost alias scope
    fn pop_alias_scope(&mut self, expected_kind: Option<NameMapKind>) -> NameSet;

    // -- REUSED RESOLUTION DEFINITIONS --

    /// Resolve a name for a type `: T`
    fn resolve_type(&mut self, name: NameAccessChain) -> Option<AccessChainResult<ResolvedType>> {
        use ResolvedType as RT;
        let access_result!(result, ptys_opt, is_macro) =
            self.resolve_name_access_chain(Access::Type, name);
        let ty = match result {
            AccessChainNameResult::ResolvedName(loc, defn) => {
                defn.to_resolved_type(self.core.env, loc)
            }
            AccessChainNameResult::UnresolvedName(nloc, name) if name.value.as_str() == "_" => {
                self.core.env.check_feature(
                    self.core.current_package,
                    FeatureGate::TypeHoles,
                    nloc,
                );
                Some(RT::Hole)
            }
            AccessChainNameResult::UnresolvedName(_, name) => {
                self.core.env.add_diag(unbound_type_error(name));
                None
            }
            AccessChainNameResult::ResolutionFailure(_, _) => {
                self.core
                    .env
                    .add_diag(access_chain_resolution_error(result));
                None
            }
            AccessChainNameResult::LegacyError(_, _, _) => unreachable!(),
        };
        ty.map(|result| AccessChainResult {
            result,
            ptys_opt,
            is_macro,
        })
    }

    /// Resolve a name for a term `x`
    fn resolve_term(&mut self, name: NameAccessChain) -> Option<AccessChainResult<ResolvedTerm>> {
        let (code, error_msg) = match &name.value {
            P::NameAccessChain_::Single(entry) => {
                if is_valid_datatype_or_constant_name(entry.name) {
                    (NameResolution::UnboundModuleMember, "constant")
                } else {
                    (NameResolution::UnboundVariable, "variable")
                }
            }
            P::NameAccessChain_::Path(entries) => {
                (NameResolution::UnboundModuleMember, "constant or variant")
            }
        };
        let access_result!(result, ptys_opt, is_macro) =
            self.resolve_name_access_chain(Access::Term, name);
        let loc = result.loc();
        result
            .resolve_or_report(self.core.env, |loc, name| {
                unbound_term_error(name, code, error_msg)
            })
            .to_resolved_term(self.core.env, loc)
            .map(|result| AccessChainResult {
                result,
                ptys_opt,
                is_macro,
            })
    }

    /// Resolve a name for a pattern name `x`
    /// Similar to a resolve_term, but with different resolution rules around variable when
    /// resolving local names.
    fn resolve_pattern_name(
        &mut self,
        name: NameAccessChain,
    ) -> Option<AccessChainResult<ResolvedPatternName>> {
        use ResolvedPatternName as RP;
        let (code, error_msg) = match &name.value {
            P::NameAccessChain_::Single(entry) => {
                if is_valid_datatype_or_constant_name(entry.name) {
                    (NameResolution::UnboundModuleMember, "constant")
                } else {
                    (NameResolution::UnboundVariable, "variable")
                }
            }
            P::NameAccessChain_::Path(entries) => {
                (NameResolution::UnboundModuleMember, "constant or variant")
            }
        };
        let access_result!(result, ptys_opt, is_macro) =
            self.resolve_name_access_chain(Access::PatternBinder, name);
        let pat = match result {
            AccessChainNameResult::ResolvedName(loc, defn) => defn
                .to_resolved_term(self.core.env, loc)
                .map(|term| match term {
                    ResolvedTerm::Constant(const_) => RP::Constant(const_),
                    ResolvedTerm::Variable(x) => RP::Variable(x),
                    ResolvedTerm::Variant(variant) => RP::Variant(variant),
                }),
            AccessChainNameResult::UnresolvedName(nloc, name) if name.value.as_str() == "_" => {
                Some(RP::Wildcard)
            }
            AccessChainNameResult::UnresolvedName(_, name) => {
                // Since binders are added to the environment before pattern resolution, this
                // should never happen.
                self.core
                    .env
                    .add_diag(ice!(name.loc, "Failed to find this pattern binder"));
                None
            }
            AccessChainNameResult::ResolutionFailure(_, _) => {
                self.core
                    .env
                    .add_diag(access_chain_resolution_error(result));
                None
            }
            AccessChainNameResult::LegacyError(_, _, _) => unreachable!(),
        };
        let loc = result.loc();
        pat.map(|result| AccessChainResult {
            result,
            ptys_opt,
            is_macro,
        })
    }

    /// Resolve a name for an lvalue name `x`
    fn resolve_lvalue_name(&mut self, name: NameAccessChain) -> Option<ResolvedLValueName> {
        use ResolvedDefinition as RD;
        use ResolvedLValueName as RL;
        let nloc = name.loc;
        let nstr = format!("{}", name);
        // `Access::Term` will treat these like variables first, which is what we want.
        let access_result!(result, mut ptys_opt, is_macro) =
            self.resolve_name_access_chain(Access::Term, name);
        if is_macro.is_some() {
            let msg = "Unexpected assignment of name with macro invocation";
            let mut diag = diag!(Syntax::InvalidLValue, (nloc, msg));
            diag.add_note("Macro invocation '!' must appear on an invocation");
            self.env().add_diag(diag);
        }
        if ptys_opt.is_some() {
            let msg = "Unexpected assignment of instantiated type without fields";
            let mut diag = diag!(Syntax::InvalidLValue, (nloc, msg));
            diag.add_note(format!(
                "If you are trying to unpack a struct, try adding fields, e.g.'{} {{}}'",
                nstr
            ));
            self.env().add_diag(diag);
        }
        let lvalue = match result {
            AccessChainNameResult::ResolvedName(loc, defn) => match defn {
                ResolvedName::Address(_)
                | ResolvedName::Module(_)
                | ResolvedName::Definition(
                    RD::BuiltinFun(_)
                    | RD::BuiltinType(_)
                    | RD::Datatype(ResolvedDatatype::Enum(_))
                    | RD::Function(_)
                    | RD::TypeParam(_, _),
                ) => {
                    self.env().add_diag(unexpected_access_msg_error(
                        loc,
                        defn.kind(),
                        "assignment target",
                    ));
                    None
                }
                // carve-out for a better error
                ResolvedName::Definition(RD::Datatype(ResolvedDatatype::Struct(struct_))) => {
                    let msg = "Unexpected assignment to struct with no field";
                    let mut diag = diag!(Syntax::InvalidLValue, (loc, msg));
                    diag.add_note(format!(
                        "If you are trying to unpack a struct, try adding fields, e.g.'{} {{}}'",
                        name
                    ));
                    self.env().add_diag(diag);
                    None
                }
                // carve-out for a better error
                ResolvedName::Definition(RD::Datatype(ResolvedDatatype::Variant())) => {
                    let cur_pkg = self.get_core_resolver().current_package;
                    if self.env().check_feature(cur_pkg, FeatureGate::Enums, loc) {
                        let msg = "Unexpected assignment of variant";
                        let mut diag = diag!(Syntax::InvalidLValue, (loc, msg));
                        diag.add_note("If you are trying to unpack an enum variant, use 'match'");
                        self.env().add_diag(diag);
                        None
                    } else {
                        assert!(self.env().has_errors());
                        None
                    }
                }
                ResolvedName::Variable(x) => Some(RL::Variable(x)),
            },
            AccessChainNameResult::UnresolvedName(nloc, name) if name.value.as_str() == "_" => {
                Some(RL::Wildcard)
            }
            AccessChainNameResult::UnresolvedName(_, name) => Some(RL::UnresolvedName(name)),
            AccessChainNameResult::ResolutionFailure(_, _) => {
                self.core
                    .env
                    .add_diag(access_chain_resolution_error(result));
                None
            }
            AccessChainNameResult::LegacyError(_, _, _) => unreachable!(),
        };
        let loc = result.loc();
        lvalue.map(|result| AccessChainResult {
            result,
            ptys_opt,
            is_macro,
        })
    }

    /// Resolve a name for a call subject `f(...)`
    fn resolve_call_subject(
        &mut self,
        name: NameAccessChain,
    ) -> Option<AccessChainResult<ResolvedCallSubject>> {
        let access_result!(result, ptys_opt, is_macro) =
            self.resolve_name_access_chain(Access::ApplyPositional, name);
        let loc = result.loc();
        result
            .resolve_or_report(self.core.env, |loc, name| unbound_function_error(name))
            .to_resolved_call_subject(self.core.env, loc)
            .map(|result| AccessChainResult {
                result,
                ptys_opt,
                is_macro,
            })
    }

    /// Resolve a constructor `D { .. }`
    fn resolve_constructor(
        &mut self,
        name: NameAccessChain,
    ) -> Option<AccessChainResult<ResolvedConstructor>> {
        let access_result!(result, ptys_opt, is_macro) =
            self.resolve_name_access_chain(Access::ApplyNamed, name);
        let loc = result.loc();
        result
            .resolve_or_report(self.core.env, |loc, name| unbound_function_error(name))
            .to_resolved_constructor(self.core.env, loc)
            .map(|result| AccessChainResult {
                result,
                ptys_opt,
                is_macro,
            })
    }

    // -- CORE NAME RESOLVER --

    // This allows us to implement the behavior below for all derivations of this trait.
    fn get_core_resolver(&mut self) -> &mut CoreNameResolver;

    // -- ENTER/EXIT METHODS --

    /// Enter a package
    fn enter_package(&mut self, package: Symbol) {
        self.get_core_resolver().enter_package(package);
    }

    fn exit_package(&mut self) {
        self.get_core_resolver().exit_package();
    }

    fn enter_module(&mut self, module: ModuleIdent) {
        self.get_core_resolver().enter_module(module);
    }

    fn exit_module(&mut self) {
        self.get_core_resolver().exit_module();
    }

    fn enter_function(&mut self, type_params: Vec<(Name, AbilitySet)>) -> Vec<N::TParam> {
        self.get_core_resolver().enter_function()
    }

    fn exit_function(&mut self) {
        self.get_core_resolver().exit_function()
    }

    // -- LOCAL SCOPES  --

    /// Enter a new local scope
    fn enter_local_scope(&mut self) {
        self.get_core_resolver().enter_local_scope()
    }

    /// Exit a local scope
    fn exit_local_scope(&mut self) {
        self.get_core_resolver().exit_local_scope()
    }

    /// Declare a new local
    fn declare_local(&mut self, is_parameter: bool, name: Name) -> N::Var {
        self.get_core_resolver().declare_local(is_parameter, name)
    }

    /// Like resolve_local, but with an ICE on failure because these are precomputed and should
    /// always exist. This also does not mark usage, as this is ostensibly the binding form.
    /// This is so that we can walk through or-patterns and reuse the bindings for both sides.
    fn resolve_pattern_binder(&mut self, name: Name) -> Option<N::Var> {
        self.get_core_resolver().resolve_pattern_binder(name)
    }

    // -- TYPES --

    // -- TYPE PARAMETERS --

    fn fun_type_parameters(&mut self, type_parameters: Vec<(Name, AbilitySet)>) -> Vec<N::TParam>;

    fn datatype_type_parameters(
        &mut self,
        type_parameters: Vec<E::DatatypeTypeParameter>,
    ) -> Vec<N::DatatypeTypeParameter>;

    fn bind_type_param(&mut self, name: Name, ty: ResolvedType);

    fn type_parameter(
        &mut self,
        unique_tparams: &mut UniqueMap<Name, ()>,
        name: Name,
        abilities: AbilitySet,
    ) -> N::TParam;

    // -- BLOCK SCOPES  --

    /// Enter a nominal block
    fn enter_nominal_block(
        &mut self,
        loc: Loc,
        name: Option<P::BlockLabel>,
        name_type: NominalBlockType,
    ) {
        self.get_core_resolver()
            .enter_nominal_block(loc, name, name_type)
    }

    /// Exit a nominal block
    fn exit_nominal_block(&mut self) -> (BlockLabel, NominalBlockType) {
        self.get_core_resolver().exit_nominal_block()
    }

    /// Get the current loop
    fn current_loop(&self, loc: Loc, usage: NominalBlockUsage) -> Option<BlockLabel> {
        self.get_core_resolver().current_loop(loc, usage)
    }

    /// Get the current continue
    fn current_continue(&mut self, loc: Loc) -> Option<BlockLabel> {
        self.get_core_resolver().current_continue(loc)
    }

    /// Get the current break
    fn current_break(&mut self, loc: Loc) -> Option<BlockLabel> {
        self.get_core_resolver().current_break(loc)
    }

    /// Get the current return
    fn current_return(&self, _loc: Loc) -> Option<BlockLabel> {
        self.get_core_resolver().current_return(_loc)
    }

    /// Resolve the block label
    fn resolve_nominal_label(
        &self,
        usage: NominalBlockUsage,
        label: P::BlockLabel,
    ) -> Option<BlockLabel> {
        self.get_core_resolver().resolve_nominal_label(usage, label)
    }
}

impl Access {
    fn case(&self) -> &'static str {
        match self {
            Access::Type | Access::ApplyNamed => "type",
            Access::ApplyPositional => "expression",
            Access::Term => "expression",
            Access::PatternBinder => "pattern constructor",
            Access::Module => "module",
        }
    }
}

impl AccessChainNameResult {
    fn resolve_or_report(
        self,
        env: &mut CompilationEnv,
        unresolved_error: dyn FnOnce(Loc, Name) -> Diagnostic,
    ) -> Option<(Loc, ResolvedName)> {
        match self {
            AccessChainNameResult::ResolvedName(loc, name) => Some((loc, name)),
            AccessChainNameResult::UnresolvedName(loc, name) => {
                env.add_diag(unresolved_error(loc, name));
                None
            }
            AccessChainNameResult::ResolutionFailure(_, _) => {
                env.add_diag(access_chain_resolution_error(self));
                None
            }
            AccessChainNameResult::LegacyError(error) => {
                env.add_diag(error);
                None
            }
        }
    }

    fn loc(&self) -> Loc {
        use AccessChainNameResult as AR;
        match self {
            AR::ResolvedName(loc, _) => *loc,
            AR::UnresolvedName(loc, _) => *loc,
            AR::ResolutionFailure(inner, _) => inner.loc(),
            AR::LegacyError(_) => unreachable!(),
        }
    }

    fn name(&self) -> String {
        use AccessChainNameResult as AR;
        match self {
            AR::ResolvedName(_, name) => name.kind(),
            AR::UnresolvedName(_, _) => "name".to_string(),
            AR::ResolutionFailure(inner, _) => inner.err_name(),
            AR::LegacyError(_) => unreachable!(),
        }
    }

    fn err_name(&self) -> String {
        a_article_prefix(self.name())
    }
}

impl ResolvedConstructor {
    pub fn module(&self) -> ModuleIdent {
        match self {
            ResolvedConstructor::Variant(v) => v.module,
            ResolvedConstructor::Struct(s) => s.module,
        }
    }

    pub fn type_name(&self) -> Name {
        match self {
            ResolvedConstructor::Variant(v) => v.enum_name,
            ResolvedConstructor::Struct(s) => s.name,
        }
    }

    pub fn type_arity(&self) -> Name {
        match self {
            ResolvedConstructor::Variant(v) => v.tyarg_arity,
            ResolvedConstructor::Struct(s) => s.tyarg_arity,
        }
    }

    pub fn field_info(&self) -> FieldInfo {
        match self {
            ResolvedConstructor::Variant(v) => v.field_info,
            ResolvedConstructor::Struct(s) => s.field_info,
        }
    }
}

//**************************************************************************************************
// Core Resolver Implementation Definitions
//**************************************************************************************************

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

/// This serves as a sort of "core" name resolver, which is handed in to per-package name
/// resolvers based on syntax feature selection.
pub struct CoreNameResolver<'env, 'member> {
    pub env: &'env mut CompilationEnv,
    pub module_members: &'member ModuleMembers,
    pub current_package: Option<Symbol>,
    pub current_module: Option<ModuleIdent>,
    local_scopes: Vec<BTreeMap<Symbol, u16>>,
    local_count: BTreeMap<Symbol, u16>,
    used_locals: BTreeSet<N::Var_>,
    used_tparams: BTreeSet<N::TParamID>,
    nominal_blocks: Vec<(Option<Symbol>, BlockLabel, NominalBlockType)>,
    nominal_block_id: u16,
}

impl CoreNameResolver<'_, '_> {
    pub fn new<'env, 'member>(
        env: &'env mut CompilationEnv,
        module_members: &'member ModuleMembers,
    ) -> CoreNameResolver<'env, 'member> {
        CoreNameResolver {
            env,
            module_members,
            current_package: None,
            current_module: None,
            local_scopes: vec![],
            local_count: BTreeMap::new(),
            used_locals: BTreeSet::new(),
            used_tparams: BTreeSet::new(),
            nominal_blocks: vec![],
            nominal_block_id: 0,
        }
    }

    pub fn enter_package(&mut self, package: Symbol) {
        self.current_package = Some(package);
    }

    pub fn exit_package(&mut self) {
        self.current_package = None;
    }

    pub fn enter_module(&mut self, module: ModuleIdent) {
        self.current_module = Some(module);
    }

    pub fn exit_module(&mut self) {
        self.current_module = None;
    }

    fn enter_function(&mut self, type_parameters: Vec<(Name, AbilitySet)>) -> Vec<N::TParam> {
        assert!(self.nominal_blocks.is_empty());
        assert!(self.local_scopes.is_empty());
        assert!(self.nominal_block_id = 0);
        assert!(self.type_parameters.is_empty());
        self.fun_type_parameters(type_parameters)
    }

    fn exit_function(&mut self) {
        assert!(self.nominal_blocks.is_empty());
        assert!(self.local_scopes.is_empty());
        self.check_type_parameters();
        self.nominal_block_id = 0;
    }

    // -- LOCALS --

    fn enter_local_scope(&mut self) {
        self.local_scopes.push(BTreeMap::new());
    }

    fn exit_local_scope(&mut self) {
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

    fn resolve_local<S: ToString>(&mut self, sp!(vloc, name): Name) -> Option<N::Var> {
        let id_opt = self
            .local_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(&name).copied());
        match id_opt {
            None => None,
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
    fn resolve_pattern_binder(&mut self, sp!(vloc, name): Name) -> Option<N::Var> {
        let id_opt = self.local_scopes.last().unwrap().get(&name).copied();
        match id_opt {
            None => {
                let msg = format!("Failed to resolve pattern binder {}", name);
                self.env.add_diag(ice!((vloc, msg)));
                None
            }
            Some(id) => {
                let nvar_ = N::Var_ { name, id, color: 0 };
                Some(sp(vloc, nvar_))
            }
        }
    }

    // -- NOMINAL BLOCKS --

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
        let block_label = BlockLabel::new(loc, name, id);
        self.nominal_blocks.push((name, block_label, name_type));
    }

    fn current_loop(&self, loc: Loc, usage: NominalBlockUsage) -> Option<BlockLabel> {
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
            self.env
                .add_diag(diag!(TypeSafety::InvalidLoopControl, (loc, msg)));
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
            self.env.add_diag(diag);
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
        &self,
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
                self.env.add_diag(diag);
                None
            }
        } else {
            let msg = format!("Invalid {usage}. Unbound label '{name}");
            self.env
                .add_diag(diag!(NameResolution::UnboundLabel, (loc, msg)));
            None
        }
    }

    fn exit_nominal_block(&mut self) -> (BlockLabel, NominalBlockType) {
        let (_name, label, name_type) = self.nominal_blocks.pop().unwrap();
        (label, name_type)
    }

    // -- TYPES --

    // -- TYPE PARAMETERS --

    fn fun_type_parameters(&mut self, type_parameters: Vec<(Name, AbilitySet)>) -> Vec<N::TParam> {
        let mut unique_tparams = UniqueMap::new();
        type_parameters
            .into_iter()
            .map(|(name, abilities)| self.type_parameter(&mut unique_tparams, name, abilities))
            .collect()
    }

    fn datatype_type_parameters(
        &mut self,
        type_parameters: Vec<E::DatatypeTypeParameter>,
    ) -> Vec<N::DatatypeTypeParameter> {
        let mut unique_tparams = UniqueMap::new();
        type_parameters
            .into_iter()
            .map(|param| {
                let is_phantom = param.is_phantom;
                let param = self.type_parameter(&mut unique_tparams, param.name, param.constraints);
                N::DatatypeTypeParameter { param, is_phantom }
            })
            .collect()
    }

    fn bind_type_param(&mut self, name: Name, ty: ResolvedType) {
        self.type_parameters.insert(name, (false, ty));
    }

    fn type_parameter(
        &mut self,
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
        self.bind_type_param(name.value, ResolvedType::TParam(loc, tp.clone()));
        if let Err((name, old_loc)) = unique_tparams.add(name, ()) {
            let msg = format!("Duplicate type parameter declared with name '{}'", name);
            self.env.add_diag(diag!(
                Declarations::DuplicateItem,
                (loc, msg),
                (old_loc, "Type parameter previously defined here"),
            ))
        }
        tp
    }

    fn validate_mident(&mut self, mident: &ModuleIdent) {
        ice_assert!(
            self.env,
            self.module_members.contains_key(&mident),
            mident.loc,
            "Resolved name to unbound module identifier"
        );
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
// Move 2024 Resolver
//**************************************************************************************************

pub struct Move2024NameResolver<'env, 'member, 'addr> {
    core: CoreNameResolver<'env, 'member>,
    named_address_map: &'addr NamedAddressMap,
    aliases: NameMap,
}

impl Move2024NameResolver<'_, '_, '_> {
    pub fn new<'env, 'member, 'addr>(
        core: CoreNameResolver<'env, 'member>,
        named_address_map: &'addr NamedAddressMap,
    ) -> Move2024NameResolver<'env, 'member, 'addr> {
        let aliases = named_addr_map_to_alias_map_builder(named_address_map);
        Move2024NameResolver {
            core,
            aliases,
            named_address_map,
        }
    }

    fn resolve_root(
        &mut self,
        access: Access,
        sp!(loc, name): P::LeadingNameAccess,
    ) -> AccessChainNameResult {
        use AccessChainFailure as NF;
        use AccessChainNameResult as NR;
        use P::LeadingNameAccess_ as LN;
        match name {
            LN::AnonymousAddress(address) => NR::Address(loc, E::Address::anonymous(loc, address)),
            LN::GlobalAddress(name) => {
                if let Some(address) = self
                    .named_address_mapping
                    .expect("ICE no named address mapping")
                    .get(&name.value)
                {
                    NR::Address(loc, make_new_address(name, name.loc, *address))
                } else {
                    NR::ResolutionFailure(
                        Box::new(NR::UnresolvedName(loc, name)),
                        NF::UnresolvedAlias(name),
                    )
                }
            }
            LN::Name(name) => match self.resolve_name(access, NameSpace::LeadingAccess, name) {
                result @ NR::UnresolvedName(_, _) => {
                    NR::ResolutionFailure(Box::new(result), NF::UnresolvedAlias(name))
                }
                other => other,
            },
        }
    }

    fn resolve_name(
        &mut self,
        access: Access,
        namespace: NameSpace,
        name: Name,
    ) -> AccessChainNameResult {
        use AccessChainFailure as NF;
        use AccessChainNameResult as NR;

        match access {
            Access::Term if !is_valid_datatype_or_constant_name(name) => {
                if let Some(local) = self.core.resolve_local(name) {
                    NR::ResolvedName(name.loc, local.into())
                } else {
                    NR::UnresolvedName(name.loc, name)
                }
            }
            Access::PatternBinder if !is_valid_datatype_or_constant_name(name) => {
                if let Some(local) = self.core.resolve_pattern_binder(name) {
                    NR::ResolvedName(name.loc, local.into())
                } else {
                    NR::UnresolvedName(name.loc, name)
                }
            }
            _ => {
                match self.aliases.resolve(namespace, &name) {
                    Some(AliasEntry::Definition(_, entry)) => {
                        // We are preserving the name's original location, rather than referring to where
                        // the alias was defined. The name represents JUST the member name, though, so we do
                        // not change location of the module as we don't have this information.
                        // TODO maybe we should also keep the alias reference (or its location)?
                        NR::ResolvedName(name.loc, entry.into())
                    }
                    Some(AliasEntry::Module(_, mident)) => {
                        // We are preserving the name's original location, rather than referring to where
                        // the alias was defined. The name represents JUST the module name, though, so we do
                        // not change location of the address as we don't have this information.
                        // TODO maybe we should also keep the alias reference (or its location)?
                        let sp!(
                            _,
                            ModuleIdent_ {
                                address,
                                module: ModuleName(sp!(_, module))
                            }
                        ) = mident;
                        let module = ModuleName(sp(name.loc, module));
                        let mident = sp(name.loc, E::ModuleIdent_ { address, module });
                        self.core.validate_mident(&mident);
                        NR::ResolvedName(name.loc, mident.into())
                    }
                    Some(AliasEntry::Address(_, address)) => {
                        NR::ResolvedName(name.loc, make_new_address(name, name.loc, address).into())
                    }
                    None => {
                        if let Some(entry) = self.aliases.resolve_any_for_error(&name) {
                            let msg = match namespace {
                                NameSpace::ModuleMembers => {
                                    "a type, function, or constant".to_string()
                                }
                                // we exclude types from this message since it would have been caught in
                                // the other namespace
                                NameSpace::LeadingAccess => "an address or module".to_string(),
                            };
                            let result = match entry {
                                AliasEntry::Address(_, address) => {
                                    let addr = make_new_address(name, name.loc, address);
                                    NR::ResolvedName(name.loc, addr.into())
                                }
                                AliasEntry::Module(_, mident) => {
                                    self.core.validate_mident(&mident);
                                    NR::ResolvedName(name.loc, mident.into())
                                }
                                AliasEntry::Definition(_, entry) => {
                                    NR::ResolvedName(name.loc, entry.into())
                                }
                            };
                            NR::ResolutionFailure(Box::new(result), NF::InvalidKind(msg))
                        } else {
                            NR::UnresolvedName(name.loc, name)
                        }
                    }
                }
            }
        }

        match self.aliases.resolve(namespace, &name) {
            Some(AliasEntry::Definition(_, entry)) => {
                // We are preserving the name's original location, rather than referring to where
                // the alias was defined. The name represents JUST the member name, though, so we do
                // not change location of the module as we don't have this information.
                // TODO maybe we should also keep the alias reference (or its location)?
                NR::ResolvedName(name.loc, entry.into())
            }
            Some(AliasEntry::Module(_, mident)) => {
                // We are preserving the name's original location, rather than referring to where
                // the alias was defined. The name represents JUST the module name, though, so we do
                // not change location of the address as we don't have this information.
                // TODO maybe we should also keep the alias reference (or its location)?
                let sp!(
                    _,
                    ModuleIdent_ {
                        address,
                        module: ModuleName(sp!(_, module))
                    }
                ) = mident;
                let module = ModuleName(sp(name.loc, module));
                let mident = sp(name.loc, E::ModuleIdent_ { address, module });
                self.core.validate_mident(&mident);
                NR::ResolvedName(name.loc, mident.into())
            }
            Some(AliasEntry::Address(_, address)) => {
                NR::ResolvedName(name.loc, make_new_address(name, name.loc, address).into())
            }
            None => {
                if let Some(entry) = self.aliases.resolve_any_for_error(&name) {
                    let msg = match namespace {
                        NameSpace::ModuleMembers => "a type, function, or constant".to_string(),
                        // we exclude types from this message since it would have been caught in
                        // the other namespace
                        NameSpace::LeadingAccess => "an address or module".to_string(),
                    };
                    let result = match entry {
                        AliasEntry::Address(_, address) => {
                            let addr = make_new_address(name, name.loc, address);
                            NR::ResolvedName(name.loc, addr.into())
                        }
                        AliasEntry::Module(_, mident) => {
                            self.core.validate_mident(&mident);
                            NR::ResolvedName(name.loc, mident.into())
                        }
                        AliasEntry::Definition(_, entry) => {
                            NR::ResolvedName(name.loc, entry.into())
                        }
                    };
                    NR::ResolutionFailure(Box::new(result), NF::InvalidKind(msg))
                } else {
                    // Resolve local variables and parameters.
                    // It's a bit strange this happens last.
                    match access {
                        Access::Type | Access::ApplyNamed | Access::Module => {
                            NR::UnresolvedName(name.loc, name)
                        }
                        Access::ApplyPositional | Access::Term => {
                            if let Some(local) = self.core.resolve_local(name) {
                                NR::ResolvedName(name.loc, local.into())
                            } else {
                                NR::UnresolvedName(name.loc, name)
                            }
                        }
                        Access::PatternBinder => {
                            if let Some(local) = self.core.resolve_pattern_binder(name) {
                                NR::ResolvedName(name.loc, local.into())
                            } else {
                                NR::UnresolvedName(name.loc, name)
                            }
                        }
                    }
                }
            }
        }
    }

    fn name_access_chain_to_access_chain_rsult(
        &mut self,
        access: Access,
        sp!(loc, chain): P::NameAccessChain,
    ) -> AccessChainResult<AccessChainNameResult> {
        use AccessChainFailure as NF;
        use AccessChainNameResult as NR;
        use ResolvedName as RN;
        use P::NameAccessChain_ as PN;

        fn check_tyargs(
            env: &mut CompilationEnv,
            tyargs: &Option<Spanned<Vec<Type>>>,
            result: &NR,
        ) {
            if let NR::ResolvedName(
                _,
                RN::Address(_, _)
                | RN::Module(_, _)
                | RN::Definition(ResolvedDefinition::Variant(_)),
            ) = result
            {
                if let Some(tyargs) = tyargs {
                    let mut diag = diag!(
                        NameResolution::InvalidTypeParameter,
                        (
                            tyargs.loc,
                            format!("Cannot use type parameters on {}", result.err_name())
                        )
                    );
                    if let NR::ResolvedName(RN::Definition(ResolvedDefinition::Variant(variant))) =
                        result
                    {
                        let (mident, name, variant) =
                            (variant.module, variant.enum_name, variant.name);
                        let tys = tyargs
                            .value
                            .iter()
                            .map(|ty| format!("{}", ty.value))
                            .collect::<Vec<_>>()
                            .join(",");
                        diag.add_note(format!("Type arguments are used with the enum, as '{mident}::{name}<{tys}>::{variant}'"))
                    }
                    env.add_diag(diag);
                }
            }
        }

        fn check_is_macro(env: &mut CompilationEnv, is_macro: &Option<Loc>, result: &NR) {
            if let NR::ResolvedName(
                _,
                RN::Address(_, _)
                | RN::Module(_, _)
                | RN::Definition(ResolvedDefinition::Variant(_)),
            ) = result
            {
                if let Some(loc) = is_macro {
                    env.add_diag(diag!(
                        Syntax::InvalidMacro,
                        (
                            *loc,
                            format!("Cannot use {} as a macro invocation", result.err_name())
                        )
                    ));
                }
            }
        }

        match chain {
            PN::Single(path_entry!(name, ptys_opt, is_macro)) => {
                // use crate::naming::ast::BuiltinFunction_;
                // use crate::naming::ast::BuiltinTypeName_;
                let namespace = match access {
                    Access::Type
                    | Access::ApplyNamed
                    | Access::ApplyPositional
                    | Access::Term
                    | Access::PatternBinder => NameSpace::ModuleMembers,
                    Access::Module => NameSpace::LeadingAccess,
                };
                let result = self.resolve_name(access, namespace, name);
                AccessChainResult {
                    result,
                    ptys_opt,
                    is_macro,
                }
                // // This is a hack to let `use std::vector` play nicely with `vector`,
                // // plus preserve things like `u64`, etc.
                // let result = if !matches!(access, Access::Module)
                //     && (BuiltinFunction_::all_names().contains(&name.value)
                //         || BuiltinTypeName_::all_names().contains(&name.value))
                // {
                //     NR::UnresolvedName(name.loc, name)
                // } else {
                //     self.resolve_name(namespace, name)
                // };
                // AccessChainResult {
                //     result,
                //     ptys_opt,
                //     is_macro,
                // }
            }
            PN::Path(path) => {
                let NamePath { root, entries } = path;
                let root_result = self.resolve_root(access, root.name);
                let mut result = match &root_result {
                    // In Move Legacy, we always treated three-place names as fully-qualified.
                    // For migration mode, if we could have gotten the correct result doing so,
                    // we emit a migration change to globally-qualify that path and remediate
                    // the error.
                    result @ NR::ResolvedName(loc, ResolvedName::Module(_))
                        if entries.len() == 2
                            && self.core.env.edition(self.core.current_package)
                                == Edition::E2024_MIGRATION
                            && root.is_macro.is_none()
                            && root.tyargs.is_none() =>
                    {
                        if let Some(address) = self.core.resolve_top_level_address(root.name) {
                            self.core.env.add_diag(diag!(
                                Migration::NeedsGlobalQualification,
                                (root.name.loc, "Must globally qualify name")
                            ));
                            NR::ResolvedName(root.name.loc, address.into())
                        } else {
                            NR::ResolutionFailure(
                                Box::new(result),
                                NF::InvalidKind("an address".to_string()),
                            )
                        }
                    }
                    result => result,
                };
                let mut ptys_opt = root.tyargs;
                let mut is_macro = root.is_macro;
                check_tyargs(self.core.env, &ptys_opt, &result);
                check_is_macro(self.core.env, &is_macro, &result);

                for entry in entries {
                    match &result {
                        NR::ResolvedName(rloc, name) => {
                            let new_loc = || {
                                make_loc(
                                    rloc.file_hash(),
                                    rloc.start() as usize,
                                    entry.name.loc.end() as usize,
                                )
                            };
                            match name {
                                RN::Address(address) => {
                                    let mident =
                                        sp(loc, ModuleIdent_::new(address, ModuleName(entry.name)));
                                    if !self.core.module_members.contains_key(&mident) {
                                        result = NR::ResolutionFailure(
                                            Box::new(result),
                                            NF::UnknownModuleDefinition(
                                                mident.value.address,
                                                mident.value.module.into(),
                                            ),
                                        );
                                    };
                                    result = NR::ResolvedName(new_loc(), mident);
                                    ptys_opt = entry.tyargs;
                                    is_macro = entry.is_macro;
                                }
                                RN::Module(mident) => {
                                    let Some(module_entries) =
                                        self.core.module_members.get(&mident)
                                    else {
                                        result = NR::ResolutionFailure(
                                            Box::new(result),
                                            NF::UnknownModuleDefinition(
                                                mident.value.address,
                                                mident.value.module.into(),
                                            ),
                                        );
                                        break;
                                    };
                                    let Some(entry) = module_entries.get(&entry.name) else {
                                        result = NR::ResolutionFailure(
                                            Box::new(result),
                                            NF::UnknownMemberDefinition(access, mident, entry.name),
                                        );
                                        break;
                                    };
                                    result = NR::ResolvedName(new_loc(), entry).clone();
                                    ptys_opt = entry.tyargs;
                                    is_macro = entry.is_macro;
                                }
                                RN::Definition(defn) => match defn {
                                    ResolvedDefinition::Datatype(ResolvedDatatype::Enum(defn)) => {
                                        let Some(variant) = defn.variants.get(entry.name) else {
                                            result = NR::ResolutionFailure(
                                                Box::new(result),
                                                NF::UnknownVariantDefinition(defn, entry.name),
                                            );
                                            break;
                                        };
                                        // For a variant, we use the type args from the previous. We
                                        // check these are empty or error.
                                        check_tyargs(self.core.env, &entry.tyargs, &result);
                                        if ptys_opt.is_none() && entry.tyargs.is_some() {
                                            // This is an error, but we can try to be helpful.
                                            ptys_opt = entry.tyargs;
                                        }
                                        result = NR::ResolvedName(new_loc(), variant.clone());
                                        check_is_macro(self.core.env, &entry.is_macro, &result);
                                    }
                                    ResolvedDefinition::TypeParam(_, _)
                                    | ResolvedDefinition::Function(_)
                                    | ResolvedDefinition::Variant(_)
                                    | ResolvedDefinition::Datatype(_)
                                    | ResolvedDefinition::Constant(_)
                                    | ResolvedDefinition::BuiltinFun(_)
                                    | ResolvedDefinition::BuiltinType(_) => {
                                        result = NR::ResolutionFailure(
                                            Box::new(result),
                                            NF::InvalidKind("an enum".to_string()),
                                        );
                                        break;
                                    }
                                },
                                RN::Variable(_) => {
                                    result = NR::ResolutionFailure(Box::new(result), entry.name);
                                    break;
                                }
                            }
                        }
                        NR::UnresolvedName(_, _) => {
                            self.core
                                .env
                                .add_diag(ice!((loc, "ICE access chain expansion failed")));
                            break;
                        }
                        NR::ResolutionFailure(_, _) => break,
                        NR::LegacyError(_) => unreachable!(),
                    }
                    check_tyargs(self.core.env, &ptys_opt, &result);
                    check_is_macro(self.core.env, &is_macro, &result);
                }

                AccessChainResult {
                    result,
                    ptys_opt,
                    is_macro,
                }
            }
        }
    }
}

fn make_new_address(name: Name, loc: Loc, value: NumericalAddress) -> Address {
    Address::Numerical {
        name: Some(name),
        value: sp(loc, value),
        name_conflict: todo!(),
    }
}

fn named_addr_map_to_alias_map_builder(named_addr_map: &NamedAddressMap) -> AliasMapBuilder {
    let mut new_aliases = AliasMapBuilder::namespaced(NameMapKind::Addresses);
    for (name, addr) in named_addr_map {
        // Address symbols get dummy locations so that we can lift them to names. These should
        // always be rewritten with more-accurate information as they are used.
        new_aliases
            .add_address_alias(sp(Loc::invalid(), *name), *addr)
            .expect("ICE dupe address");
    }
    new_aliases
}

impl NameResolver for Move2024NameResolver<'_, '_, '_> {
    fn env(&mut self) -> &mut CompilationEnv {
        self.core.env
    }

    fn resolve_name_access_chain(
        &mut self,
        access: Access,
        name: NameAccessChain,
    ) -> Option<AccessChainResult<AccessChainNameResult>> {
        Some(self.name_access_chain_to_access_chain_rsult(access, name))
    }

    fn resolve_module(&mut self, name: NameAccessChain) -> Option<ModuleIdent> {
        // FIXME: check for is_macro, tyargs
        let result = self.resolve_name_access_chain(Access::Module, name);
        match &self.resolve_name_access_chain(Access::Module, name).result {
            AccessChainNameResult::ResolvedName(loc, name) => match name {
                ResolvedName::Module(mident) => Some(mident),
                name @ (ResolvedName::Address(_)
                | ResolvedName::Definition(_)
                | ResolvedName::Variable(_)) => {
                    self.core.env.add_diag(unexpected_access_error(
                        loc,
                        name.kind(),
                        Access::Module,
                    ));
                    None
                }
            },
            AccessChainNameResult::UnresolvedName(_, name) => {
                self.core.env.add_diag(unbound_module_error(name));
                None
            }
            AccessChainNameResult::ResolutionFailure(_, _) => {
                self.core
                    .env
                    .add_diag(access_chain_resolution_error(result));
                None
            }
            AccessChainNameResult::LegacyError(_) => unreachable!(),
        }
    }

    fn new_alias_map_builder(&mut self, kind: NameMapKind) -> AliasMapBuilder {
        AliasMapBuilder::namespaced(kind)
    }

    fn push_alias_scope(
        &mut self,
        loc: Loc,
        new_scope: AliasMapBuilder,
    ) -> Result<Vec<UnnecessaryAlias>, Box<Diagnostic>> {
        self.aliases.push_alias_scope(loc, new_scope)
    }

    fn push_type_parameters(&mut self, tparams: Vec<&Name>) {
        self.aliases.push_type_parameters(tparams)
    }

    fn pop_alias_scope(&mut self, expected_kind: Option<NameMapKind>) -> NameSet {
        let prev_scope = self.aliases.pop_scope();
        if let Some(kind) = expected_kind {
            if !kind == prev_scope.kind {
                // turn this into an ICE?
                panic!("Kind did not match expected");
            }
        }
    }

    fn get_core_resolver(&mut self) -> &mut CoreNameResolver {
        &mut self.core
    }
}

//**************************************************************************************************
// Legacy Name Resolver
//**************************************************************************************************

pub struct LegacyNameResolver<'env, 'member, 'addr, 'builtin> {
    core: CoreNameResolver<'env, 'member>,
    named_address_map: &'addr NamedAddressMap,
    builtin_types: &'builtin BTreeMap<Symbol, ResolvedType>,
    aliases: legacy_aliases::NameMap,
    old_alias_maps: Vec<legacy_aliases::OldNameMap>,
}

impl LegacyNameResolver<'_, '_, '_, '_> {
    pub fn new<'env, 'member, 'addr, 'builtin>(
        core: CoreNameResolver<'env, 'member>,
        named_address_map: &'addr NamedAddressMap,
        builtin_types: &'builtin BTreeMap<Symbol, ResolvedType>,
    ) -> LegacyNameResolver<'env, 'member, 'addr, 'builtin> {
        LegacyNameResolver {
            core,
            named_address_map,
            builtin_types,
            aliases: legacy_aliases::NameMap::new(),
            old_alias_maps: vec![],
        }
    }

    fn name_access_chain_to_attribute_value(
        &mut self,
        sp!(loc, avalue_): E::AttributeValue,
    ) -> Option<N::AttributeValue> {
        use E::AttributeValue_ as EV;
        use N::AttributeValue_ as NV;
        use P::LeadingNameAccess_ as LN;
        use P::NameAccessChain_ as PN;
        Some(sp(
            loc,
            match avalue_ {
                EV::Value(v) => NV::Value(v),
                // bit wonky, but this is the only spot currently where modules and expressions
                // exist in the same namespace.
                // TODO: consider if we want to just force all of these checks into the well-known
                // attribute setup
                EV::NameAccessChain(name) => {
                    let ident_loc = name.loc;
                    match name.value {
                        single_entry!(name, tyargs, is_macro) => {
                            if self.aliases.module_alias_get(&name).is_some() {
                                ice_assert!(self.core.env, tyargs.is_none(), loc, "Found tyargs");
                                ice_assert!(self.core.env, is_macro.is_none(), loc, "Found macro");
                                let sp!(_, mident_) = self.aliases.module_alias_get(&name).unwrap();
                                let mident = sp(ident_loc, mident_);
                                if !self
                                    .get_core_resolver()
                                    .module_members
                                    .contains_key(&mident)
                                {
                                    self.core.env.add_diag(undefined_module_error(mident));
                                    return None;
                                } else {
                                    NV::Module(mident)
                                }
                            } else if let Some(result) =
                                self.name_access_chain_to_access_chain_result(Access::Type, name)
                            {
                                let result: AccessChainResult<ResolvedName> = result;
                                match result.result {
                                    ResolvedName::Address(addr) => NV::Address(addr),
                                    ResolvedName::Module(mident) => NV::Module(mident),
                                    ResolvedName::Definition(definition) => match definition {
                                        ResolvedDefinition::Function(_)
                                        | ResolvedDefinition::Datatype(_)
                                        | ResolvedDefinition::Constant(_)
                                        | ResolvedDefinition::BuiltinFun(_)
                                        | ResolvedDefinition::BuiltinType(_)
                                        | ResolvedDefinition::Variant(_) => {
                                            NV::Definition(definition)
                                        }
                                        ResolvedDefinition::TypeParam(_, _) => unreachable!(),
                                    },
                                    ResolvedName::Variable(_) => unreachable!(),
                                }
                            } else {
                                NV::UnresolvedName(name)
                            }
                        }
                        PN::Path(path) => {
                            ice_assert!(self.core.env, !path.has_tyargs(), loc, "Found tyargs");
                            ice_assert!(
                                self.core.env,
                                path.is_macro().is_none(),
                                loc,
                                "Found macro"
                            );
                            match (&path.root.name, &path.entries[..]) {
                                (sp!(aloc, LN::AnonymousAddress(a)), [n]) => {
                                    let addr = Address::anonymous(*aloc, *a);
                                    let mident =
                                        sp(ident_loc, ModuleIdent_::new(addr, ModuleName(n.name)));
                                    if self.core.module_members.get(&mident).is_none() {
                                        self.core.env.add_diag(undefined_module_error(mident));
                                        return None;
                                    } else {
                                        NV::Module(mident)
                                    }
                                }
                                (sp!(aloc, LN::GlobalAddress(n1) | LN::Name(n1)), [n2])
                                    if self
                                        .core
                                        .named_address_mapping
                                        .as_ref()
                                        .map(|m| m.contains_key(&n1.value))
                                        .unwrap_or(false) =>
                                {
                                    let Some(addr) =
                                        self.top_level_address(sp(*aloc, LN::Name(*n1)))
                                    else {
                                        assert!(self.core.env.has_errors());
                                        return None;
                                    };
                                    let mident =
                                        sp(ident_loc, ModuleIdent_::new(addr, ModuleName(n2.name)));
                                    if !self.core.module_members.contains_key(&mident) {
                                        self.core.env.add_diag(undefined_module_error(mident));
                                        return None;
                                    } else {
                                        NV::Module(mident)
                                    }
                                }
                                _ => {
                                    let result: AccessChainResult<ResolvedName> = self
                                        .name_access_chain_to_access_chain_result(
                                            Access::Type,
                                            name,
                                        )?;
                                    match result.result {
                                        ResolvedName::Address(addr) => NV::Address(addr),
                                        ResolvedName::Module(mident) => NV::Module(mident),
                                        ResolvedName::Definition(definition) => match definition {
                                            ResolvedDefinition::Function(_)
                                            | ResolvedDefinition::Datatype(_)
                                            | ResolvedDefinition::Constant(_)
                                            | ResolvedDefinition::BuiltinFun(_)
                                            | ResolvedDefinition::BuiltinType(_)
                                            | ResolvedDefinition::Variant(_) => {
                                                NV::Definition(definition)
                                            }
                                            ResolvedDefinition::TypeParam(_, _) => unreachable!(),
                                        },
                                        ResolvedName::Variable(_) => unreachable!(),
                                    }
                                }
                            }
                        }
                    }
                }
            },
        ))
    }

    // This function is a bit of a stick in the mud: some errors are pretty bespoke, and it would
    // be rather painful to propagate them outward. To this end, this returns None if it is already
    // reported an error, but may also report a standard AccessCianNameResult failure for later
    // reporting.
    fn name_access_chain_to_access_chain_result(
        &mut self,
        access: Access,
        sp!(loc, ptn_): P::NameAccessChain,
    ) -> AccessChainResult<AccessChainNameResult> {
        use AccessChainFailure as NF;
        use AccessChainNameResult as NR;
        use P::{LeadingNameAccess_ as LN, NameAccessChain_ as PN};
        match (access, ptn_) {
            // Unreachable cases / ICEs
            (Access::PatternBinder, _) => {
                // This _should_ be unreachable, but we need to do something.
                let diag = ice!((
                    loc,
                    "Attempted to expand a variant with the legacy path expander"
                ));
                make_access_result(NR::LegacyError(diag), None, None)
            }
            (Access::Module, single_entry!(name, tyargs, is_macro)) => {
                // This _should_ be unreachable, but we need to do something.
                let diag = ice!((
                    loc,
                    "ICE path resolution produced an impossible path for a module"
                ));
                make_access_result(NR::LegacyError(diag), None, None)
            }
            // Single Entries
            (
                Access::ApplyPositional | Access::ApplyNamed | Access::Type,
                single_entry!(name, tyargs, is_macro),
            ) => {
                if access == Access::Type {
                    ice_assert!(self.core.env, is_macro.is_none(), loc, "Found macro");
                }
                let defn = self
                    .aliases
                    .member_alias_get(&name)
                    .map(ResolvedName::Definition)
                    .map(|name| NR::ResolvedName(loc, name))
                    .unwrap_or_else(|| NR::UnresolvedName(loc, name));
                make_access_result(defn, tyargs, is_macro)
            }
            (Access::Term, single_entry!(name, tyargs, is_macro))
                if is_valid_datatype_or_constant_name(name.value.as_str()) =>
            {
                let defn = self
                    .aliases
                    .member_alias_get(&name)
                    .map(ResolvedName::Definition)
                    .map(|name| NR::ResolvedName(loc, name))
                    .unwrap_or_else(|| NR::UnresolvedName(loc, name));
                make_access_result(defn, tyargs, is_macro)
            }
            (Access::Term, single_entry!(name, tyargs, is_macro)) => {
                let defn = self
                    .core
                    .resolve_local(&name)
                    .map(ResolvedName::Variable)
                    .map(|name| NR::ResolvedName(loc, name))
                    .unwrap_or_else(|| NR::UnresolvedName(loc, name));
                make_access_result(defn, tyargs, is_macro)
            }
            // Paths
            (_, PN::Path(mut path)) => {
                if access == Access::Type {
                    ice_assert!(self.core.env, path.is_macro().is_none(), loc, "Found macro");
                }
                match (&path.root.name, &path.entries[..]) {
                    // Error cases: an address followed by one thing
                    (sp!(aloc, LN::AnonymousAddress(_)), [_]) => {
                        let diag = unexpected_address_module_error(loc, *aloc, access);
                        make_access_result(NR::LegacyError(diag), None, None)
                    }
                    (sp!(_aloc, LN::GlobalAddress(_)), [_]) => {
                        let mut diag: Diagnostic = create_feature_error(
                            self.core.env.edition(None), // We already know we are failing, so no package.
                            FeatureGate::Move2024Paths,
                            loc,
                        );
                        diag.add_secondary_label((
                            loc,
                            "Paths that start with `::` are not valid in legacy move.",
                        ));
                        make_access_result(NR::LegacyError(diag), None, None)
                    }
                    // Others
                    (sp!(_, LN::Name(n1)), [n2]) => match self.aliases.module_alias_get(n1) {
                        None => {
                            let diag = diag!(
                                NameResolution::UnboundModule,
                                (n1.loc, format!("Unbound module alias '{}'", n1))
                            );
                            self.core.env.add_diag(diag);
                            make_access_result(NR::LegacyError(diag), None, None)
                        }
                        Some(mident) => {
                            let Some(module_entries) = self.core.module_members.get(&mident) else {
                                let result = return NR::ResolutionFailure(
                                    Box::new(NR::ResolvedName(
                                        mident.loc,
                                        ResolvedName::Address(mident.value.address),
                                    )),
                                    NF::UnknownModuleDefinition(
                                        mident.value.address,
                                        mident.value.module.into(),
                                    ),
                                );
                                return make_access_result(result, None, None);
                            };
                            let n2_name = n2.name;
                            let (tyargs, is_macro) = if !(path.has_tyargs_last()) {
                                let mut diag = diag!(
                                    Syntax::InvalidName,
                                    (path.tyargs_loc().unwrap(), "Invalid type argument position")
                                );
                                diag.add_note(
                                    "Type arguments may only be used with module members",
                                );
                                self.core.env.add_diag(diag);
                                (None, path.is_macro())
                            } else {
                                (path.take_tyargs(), path.is_macro())
                            };

                            let defn = if let Some(entry) = module_entries.get(&n2_name) {
                                NR::ResolvedName(loc, ResolvedName::Definition(entry))
                            } else {
                                let result = NR::ResolutionFailure(
                                    Box::new(NR::ResolvedName(
                                        mident.loc,
                                        ResolvedName::Module(mident),
                                    )),
                                    NF::UnknownMemberDefinition(access, mident, n2.name),
                                );
                                return make_access_result(result, tyargs, is_macro);
                            };
                            make_access_result(defn, tyargs, is_macro)
                        }
                    },
                    (ln, [n2, n3]) => {
                        let ident_loc = make_loc(
                            ln.loc.file_hash(),
                            ln.loc.start() as usize,
                            n2.name.loc.end() as usize,
                        );
                        let addr = self.top_level_address(ln);
                        let mident = sp(loc, ModuleIdent_::new(addr, ModuleName(n2)));
                        let Some(module_entries) = self.core.module_members.get(&mident) else {
                            let result = return NR::ResolutionFailure(
                                Box::new(NR::ResolvedName(mident.loc, ResolvedName::Address(addr))),
                                NF::UnknownModuleDefinition(addr, n2.name),
                            );
                            return make_access_result(result, None, None);
                        };
                        let n3_name = n3.name;
                        let (tyargs, is_macro) = if !(path.has_tyargs_last()) {
                            let mut diag = diag!(
                                Syntax::InvalidName,
                                (path.tyargs_loc().unwrap(), "Invalid type argument position")
                            );
                            diag.add_note("Type arguments may only be used with module members");
                            self.core.env.add_diag(diag);
                            (None, path.is_macro())
                        } else {
                            (path.take_tyargs(), path.is_macro())
                        };

                        let defn = if let Some(entry) = module_entries.get(&n3_name) {
                            NR::ResolvedName(loc, ResolvedName::Definition(entry))
                        } else {
                            let result = NR::ResolutionFailure(
                                Box::new(NR::ResolvedName(
                                    mident.loc,
                                    ResolvedName::Module(mident),
                                )),
                                NF::UnknownMemberDefinition(access, mident, n3.name),
                            );
                            return make_access_result(result, tyargs, is_macro);
                        };
                        make_access_result(defn, tyargs, is_macro)
                    }
                    (_ln, []) => {
                        let diag = ice!((loc, "Found a root path with no additional entries"));
                        return None;
                    }
                    (_ln, [_n1, _n2, ..]) => {
                        let mut diag = diag!(Syntax::InvalidName, (loc, "Too many name segments"));
                        diag.add_note("Names may only have 0, 1, or 2 segments separated by '::'");
                        return None;
                    }
                }
            }
        }
    }

    fn name_access_chain_to_module_ident(
        &mut self,
        sp!(loc, pn_): P::NameAccessChain,
    ) -> Option<E::ModuleIdent> {
        use P::NameAccessChain_ as PN;
        let mident = match pn_ {
            PN::Single(single) => {
                ice_assert!(self.core.env, single.tyargs.is_none(), loc, "Found tyargs");
                ice_assert!(self.core.env, single.is_macro.is_none(), loc, "Found macro");
                match self.aliases.module_alias_get(&single.name) {
                    None => {
                        self.core.env.add_diag(diag!(
                            NameResolution::UnboundModule,
                            (
                                single.name.loc,
                                format!("Unbound module alias '{}'", single.name)
                            ),
                        ));
                        None
                    }
                    Some(mident) => {
                        self.core.validate_mident(&mident);
                        Some(mident)
                    }
                }
            }
            PN::Path(path) => {
                ice_assert!(self.core.env, !path.has_tyargs(), loc, "Found tyargs");
                ice_assert!(self.core.env, path.is_macro().is_none(), loc, "Found macro");
                match (&path.root.name, &path.entries[..]) {
                    (ln, [n]) => {
                        let addr = self.top_level_address(ln);
                        let mident = sp(loc, ModuleIdent_::new(addr, ModuleName(n.name)));
                        if !self.core.module_members.contains_key(&mident) {
                            self.core.env.add_diag(undefined_module_error(n.name));
                            None
                        } else {
                            Some(mident)
                        }
                    }
                    // Error cases
                    (_ln, []) => {
                        self.core
                            .env
                            .add_diag(ice!((loc, "Found path with no path entries")));
                        None
                    }
                    (ln, [n, m, ..]) => {
                        let ident_loc = make_loc(
                            ln.loc.file_hash(),
                            ln.loc.start() as usize,
                            n.name.loc.end() as usize,
                        );
                        // Process the module ident just for errors
                        let addr = self.top_level_address(ln);
                        let mident = sp(loc, ModuleIdent_::new(addr, ModuleName(n.name)));
                        if !self.core.module_members.contains_key(&mident) {
                            self.core.env.add_diag(undefined_module_error(n.name));
                        }
                        self.core.env.add_diag(diag!(
                            NameResolution::NamePositionMismatch,
                                if path.entries.len() < 3 {
                                    (m.name.loc, "Unexpected module member access. Expected a module identifier only")
                                } else {
                                    (loc, "Unexpected access. Expected a module identifier only")
                                }
                        ));
                        None
                    }
                }
            }
        };
        mident.map(|mdient| self.core.validate_module_ident(mident))
    }

    fn top_level_address(&mut self, ln: P::LeadingNameAccess) -> Address {
        let name_res = check_valid_address_name(self.core.env, &ln);
        let named_address_mapping = self.named_address_mapping.unwrap();
        let sp!(loc, ln_) = ln;
        match ln_ {
            P::LeadingNameAccess_::AnonymousAddress(bytes) => {
                debug_assert!(name_res.is_ok());
                Address::anonymous(loc, bytes)
            }
            // This should have been handled elsewhere in alias resolution for user-provided paths, and
            // should never occur in compiler-generated ones.
            P::LeadingNameAccess_::GlobalAddress(name) => {
                self.core.env.add_diag(ice!((
                    loc,
                    "Found an address in top-level address position that uses a global name"
                )));
                // Try to keep going
                self.top_level_address(sp(loc, P::LeadingNameAccess_::Name(name)))
            }
            P::LeadingNameAccess_::Name(name) => {
                match named_address_mapping.get(&name.value).copied() {
                    Some(addr) => make_address(self.core.env, name, loc, addr),
                    None => {
                        if name_res.is_ok() {
                            self.core
                                .env
                                .add_diag(address_without_value_error(false, loc, &name));
                        } else {
                            assert!(self.core.env.has_errors());
                        }
                        Address::NamedUnassigned(name)
                    }
                }
            }
        }
    }
}

impl NameResolver for LegacyNameResolver<'_, '_, '_, '_> {
    fn push_alias_scope(
        &mut self,
        loc: Loc,
        new_scope: AliasMapBuilder,
    ) -> Result<Vec<UnnecessaryAlias>, Box<Diagnostic>> {
        self.old_alias_maps
            .push(self.aliases.add_and_shadow_all(loc, new_scope)?);
        Ok(vec![])
    }

    fn push_type_parameters(&mut self, tparams: Vec<&Name>) {
        self.old_alias_maps
            .push(self.aliases.shadow_for_type_parameters(tparams));
    }

    fn pop_alias_scope(&mut self) -> NameSet {
        if let Some(outer_scope) = self.old_alias_maps.pop() {
            self.aliases.set_to_outer_scope(outer_scope)
        } else {
            NameSet::new(self.aliases.kind)
        }
    }

    fn env(&mut self) -> &mut CompilationEnv {
        self.core.env
    }

    fn get_core_resolver(&mut self) -> &mut CoreNameResolver {
        &mut self.core
    }

    fn resolve_name_access_chain(
        &mut self,
        access: Access,
        name: NameAccessChain,
    ) -> AccessChainResult<AccessChainNameResult> {
        self.name_access_chain_to_access_chain_result(access, name)
    }

    fn resolve_module(&mut self, name: NameAccessChain) -> Option<ModuleIdent> {
        todo!()
    }

    fn new_alias_map_builder(&mut self, kind: NameMapKind) -> AliasMapBuilder {
        todo!()
    }
}

//**************************************************************************************************
// Error Builders
//**************************************************************************************************

fn unexpected_access_error(loc: Loc, result: String, access: Access) -> Diagnostic {
    let unexpected_msg = if result.starts_with('a') | result.starts_with('e') {
        format!(
            "Unexpected {0} identifier. An {0} identifier is not a valid {1}",
            result,
            access.case()
        )
    } else {
        format!(
            "Unexpected {0} identifier. A {0} identifier is not a valid {1}",
            result,
            access.case()
        )
    };
    diag!(NameResolution::NamePositionMismatch, (loc, unexpected_msg),)
}

fn unexpected_access_msg_error(loc: Loc, result: String, access: &str) -> Diagnostic {
    let unexpected_msg = if result.starts_with('a') | result.starts_with('e') {
        format!(
            "Unexpected {0} identifier. An {0} identifier is not a valid {1}",
            result, access
        )
    } else {
        format!(
            "Unexpected {0} identifier. A {0} identifier is not a valid {1}",
            result, access
        )
    };
    diag!(NameResolution::NamePositionMismatch, (loc, unexpected_msg),)
}

fn unbound_type_error(name: Name) -> Diagnostic {
    diag!(
        NameResolution::UnboundType,
        (name.loc, format!("Unbound type '{}'", name))
    )
}

fn unbound_term_error(name: Name, code: NameResolution, error_msg: &str) -> Diagnostic {
    unbound_term_msg_error(&name.to_string(), code, error_msg)
}

fn unbound_term_msg_error(name: &str, code: NameResolution, error_msg: &str) -> Diagnostic {
    diag!(code, (name.loc, format!("Unbound {error_msg} '{name}'")))
}

fn unbound_function_error(name: Name) -> Diagnostic {
    diag!(
        NameResolution::UnboundModuleMember,
        (name.loc, format!("Unbound function '{}'", name))
    )
}

fn unbound_constructor_error(name: Name) -> Diagnostic {
    diag!(
        NameResolution::UnboundModuleMember,
        (name.loc, format!("Unbound constructor '{}'", name))
    )
}

fn unbound_module_error(name: Name) -> Diagnostic {
    diag!(
        NameResolution::UnboundModule,
        (name.loc, format!("Unbound module alias '{}'", name))
    )
}

fn access_chain_resolution_error(result: AccessChainNameResult) -> Diagnostic {
    if let AccessChainNameResult::ResolutionFailure(inner, reason) = result {
        let loc = inner.loc();
        let msg = match reason {
            AccessChainFailure::InvalidKind(kind) => {
                let msg = format!(
                    "Expected {} in this position, not {}",
                    kind,
                    inner.err_name()
                );
                diag!(NameResolution::NamePositionMismatch, (loc, msg))
            }
            AccessChainFailure::UnresolvedAlias(name) => {
                let msg = format!("Could not resolve the name '{}' in this scope", name);
                // FIXME: the error category should be decided and passed along
                diag!(NameResolution::UnboundModuleMember, (loc, msg))
            }
            AccessChainFailure::UnknownMemberDefinition(access, path, name) => {
                let msg = format!(
                    "Invalid module access. Unbound {} '{name}' in module '{path}'",
                    access.kind()
                );
                // FIXME: the error category should be decided and passed along
                diag!(NameResolution::UnboundModuleMember, (loc, msg))
            }
            AccessChainFailure::UnknownModuleDefinition(address, name) => {
                let msg = format!(
                    "Invalid module access. Unbound module '{name}' in address '{address}'"
                );
                diag!(NameResolution::UnboundModule, (loc, msg))
            }
            AccessChainFailure::UnknownVariantDefinition(enum_, variant) => {
                let msg =
                    format!("Invalid module access. Unbound variant '{variant}' in enum '{enum_}'");
                diag!(NameResolution::UnboundVariant, (loc, msg))
            }
        };
        diag!(NameResolution::NamePositionMismatch, (loc, msg))
    } else {
        ice!((
            result.loc(),
            "ICE compiler miscalled access chain resolution error handler"
        ))
    }
}

fn unexpected_address_module_error(loc: Loc, nloc: Loc, access: Access) -> Diagnostic {
    let case = match access {
        Access::Type | Access::ApplyNamed | Access::ApplyPositional => "type",
        Access::Term => "expression",
        Access::PatternBinder => "pattern constructor",
        Access::Module => {
            return ice!(
                (
                    loc,
                    "ICE expected a module name and got one, but tried to report an error"
                ),
                (nloc, "Name location")
            )
        }
    };
    let unexpected_msg = format!(
        "Unexpected module identifier. A module identifier is not a valid {}",
        case
    );
    diag!(
        NameResolution::NamePositionMismatch,
        (loc, unexpected_msg),
        (nloc, "Expected a module name".to_owned()),
    )
}

fn address_without_value_error(suggest_declaration: bool, loc: Loc, n: &Name) -> Diagnostic {
    let mut msg = format!("address '{}' is not assigned a value", n);
    if suggest_declaration {
        msg = format!(
            "{}. Try assigning it a value when calling the compiler",
            msg,
        )
    }
    diag!(NameResolution::AddressWithoutValue, (loc, msg))
}

fn undefined_module_error(mident: ModuleIdent) -> Diagnostic {
    diag!(
        NameResolution::UnboundModule,
        (mident.loc, format!("Unbound module '{}'", mident))
    )
}

fn invalid_address_error(loc: Loc, n: &Name) -> Diagnostic {
    let mut msg = format!("Invalid address '{}'", n);
    diag!(Declarations::InvalidAddress, (loc, msg))
}

//************************************************
// Display
//************************************************

impl std::fmt::Display for ResolvedDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolvedDefinition::Function(d)
            | ResolvedDefinition::Variant(d)
            | ResolvedDefinition::Datatype(d)
            | ResolvedDefinition::Constant(d)
            | ResolvedDefinition::TypeParam(_, d)
            | ResolvedDefinition::BuiltinFun(d)
            | ResolvedDefinition::BuiltinType(d) => d.fmt(f),
        }
    }
}

impl std::fmt::Display for ResolvedDatatype {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolvedDatatype::Struct(d) | ResolvedDatatype::Enum(d) => d.fmt(f),
        }
    }
}

impl std::fmt::Display for ResolvedStruct {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}::{}", self.module, self.name)
    }
}

impl std::fmt::Display for ResolvedEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}::{}", self.module, self.name)
    }
}

impl std::fmt::Display for ResolvedMemberFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}::{}", self.module, self.name)
    }
}
impl std::fmt::Display for ResolvedVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}::{}::{}", self.module, self.enum_name, self.name)
    }
}

impl std::fmt::Display for ResolvedVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}::{}::{}", self.module, self.enum_name, self.name)
    }
}

impl std::fmt::Display for ResolvedName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolvedName::Address(d)
            | ResolvedName::Module(d)
            | ResolvedName::Definition(d)
            | ResolvedName::Variable(d) => d.fmt(f),
        }
    }
}
