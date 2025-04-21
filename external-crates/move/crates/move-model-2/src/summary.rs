// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Provides serializable summaries for packages--potentially with additional names from the Move
//! source code. The summaries include the signatures of all functions (potentially macros) and
//! datatypes (structs and enums).

use crate::{
    TModuleId,
    normalized::{self, ModuleId},
    source_model,
};
use indexmap::IndexMap;
use move_binary_format::file_format;
use move_compiler::{
    expansion::ast as E, naming::ast as N, parser::ast as P, parser::ast::DocComment,
    shared::program_info::FunctionInfo,
};
use move_core_types::account_address::AccountAddress;
use move_symbol_pool::Symbol;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Indicates that the information came from the source code
pub type FromSource<T> = Option<T>;

#[derive(Serialize, Deserialize)]
pub struct Packages {
    pub packages: BTreeMap<AccountAddress, Package>,
}

#[derive(Serialize, Deserialize)]
pub struct Package {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub name: FromSource<Symbol>,
    pub modules: BTreeMap<Symbol, Module>,
}

#[derive(Serialize, Deserialize)]
pub struct Module {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub doc: FromSource<Option<String>>,
    pub immediate_dependencies: BTreeSet<ModuleId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub attributes: FromSource<Attributes>,
    pub functions: IndexMap<Symbol, Function>,
    pub structs: IndexMap<Symbol, Struct>,
    pub enums: IndexMap<Symbol, Enum>,
}

pub type Attributes = Vec<Attribute>;

#[derive(Serialize, Deserialize)]
pub enum Attribute {
    Name(Symbol),
    Assigned(Symbol, String),
    Parameterized(Symbol, Attributes),
}

#[derive(Serialize, Deserialize)]
pub struct Function {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub source_index: FromSource<usize>,
    /// Set to usize::max_value if the function is a macro
    pub index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub doc: FromSource<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub attributes: FromSource<Attributes>,
    pub visibility: Visibility,
    pub entry: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub macro_: FromSource<bool>,
    pub type_parameters: Vec<TParam>,
    pub parameters: Vec<Parameter>,
    pub return_: Vec<Type>,
}

#[derive(Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Friend,
    Package,
    Private,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TParam {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub name: FromSource<Symbol>,
    pub constraints: AbilitySet,
}

