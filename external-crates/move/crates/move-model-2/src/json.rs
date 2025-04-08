// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    normalized::{self, ModuleId},
    source_model,
};
use move_binary_format::file_format;
use move_compiler::{expansion::ast as E, naming::ast as N, parser::ast::DocComment};
use move_core_types::account_address::AccountAddress;
use move_symbol_pool::Symbol;
use serde::{de, Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub use normalized::Type;

#[derive(Serialize, Deserialize)]
pub struct Packages {
    pub packages: BTreeMap<AccountAddress, Package>,
}

#[derive(Serialize, Deserialize)]
pub struct Package {
    pub name: Option<Symbol>,
    pub modules: BTreeMap<Symbol, Module>,
}

#[derive(Serialize, Deserialize)]
pub struct Module {
    pub doc: Option<String>,
    pub friends: BTreeSet<ModuleId>,
    pub immediate_dependencies: BTreeSet<ModuleId>,
    pub attributes: Attributes,
    pub functions: BTreeMap<Symbol, Function>,
    pub structs: BTreeMap<Symbol, Struct>,
    pub enums: BTreeMap<Symbol, Enum>,
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
    pub source_index: Option<usize>,
    pub compiled_index: usize,
    pub doc: Option<String>,
    pub attributes: Attributes,
    pub visibility: Visibility,
    pub entry: bool,
    pub macro_: bool,
    pub type_parameters: Vec<TParam>,
    pub parameters: Vec<(Option<Symbol>, Type)>,
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
    pub name: Option<Symbol>,
    pub constraints: AbilitySet,
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
    pub doc: Option<String>,
    pub attributes: Attributes,
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
    pub doc: Option<String>,
    pub attributes: Attributes,
    pub abilities: AbilitySet,
    pub type_parameters: Vec<DatatypeTParam>,
    pub variants: BTreeMap<Symbol, Variant>,
}

#[derive(Serialize, Deserialize)]
pub struct Variant {
    pub index: usize,
    pub doc: Option<String>,
    pub attributes: Attributes,
    pub fields: Fields,
}

#[derive(Serialize, Deserialize)]
pub struct Fields {
    /// True if the variant was known to be defined using positional fields
    pub positional_fields: bool,
    pub fields: BTreeMap<Symbol, Field>,
}

#[derive(Serialize, Deserialize)]
pub struct Field {
    pub index: usize,
    pub doc: Option<String>,
    pub attributes: Attributes,
    pub type_: Type,
}

//**************************************************************************************************
// Construction
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
            friends,
            structs,
            enums,
            functions,
            ..
        } = m;
        let friends = friends.iter().cloned().collect();
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
            doc: None,          // set by ProgramInfo
            attributes: vec![], // set by ProgramInfo
            friends,
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
            .map(|t| (None /* set by ProgramInfo */, (**t).clone()))
            .collect();
        let return_ = return_.iter().map(|t| (**t).clone()).collect();
        Self {
            compiled_index: index,
            source_index: None, // set by ProgramInfo
            doc: None,          // set by ProgramInfo
            attributes: vec![], // set by ProgramInfo
            visibility,
            entry: *is_entry,
            macro_: false,
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
                .iter()
                .enumerate()
                .map(|(index, f)| (f.name, (index, &**f).into()))
                .collect(),
        };
        Self {
            index,
            doc: None,          // set by ProgramInfo
            attributes: vec![], // set by ProgramInfo
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
            doc: None,          // set by ProgramInfo
            attributes: vec![], // set by ProgramInfo
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
                .iter()
                .enumerate()
                .map(|(index, f)| (f.name, (index, &*f).into()))
                .collect(),
        };
        Self {
            index,
            doc: None,          // set by ProgramInfo
            attributes: vec![], // set by ProgramInfo
            fields,
        }
    }
}

impl From<(usize, &normalized::Field)> for Field {
    fn from((index, f): (usize, &normalized::Field)) -> Self {
        let normalized::Field { type_, .. } = f;
        Self {
            index,
            doc: None,          // set by ProgramInfo
            attributes: vec![], // set by ProgramInfo
            type_: type_.clone(),
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
        debug_assert!(self.attributes.is_empty());
        let info = module.info();
        self.doc = doc_comment(&info.doc);
        self.attributes = attributes(&info.attributes);
        for (name, f) in &mut self.functions {
            f.annotate(&module.function(*name));
        }
        // TODO macros
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
        debug_assert!(self.attributes.is_empty());
        let info = function.info();
        self.doc = doc_comment(&info.doc);
        self.attributes = attributes(&info.attributes);
        self.source_index = Some(info.index);
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
            .for_each(|((name, _), (_, param_name, _))| {
                debug_assert!(name.is_none());
                *name = Some(param_name.value.name);
            });
    }
}

impl Struct {
    pub fn annotate(&mut self, s: &source_model::Struct) {
        debug_assert!(self.doc.is_none());
        debug_assert!(self.attributes.is_empty());
        let info = s.info();
        self.doc = doc_comment(&info.doc);
        self.attributes = attributes(&info.attributes);
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
        debug_assert!(self.attributes.is_empty());
        let info = e.info();
        self.doc = doc_comment(&info.doc);
        self.attributes = attributes(&info.attributes);
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
        debug_assert!(self.attributes.is_empty());
        let info = v.info();
        self.doc = doc_comment(&info.doc);
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
            debug_assert!(field.attributes.is_empty());
            field.doc = doc_comment(doc);
        }
    }

    pub fn annotate_variant(&mut self, fields: &N::VariantFields) {
        debug_assert!(!self.positional_fields);
        let (is_positional, fields) = match fields {
            N::VariantFields::Defined(is_positional, fields) => (*is_positional, fields),
            N::VariantFields::Empty => return,
        };
        self.positional_fields = is_positional;
        for (name, (_, (doc, _))) in fields.key_cloned_iter() {
            let field = self.fields.get_mut(&name.0.value).unwrap();
            debug_assert!(field.doc.is_none());
            debug_assert!(field.attributes.is_empty());
            field.doc = doc_comment(doc);
        }
    }
}

//**************************************************************************************************
// Comments and Attributes
//**************************************************************************************************

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