#[derive(Serialize, Deserialize)]
pub struct Parameter {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    name: FromSource<Symbol>,
    type_: Type,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbilitySet(BTreeSet<Ability>);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Ability {
    Copy,
    Drop,
    Key,
    Store,
}

#[derive(Serialize, Deserialize)]
pub struct Struct {
    pub index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub doc: FromSource<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub attributes: FromSource<Attributes>,
    pub abilities: AbilitySet,
    pub type_parameters: Vec<DatatypeTParam>,
    pub fields: Fields,
}

#[derive(Serialize, Deserialize)]
pub struct DatatypeTParam {
    pub phantom: bool,
    #[serde(flatten)]
    pub tparam: TParam,
}

#[derive(Serialize, Deserialize)]
pub struct Enum {
    pub index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub doc: FromSource<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub attributes: FromSource<Attributes>,
    pub abilities: AbilitySet,
    pub type_parameters: Vec<DatatypeTParam>,
    pub variants: IndexMap<Symbol, Variant>,
}

#[derive(Serialize, Deserialize)]
pub struct Variant {
    pub index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub doc: FromSource<Option<String>>,
    pub fields: Fields,
}

#[derive(Serialize, Deserialize)]
pub struct Fields {
    /// True if the variant was known to be defined using positional fields
    pub positional_fields: bool,
    pub fields: IndexMap<Symbol, Field>,
}

#[derive(Serialize, Deserialize)]
pub struct Field {
    pub index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub doc: FromSource<Option<String>>,
    pub type_: Type,
}

#[derive(Serialize, Deserialize)]
pub enum Type {
    #[serde(rename = "bool")]
    Bool,
    #[serde(rename = "u8")]
    U8,
    #[serde(rename = "u16")]
    U16,
    #[serde(rename = "u32")]
    U32,
    #[serde(rename = "u64")]
    U64,
    #[serde(rename = "u128")]
    U128,
    #[serde(rename = "u256")]
    U256,
    #[serde(rename = "address")]
    Address,
    #[serde(rename = "signer")]
    Signer,
    Datatype(Box<Datatype>),
    #[serde(rename = "vector")]
    Vector(Box<Type>),
    Reference(/* is_mut */ bool, Box<Type>),
    /// From bytecode
    TypeParameter(u16),
    /// From source code
    NamedTypeParameter(Symbol),
    /// Potentially present in macros
    #[serde(rename = "tuple")]
    Tuple(Vec<Type>),
    /// Potentially present in macros
    #[serde(rename = "fun")]
    Fun(Vec<Type>, Box<Type>),
    /// Potentially present in macros
    #[serde(rename = "_")]
    Any,
}

#[derive(Serialize, Deserialize)]
pub struct Datatype {
    pub module: ModuleId,
    pub name: Symbol,
    pub type_arguments: Vec<Type>,
}

//**************************************************************************************************
// Normalized
//**************************************************************************************************

impl From<&normalized::Packages> for Packages {
    fn from(p: &normalized::Packages) -> Self {
        let normalized::Packages { packages } = p;
        let packages = packages
            .iter()
            .map(|(address, package)| (*address, package.into()))
            .collect();
        Self { packages }
    }
}

impl From<&normalized::Package> for Package {
    fn from(p: &normalized::Package) -> Self {
        let normalized::Package {
            package: _,
            modules,
        } = p;
        let modules = modules
            .iter()
            .map(|(name, module)| (*name, module.into()))
            .collect();
        Self {
            name: None, // set by ProgramInfo
            modules,
        }
    }
}

impl From<&normalized::Module> for Module {
    fn from(m: &normalized::Module) -> Self {
        let normalized::Module {
            immediate_dependencies,
            structs,
            enums,
            functions,
            ..
        } = m;
        let immediate_dependencies = immediate_dependencies.iter().cloned().collect();
        let functions = functions
            .iter()
            .enumerate()
            .map(|(index, (name, function))| (*name, (index, &**function).into()))
            .collect();
        let structs = structs
            .iter()
            .enumerate()
            .map(|(index, (name, s))| (*name, (index, &**s).into()))
            .collect();
        let enums = enums
            .iter()
            .enumerate()
            .map(|(index, (name, e))| (*name, (index, &**e).into()))
            .collect();
        Self {
            doc: None,        // set by ProgramInfo
            attributes: None, // set by ProgramInfo
            immediate_dependencies,
            functions,
            structs,
            enums,
        }
    }
}

impl From<(usize, &normalized::Function)> for Function {
    fn from((index, f): (usize, &normalized::Function)) -> Self {
        let normalized::Function {
            visibility,
            is_entry,
            type_parameters,
            parameters,
            return_,
            ..
        } = f;
        let visibility = match visibility {
            file_format::Visibility::Public => Visibility::Public,
            file_format::Visibility::Friend => Visibility::Friend,
            file_format::Visibility::Private => Visibility::Private,
        };
        let type_parameters = type_parameters
            .iter()
            .map(|tp| TParam {
                name: None, // set by ProgramInfo
                constraints: (*tp).into(),
            })
            .collect();
        let parameters = parameters
            .iter()
            .map(|t| Parameter {
                name: None, // set by ProgramInfo
                type_: (&**t).into(),
            })
            .collect();
        let return_ = return_.iter().map(|t| (&**t).into()).collect();
        Self {
            index,
            source_index: None, // set by ProgramInfo
            doc: None,          // set by ProgramInfo
            attributes: None,   // set by ProgramInfo
            visibility,
            entry: *is_entry,
            macro_: None, // set by ProgramInfo
            type_parameters,
            parameters,
            return_,
        }
    }
}

impl From<file_format::AbilitySet> for AbilitySet {
    fn from(abilities: file_format::AbilitySet) -> Self {
        Self(
            abilities
                .into_iter()
                .map(|a| match a {
                    file_format::Ability::Copy => Ability::Copy,
                    file_format::Ability::Drop => Ability::Drop,
                    file_format::Ability::Key => Ability::Key,
                    file_format::Ability::Store => Ability::Store,
                })
                .collect(),
        )
    }
}

impl From<(usize, &normalized::Struct)> for Struct {
    fn from((index, s): (usize, &normalized::Struct)) -> Self {
        let normalized::Struct {
            abilities,
            type_parameters,
            fields,
            ..
        } = s;
        let abilities = (*abilities).into();
        let type_parameters = type_parameters
            .iter()
            .copied()
            .map(|tp| tp.into())
            .collect();
        let fields = Fields {
            positional_fields: false, // set by ProgramInfo
            fields: fields
                .0
                .iter()
                .enumerate()
                .map(|(index, (n, f))| (*n, (index, &**f).into()))
                .collect(),
        };
        Self {
            index,
            doc: None,        // set by ProgramInfo
            attributes: None, // set by ProgramInfo
            abilities,
            type_parameters,
            fields,
        }
    }
}

impl From<(usize, &normalized::Enum)> for Enum {
    fn from((index, e): (usize, &normalized::Enum)) -> Self {
        let normalized::Enum {
            type_parameters,
            abilities,
            variants,
            ..
        } = e;
        let abilities = (*abilities).into();
        let type_parameters = type_parameters
            .iter()
            .copied()
            .map(|tp| tp.into())
            .collect();
        let variants = variants
            .iter()
            .enumerate()
            .map(|(index, (name, v))| (*name, (index, &**v).into()))
            .collect();
        Self {
            index,
            doc: None,        // set by ProgramInfo
            attributes: None, // set by ProgramInfo
            abilities,
            type_parameters,
            variants,
        }
    }
}

impl From<(usize, &normalized::Variant)> for Variant {
    fn from((index, v): (usize, &normalized::Variant)) -> Self {
        let normalized::Variant { fields, .. } = v;
        let fields = Fields {
            positional_fields: false, // set by ProgramInfo
            fields: fields
                .0
                .iter()
                .enumerate()
                .map(|(index, (n, f))| (*n, (index, &**f).into()))
                .collect(),
        };
        Self {
            index,
            doc: None, // set by ProgramInfo
            fields,
        }
    }
}

impl From<(usize, &normalized::Field)> for Field {
    fn from((index, f): (usize, &normalized::Field)) -> Self {
        let normalized::Field { type_, .. } = f;
        Self {
            index,
            doc: None, // set by ProgramInfo
            type_: Type::from(type_),
        }
    }
}

impl From<file_format::DatatypeTyParameter> for DatatypeTParam {
    fn from(tparam: file_format::DatatypeTyParameter) -> Self {
        let file_format::DatatypeTyParameter {
            constraints,
            is_phantom,
        } = tparam;
        Self {
            phantom: is_phantom,
            tparam: TParam {
                name: None, // set by ProgramInfo
                constraints: constraints.into(),
            },
        }
    }
}

impl From<&normalized::Type> for Type {
    fn from(ty: &normalized::Type) -> Self {
        match ty {
            normalized::Type::Bool => Type::Bool,
            normalized::Type::U8 => Type::U8,
            normalized::Type::U16 => Type::U16,
            normalized::Type::U32 => Type::U32,
            normalized::Type::U64 => Type::U64,
            normalized::Type::U128 => Type::U128,
            normalized::Type::U256 => Type::U256,
            normalized::Type::Address => Type::Address,
            normalized::Type::Signer => Type::Signer,
            normalized::Type::Datatype(d) => Type::Datatype(Box::new(Datatype {
                module: d.module,
                name: d.name,
                type_arguments: d.type_arguments.iter().map(Type::from).collect(),
            })),
            normalized::Type::Vector(t) => Type::Vector(Box::new((&**t).into())),
            normalized::Type::Reference(is_mut, t) => {
                Type::Reference(*is_mut, Box::new((&**t).into()))
            }
            normalized::Type::TypeParameter(t) => Type::TypeParameter(*t),
        }
    }
}

//**************************************************************************************************
// Annotation
//**************************************************************************************************

impl Packages {
    pub fn annotate(&mut self, model: &source_model::Model) {
        for (address, package) in &mut self.packages {
            package.annotate(&model.package(address))
        }
    }
}

impl Package {
    pub fn annotate(&mut self, package: &source_model::Package) {
        debug_assert!(self.name.is_none());
        self.name = package.name();
        for (name, module) in &mut self.modules {
            module.annotate(&package.module(*name));
        }
    }
}

impl Module {
    pub fn annotate(&mut self, module: &source_model::Module) {
        debug_assert!(self.doc.is_none());
        debug_assert!(self.attributes.is_none());
        let info = module.info();
        self.doc = Some(doc_comment(&info.doc));
        self.attributes = Some(attributes(&info.attributes));
        for (name, f) in &mut self.functions {
            f.annotate(&module.function(*name));
        }
        for (name, finfo) in info
            .functions
            .key_cloned_iter()
            .filter(|(_, finfo)| finfo.macro_.is_some())
        {
            self.functions
                .insert(name.0.value, Function::from_macro(finfo));
        }

        for (name, s) in &mut self.structs {
            s.annotate(&module.struct_(*name));
        }
        for (name, e) in &mut self.enums {
            e.annotate(&module.enum_(*name));
        }
    }
}

impl Function {
    pub fn annotate(&mut self, function: &source_model::Function) {
        debug_assert!(self.doc.is_none());
        debug_assert!(self.attributes.is_none());
        debug_assert!(self.source_index.is_none());
        debug_assert!(self.macro_.is_none());
        let info = function.info();
        self.doc = Some(doc_comment(&info.doc));
        self.attributes = Some(attributes(&info.attributes));
        self.source_index = Some(info.index);
        self.visibility = (&info.visibility).into();
        self.type_parameters
            .iter_mut()
            .zip(&info.signature.type_parameters)
            .for_each(|(tp, tp_info)| {
                debug_assert!(tp.name.is_none());
                tp.name = Some(tp_info.user_specified_name.value)
            });
        self.parameters
            .iter_mut()
            .zip(&info.signature.parameters)
            .for_each(|(Parameter { name, type_ }, (_, param_name, param_ty))| {
                debug_assert!(name.is_none());
                *name = Some(param_name.value.name);
                *type_ = param_ty.into();
            });
        self.return_ = compiler_multiple_types(&info.signature.return_type);
    }

    pub fn from_macro(finfo: &FunctionInfo) -> Self {
        assert!(finfo.macro_.is_some());
        Self {
            source_index: Some(finfo.index),
            index: usize::MAX,
            doc: Some(doc_comment(&finfo.doc)),
            attributes: Some(attributes(&finfo.attributes)),
            visibility: (&finfo.visibility).into(),
            entry: false,
            macro_: Some(true),
            type_parameters: finfo
                .signature
                .type_parameters
                .iter()
                .map(|tp| TParam {
                    name: Some(tp.user_specified_name.value),
                    constraints: (&tp.abilities).into(),
                })
                .collect(),
            parameters: finfo
                .signature
                .parameters
                .iter()
                .map(|(_, param_name, type_)| Parameter {
                    name: Some(param_name.value.name),
                    type_: type_.into(),
                })
                .collect(),
            return_: compiler_multiple_types(&finfo.signature.return_type),
        }
    }
}

impl From<&E::Visibility> for Visibility {
    fn from(visibility: &E::Visibility) -> Self {
        match visibility {
            E::Visibility::Public(_) => Visibility::Public,
            E::Visibility::Friend(_) => Visibility::Friend,
            E::Visibility::Package(_) => Visibility::Package,
            E::Visibility::Internal => Visibility::Private,
        }
    }
}

impl From<&E::AbilitySet> for AbilitySet {
    fn from(abilities: &E::AbilitySet) -> Self {
        Self(
            abilities
                .into_iter()
                .map(|a| match &a.value {
                    P::Ability_::Copy => Ability::Copy,
                    P::Ability_::Drop => Ability::Drop,
                    P::Ability_::Key => Ability::Key,
                    P::Ability_::Store => Ability::Store,
                })
                .collect(),
        )
    }
}
impl Struct {
    pub fn annotate(&mut self, s: &source_model::Struct) {
        debug_assert!(self.doc.is_none());
        debug_assert!(self.attributes.is_none());
        let info = s.info();
        self.doc = Some(doc_comment(&info.doc));
        self.attributes = Some(attributes(&info.attributes));
        self.type_parameters
            .iter_mut()
            .zip(&info.type_parameters)
            .for_each(|(tp, tp_info)| {
                debug_assert!(tp.tparam.name.is_none());
                tp.tparam.name = Some(tp_info.param.user_specified_name.value);
            });
        self.fields.annotate_struct(&info.fields);
    }
}

impl Enum {
    pub fn annotate(&mut self, e: &source_model::Enum) {
        debug_assert!(self.doc.is_none());
        debug_assert!(self.attributes.is_none());
        let info = e.info();
        self.doc = Some(doc_comment(&info.doc));
        self.attributes = Some(attributes(&info.attributes));
        self.type_parameters
            .iter_mut()
            .zip(&info.type_parameters)
            .for_each(|(tp, tp_info)| {
                debug_assert!(tp.tparam.name.is_none());
                tp.tparam.name = Some(tp_info.param.user_specified_name.value);
            });
        for (name, v) in &mut self.variants {
            v.annotate(&e.variant(*name));
        }
    }
}

impl Variant {
    pub fn annotate(&mut self, v: &source_model::Variant) {
        debug_assert!(self.doc.is_none());
        let info = v.info();
        self.doc = Some(doc_comment(&info.doc));
        self.fields.annotate_variant(&info.fields);
    }
}

impl Fields {
    pub fn annotate_struct(&mut self, fields: &N::StructFields) {
        debug_assert!(!self.positional_fields);
        let (is_positional, fields) = match fields {
            N::StructFields::Defined(is_positional, fields) => (*is_positional, fields),
            N::StructFields::Native(_) => return,
        };
        self.positional_fields = is_positional;
        for (name, (_, (doc, _))) in fields.key_cloned_iter() {
            let field = self.fields.get_mut(&name.0.value).unwrap();
            debug_assert!(field.doc.is_none());
            field.doc = Some(doc_comment(doc));
        }
    }

    pub fn annotate_variant(&mut self, fields: &N::VariantFields) {
        debug_assert!(!self.positional_fields);
        let (is_positional, fields) = match fields {
            N::VariantFields::Defined(is_positional, fields) => (*is_positional, fields),
            N::VariantFields::Empty => return,
        };
        self.positional_fields = is_positional;
        for (name, (_, (doc, ty))) in fields.key_cloned_iter() {
            let field = self.fields.get_mut(&name.0.value).unwrap();
            debug_assert!(field.doc.is_none());
            field.doc = Some(doc_comment(doc));
            field.type_ = ty.into();
        }
    }
}

impl From<&N::Type> for Type {
    fn from(sp!(_, ty_): &N::Type) -> Self {
        match ty_ {
            N::Type_::Unit => Type::Tuple(vec![]),
            N::Type_::Ref(mut_, inner) => Type::Reference(*mut_, Box::new((&**inner).into())),
            N::Type_::Param(tp) => Type::NamedTypeParameter(tp.user_specified_name.value),
            N::Type_::Apply(_, sp!(_, tn_), tys) => match tn_ {
                N::TypeName_::ModuleType(m, n) => Type::Datatype(Box::new(Datatype {
                    module: m.value.module_id(),
                    name: n.0.value,
                    type_arguments: tys.iter().map(Type::from).collect(),
                })),
                N::TypeName_::Multiple(_) => {
                    if tys.len() == 1 {
                        (&tys[0]).into()
                    } else {
                        Type::Tuple(tys.iter().map(Type::from).collect())
                    }
                }
                N::TypeName_::Builtin(sp!(_, bt)) => match bt {
                    N::BuiltinTypeName_::Bool => Type::Bool,
                    N::BuiltinTypeName_::U8 => Type::U8,
                    N::BuiltinTypeName_::U16 => Type::U16,
                    N::BuiltinTypeName_::U32 => Type::U32,
                    N::BuiltinTypeName_::U64 => Type::U64,
                    N::BuiltinTypeName_::U128 => Type::U128,
                    N::BuiltinTypeName_::U256 => Type::U256,
                    N::BuiltinTypeName_::Address => Type::Address,
                    N::BuiltinTypeName_::Signer => Type::Signer,
                    N::BuiltinTypeName_::Vector => Type::Vector(Box::new((&tys[0]).into())),
                },
            },
            N::Type_::Fun(params, ret_) => Type::Fun(
                params.iter().map(Type::from).collect(),
                Box::new((&**ret_).into()),
            ),
            N::Type_::Var(_) | N::Type_::Anything | N::Type_::UnresolvedError => Type::Any,
        }
    }
}

//**************************************************************************************************
// FromSource annotations
//**************************************************************************************************

fn compiler_multiple_types(ty @ sp!(_, ty_): &N::Type) -> Vec<Type> {
    match ty_ {
        N::Type_::Unit => vec![],
        N::Type_::Apply(_, sp!(_, N::TypeName_::Multiple(_)), tys) => {
            tys.iter().map(Type::from).collect()
        }
        _ => {
            vec![Type::from(ty)]
        }
    }
}

fn doc_comment(doc: &DocComment) -> Option<String> {
    doc.comment().map(|c| c.to_string())
}

fn attributes(attributes: &E::Attributes) -> Vec<Attribute> {
    attributes.iter().map(|(_, _, a)| attribute(a)).collect()
}

fn attribute(attr: &E::Attribute) -> Attribute {
    match &attr.value {
        E::Attribute_::Name(name) => Attribute::Name(name.value),
        E::Attribute_::Assigned(name, value) => {
            Attribute::Assigned(name.value, attribute_value(value))
        }
        E::Attribute_::Parameterized(name, attrs) => Attribute::Parameterized(
            name.value,
            attrs.iter().map(|(_, _, a)| attribute(a)).collect(),
        ),
    }
}

fn attribute_value(value: &E::AttributeValue) -> String {
    match &value.value {
        E::AttributeValue_::Value(v) => attribute_assigned_value(v),
        E::AttributeValue_::Address(a) => attribute_address(a),
        E::AttributeValue_::Module(m) => attribute_module_ident(m),
        E::AttributeValue_::ModuleAccess(ma) => attrribute_module_access(ma),
    }
}

fn attribute_assigned_value(v: &E::Value) -> String {
    match &v.value {
        E::Value_::Address(address) => format!("@{}", attribute_address(address)),
        _ => {
            format!("{v}")
        }
    }
}

fn attribute_address(addr: &E::Address) -> String {
    match addr {
        E::Address::Numerical { name: Some(n), .. } | E::Address::NamedUnassigned(n) => {
            format!("{n}")
        }
        E::Address::Numerical { value, .. } => format!("{value}"),
    }
}

fn attribute_module_ident(module: &E::ModuleIdent) -> String {
    format!(
        "{}::{}",
        attribute_address(&module.value.address),
        module.value.module
    )
}

fn attrribute_module_access(ma: &E::ModuleAccess) -> String {
    match &ma.value {
        E::ModuleAccess_::Name(n) => {
            format!("{n}")
        }
        E::ModuleAccess_::ModuleAccess(m, n) => {
            format!("{}::{n}", attribute_module_ident(m))
        }
        E::ModuleAccess_::Variant(sp!(_, (m, n)), v) => {
            format!("{}::{n}::{v}", attribute_module_ident(m))
        }
    }
}
