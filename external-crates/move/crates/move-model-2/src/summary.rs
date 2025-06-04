// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Provides serializable summaries for packages--potentially with additional names from the Move
//! source code. The summaries include the signatures of all functions (potentially macros) and
//! datatypes (structs and enums).

use crate::{TModuleId, model::Model, normalized, source_kind::SourceKind, source_model};
use indexmap::IndexMap;
use move_binary_format::file_format;
use move_compiler::{
    expansion::ast as E,
    naming::ast as N,
    parser::ast::{self as P, DocComment},
    shared::{known_attributes as KA, program_info::FunctionInfo},
};
use move_core_types::{account_address::AccountAddress, vm_status::StatusCode};
use move_symbol_pool::Symbol;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Indicates that the information came from the source code
pub type FromSource<T> = Option<T>;

#[derive(Debug, Serialize, Deserialize)]
pub struct Packages {
    pub packages: BTreeMap<AccountAddress, Package>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Package {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub name: FromSource<Symbol>,
    pub modules: BTreeMap<Symbol, Module>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Module {
    pub id: ModuleId,
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

#[derive(Clone, Copy, Debug, Ord, PartialOrd, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct ModuleId {
    pub address: Symbol,
    pub name: Symbol,
}

pub type Attributes = Vec<Attribute>;

#[derive(Debug, Serialize, Deserialize)]
// TODO(cswords): This should mirror the KnownAttribute structure to save consumers of this from
// from needing to parse attributes a second time.
pub enum Attribute {
    Name(Symbol),
    Assigned(Symbol, String),
    Parameterized(Symbol, Attributes),
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Friend,
    Package,
    Private,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TParam {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub name: FromSource<Symbol>,
    pub constraints: AbilitySet,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Parameter {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    name: FromSource<Symbol>,
    type_: Type,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AbilitySet(BTreeSet<Ability>);

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Ability {
    Copy,
    Drop,
    Key,
    Store,
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct DatatypeTParam {
    pub phantom: bool,
    #[serde(flatten)]
    pub tparam: TParam,
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct Variant {
    pub index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub doc: FromSource<Option<String>>,
    pub fields: Fields,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Fields {
    /// True if the variant was known to be defined using positional fields
    pub positional_fields: bool,
    pub fields: IndexMap<Symbol, Field>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Field {
    pub index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub doc: FromSource<Option<String>>,
    pub type_: Type,
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct Datatype {
    pub module: ModuleId,
    pub name: Symbol,
    pub type_arguments: Vec<DatatypeTArg>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DatatypeTArg {
    pub phantom: bool,
    pub argument: Type,
}

//**************************************************************************************************
// Normalized
//**************************************************************************************************

pub struct Context {
    root_named_address_reverse_map: BTreeMap<AccountAddress, Symbol>,
    phantom_type_positions:
        BTreeMap<normalized::ModuleId, BTreeMap<Symbol, Vec</* is phantom */ bool>>>,
}

impl Context {
    pub fn new<K: SourceKind>(model: &Model<K>) -> Self {
        let root_named_address_reverse_map = model.root_named_address_reverse_map.clone();
        let phantom_type_positions = model
            .compiled
            .packages
            .iter()
            .flat_map(|(addr, pkg)| {
                pkg.modules.iter().map(|(name, module)| {
                    let mut phantom_type_positions = BTreeMap::new();
                    for (n, s) in &module.structs {
                        let phantom_positions =
                            s.type_parameters.iter().map(|t| t.is_phantom).collect();
                        phantom_type_positions.insert(*n, phantom_positions);
                    }
                    for (n, e) in &module.enums {
                        let phantom_positions =
                            e.type_parameters.iter().map(|t| t.is_phantom).collect();
                        phantom_type_positions.insert(*n, phantom_positions);
                    }
                    ((*addr, *name).module_id(), phantom_type_positions)
                })
            })
            .collect();
        Self {
            root_named_address_reverse_map,
            phantom_type_positions,
        }
    }
}

impl Packages {
    pub fn from_normalized(context: &Context, normalized: &normalized::Packages) -> Self {
        let packages = normalized
            .packages
            .iter()
            .map(|(address, package)| (*address, Package::from_normalized(context, package)))
            .collect();
        Self { packages }
    }
}

impl Package {
    pub fn from_normalized(context: &Context, normalized: &normalized::Package) -> Self {
        let modules = normalized
            .modules
            .iter()
            .map(|(name, module)| (*name, Module::from_normalized(context, module)))
            .collect();
        Self {
            name: None, // set by ProgramInfo
            modules,
        }
    }
}

impl Module {
    pub fn from_normalized(context: &Context, normalized: &normalized::Module) -> Self {
        let immediate_dependencies = normalized
            .immediate_dependencies
            .iter()
            .map(|id| ModuleId::from_normalized(context, id))
            .collect();
        let functions = normalized
            .functions
            .iter()
            .enumerate()
            .map(|(index, (name, function))| {
                (*name, Function::from_normalized(context, function, index))
            })
            .collect();
        let structs = normalized
            .structs
            .iter()
            .enumerate()
            .map(|(index, (name, s))| (*name, Struct::from_normalized(context, s, index)))
            .collect();
        let enums = normalized
            .enums
            .iter()
            .enumerate()
            .map(|(index, (name, e))| (*name, Enum::from_normalized(context, e, index)))
            .collect();
        Self {
            id: ModuleId::from_normalized(context, &normalized.id),
            doc: None,        // set by ProgramInfo
            attributes: None, // set by ProgramInfo
            immediate_dependencies,
            functions,
            structs,
            enums,
        }
    }
}

impl Function {
    pub fn from_normalized(
        context: &Context,
        normalized: &normalized::Function,
        index: usize,
    ) -> Self {
        let visibility = match normalized.visibility {
            file_format::Visibility::Public => Visibility::Public,
            file_format::Visibility::Friend => Visibility::Friend,
            file_format::Visibility::Private => Visibility::Private,
        };
        let type_parameters = normalized
            .type_parameters
            .iter()
            .map(|tp| TParam {
                name: None, // set by ProgramInfo
                constraints: AbilitySet::from_file_format(*tp),
            })
            .collect();
        let parameters = normalized
            .parameters
            .iter()
            .map(|t| Parameter {
                name: None, // set by ProgramInfo
                type_: Type::from_normalized(context, t),
            })
            .collect();
        let return_ = normalized
            .return_
            .iter()
            .map(|t| Type::from_normalized(context, t))
            .collect();
        Self {
            index,
            source_index: None, // set by ProgramInfo
            doc: None,          // set by ProgramInfo
            attributes: None,   // set by ProgramInfo
            visibility,
            entry: normalized.is_entry,
            macro_: None, // set by ProgramInfo
            type_parameters,
            parameters,
            return_,
        }
    }
}

impl AbilitySet {
    pub fn from_file_format(abilities: file_format::AbilitySet) -> Self {
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

impl Struct {
    pub fn from_normalized(
        context: &Context,
        normalized: &normalized::Struct,
        index: usize,
    ) -> Self {
        let abilities = AbilitySet::from_file_format(normalized.abilities);
        let type_parameters = normalized
            .type_parameters
            .iter()
            .copied()
            .map(DatatypeTParam::from_file_format)
            .collect();
        let fields = Fields {
            positional_fields: false, // set by ProgramInfo
            fields: normalized
                .fields
                .0
                .iter()
                .enumerate()
                .map(|(idx, (n, f))| (*n, Field::from_normalized(context, f, idx)))
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

impl Enum {
    pub fn from_normalized(context: &Context, normalized: &normalized::Enum, index: usize) -> Self {
        let abilities = AbilitySet::from_file_format(normalized.abilities);
        let type_parameters = normalized
            .type_parameters
            .iter()
            .copied()
            .map(DatatypeTParam::from_file_format)
            .collect();
        let variants = normalized
            .variants
            .iter()
            .enumerate()
            .map(|(idx, (name, v))| (*name, Variant::from_normalized(context, v, idx)))
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

impl Variant {
    pub fn from_normalized(
        context: &Context,
        normalized: &normalized::Variant,
        index: usize,
    ) -> Self {
        let fields = Fields {
            positional_fields: false, // set by ProgramInfo
            fields: normalized
                .fields
                .0
                .iter()
                .enumerate()
                .map(|(idx, (n, f))| (*n, Field::from_normalized(context, f, idx)))
                .collect(),
        };
        Self {
            index,
            doc: None, // set by ProgramInfo
            fields,
        }
    }
}

impl Field {
    pub fn from_normalized(
        context: &Context,
        normalized: &normalized::Field,
        index: usize,
    ) -> Self {
        Self {
            index,
            doc: None, // set by ProgramInfo
            type_: Type::from_normalized(context, &normalized.type_),
        }
    }
}

impl DatatypeTParam {
    pub fn from_file_format(tparam: file_format::DatatypeTyParameter) -> Self {
        Self {
            phantom: tparam.is_phantom,
            tparam: TParam {
                name: None, // set by ProgramInfo
                constraints: AbilitySet::from_file_format(tparam.constraints),
            },
        }
    }
}

impl Type {
    pub fn from_normalized(context: &Context, normalized: &normalized::Type) -> Self {
        match normalized {
            normalized::Type::Bool => Self::Bool,
            normalized::Type::U8 => Self::U8,
            normalized::Type::U16 => Self::U16,
            normalized::Type::U32 => Self::U32,
            normalized::Type::U64 => Self::U64,
            normalized::Type::U128 => Self::U128,
            normalized::Type::U256 => Self::U256,
            normalized::Type::Address => Self::Address,
            normalized::Type::Signer => Self::Signer,
            normalized::Type::Datatype(d) => {
                let name = d.name;
                Type::Datatype(Box::new(Datatype {
                    module: ModuleId::from_normalized(context, &d.module),
                    name,
                    type_arguments: d
                        .type_arguments
                        .iter()
                        .enumerate()
                        .map(|(idx, ty)| {
                            let ty = Self::from_normalized(context, ty);
                            DatatypeTArg::new(context, &d.module, &name, idx, ty)
                        })
                        .collect(),
                }))
            }
            normalized::Type::Vector(t) => {
                Type::Vector(Box::new(Self::from_normalized(context, t)))
            }
            normalized::Type::Reference(is_mut, t) => {
                Type::Reference(*is_mut, Box::new(Self::from_normalized(context, t)))
            }
            normalized::Type::TypeParameter(t) => Self::TypeParameter(*t),
        }
    }
}

impl DatatypeTArg {
    pub fn new(
        context: &Context,
        module: &normalized::ModuleId,
        name: &Symbol,
        idx: usize,
        ty: Type,
    ) -> Self {
        Self {
            phantom: context.phantom_type_positions[module][name][idx],
            argument: ty,
        }
    }
}

impl ModuleId {
    pub fn from_normalized(context: &Context, normalized: &normalized::ModuleId) -> Self {
        let address = context
            .root_named_address_reverse_map
            .get(&normalized.address)
            .copied()
            .unwrap_or_else(|| {
                format!(
                    "{}",
                    normalized
                        .address
                        .to_canonical_display(/* with_prefix */ true)
                )
                .into()
            });
        Self {
            address,
            name: normalized.name,
        }
    }
}

//**************************************************************************************************
// Annotation
//**************************************************************************************************

impl Packages {
    pub fn annotate(&mut self, context: &Context, model: &source_model::Model) {
        for (address, package) in &mut self.packages {
            package.annotate(context, &model.package(address))
        }
    }
}

impl Package {
    pub fn annotate(&mut self, context: &Context, package: &source_model::Package) {
        debug_assert!(self.name.is_none());
        self.name = package.name();
        for (name, module) in &mut self.modules {
            module.annotate(context, &package.module(*name));
        }
    }
}

impl Module {
    pub fn annotate(&mut self, context: &Context, module: &source_model::Module) {
        debug_assert!(self.doc.is_none());
        debug_assert!(self.attributes.is_none());
        let info = module.info();
        self.doc = Some(doc_comment(&info.doc));
        self.attributes = Some(attributes(&info.attributes));
        for (name, f) in &mut self.functions {
            f.annotate(context, &module.function(*name));
        }
        for (name, finfo) in info
            .functions
            .key_cloned_iter()
            .filter(|(_, finfo)| finfo.macro_.is_some())
        {
            self.functions
                .insert(name.0.value, Function::from_macro(context, finfo));
        }

        for (name, s) in &mut self.structs {
            s.annotate(context, &module.struct_(*name));
        }
        for (name, e) in &mut self.enums {
            e.annotate(context, &module.enum_(*name));
        }
    }
}

impl Function {
    pub fn annotate(&mut self, context: &Context, function: &source_model::Function) {
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
                *type_ = Type::from_ast(context, param_ty);
            });
        self.return_ = Type::multiple_from_ast(context, &info.signature.return_type);
    }

    pub fn from_macro(context: &Context, finfo: &FunctionInfo) -> Self {
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
                    type_: Type::from_ast(context, type_),
                })
                .collect(),
            return_: Type::multiple_from_ast(context, &finfo.signature.return_type),
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
    pub fn annotate(&mut self, context: &Context, s: &source_model::Struct) {
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
        self.fields.annotate_struct(context, &info.fields);
    }
}

impl Enum {
    pub fn annotate(&mut self, context: &Context, e: &source_model::Enum) {
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
            v.annotate(context, &e.variant(*name));
        }
    }
}

impl Variant {
    pub fn annotate(&mut self, context: &Context, v: &source_model::Variant) {
        debug_assert!(self.doc.is_none());
        let info = v.info();
        self.doc = Some(doc_comment(&info.doc));
        self.fields.annotate_variant(context, &info.fields);
    }
}

impl Fields {
    pub fn annotate_struct(&mut self, context: &Context, fields: &N::StructFields) {
        debug_assert!(!self.positional_fields);
        let (is_positional, fields) = match fields {
            N::StructFields::Defined(is_positional, fields) => (*is_positional, fields),
            N::StructFields::Native(_) => return,
        };
        self.positional_fields = is_positional;
        let pos_name_of = if self.positional_fields {
            |sym| Symbol::from(format!("pos{}", sym))
        } else {
            |sym| sym
        };
        for (name, (_, (doc, ty))) in fields.key_cloned_iter() {
            let field = self.fields.get_mut(&pos_name_of(name.0.value)).unwrap();
            debug_assert!(field.doc.is_none());
            field.doc = Some(doc_comment(doc));
            field.type_ = Type::from_ast(context, ty);
        }
    }

    pub fn annotate_variant(&mut self, context: &Context, fields: &N::VariantFields) {
        debug_assert!(!self.positional_fields);
        let (is_positional, fields) = match fields {
            N::VariantFields::Defined(is_positional, fields) => (*is_positional, fields),
            N::VariantFields::Empty => return,
        };
        self.positional_fields = is_positional;
        let pos_name_of = if self.positional_fields {
            |sym| Symbol::from(format!("pos{}", sym))
        } else {
            |sym| sym
        };
        for (name, (_, (doc, ty))) in fields.key_cloned_iter() {
            let field = self.fields.get_mut(&pos_name_of(name.0.value)).unwrap();
            debug_assert!(field.doc.is_none());
            field.doc = Some(doc_comment(doc));
            field.type_ = Type::from_ast(context, ty);
        }
    }
}

impl Type {
    fn multiple_from_ast(context: &Context, ty @ sp!(_, ty_): &N::Type) -> Vec<Self> {
        match ty_ {
            N::Type_::Unit => vec![],
            N::Type_::Apply(_, sp!(_, N::TypeName_::Multiple(_)), tys) => {
                tys.iter().map(|ty| Type::from_ast(context, ty)).collect()
            }
            _ => {
                vec![Type::from_ast(context, ty)]
            }
        }
    }

    fn from_ast(context: &Context, sp!(_, ty_): &N::Type) -> Self {
        match ty_ {
            N::Type_::Unit => Self::Tuple(vec![]),
            N::Type_::Ref(mut_, inner) => {
                Type::Reference(*mut_, Box::new(Self::from_ast(context, inner)))
            }
            N::Type_::Param(tp) => Type::NamedTypeParameter(tp.user_specified_name.value),
            N::Type_::Apply(_, sp!(_, tn_), tys) => match tn_ {
                N::TypeName_::ModuleType(m, n) => {
                    let normalized_id = m.value.module_id();
                    let name = n.0.value;
                    Self::Datatype(Box::new(Datatype {
                        module: ModuleId::from_normalized(context, &normalized_id),
                        name,
                        type_arguments: tys
                            .iter()
                            .enumerate()
                            .map(|(idx, ty)| {
                                let ty = Self::from_ast(context, ty);
                                DatatypeTArg::new(context, &normalized_id, &name, idx, ty)
                            })
                            .collect(),
                    }))
                }
                N::TypeName_::Multiple(_) => {
                    if tys.len() == 1 {
                        Self::from_ast(context, &tys[0])
                    } else {
                        Type::Tuple(tys.iter().map(|ty| Self::from_ast(context, ty)).collect())
                    }
                }
                N::TypeName_::Builtin(sp!(_, bt)) => match bt {
                    N::BuiltinTypeName_::Bool => Self::Bool,
                    N::BuiltinTypeName_::U8 => Self::U8,
                    N::BuiltinTypeName_::U16 => Self::U16,
                    N::BuiltinTypeName_::U32 => Self::U32,
                    N::BuiltinTypeName_::U64 => Self::U64,
                    N::BuiltinTypeName_::U128 => Self::U128,
                    N::BuiltinTypeName_::U256 => Self::U256,
                    N::BuiltinTypeName_::Address => Self::Address,
                    N::BuiltinTypeName_::Signer => Self::Signer,
                    N::BuiltinTypeName_::Vector => {
                        Self::Vector(Box::new(Self::from_ast(context, &tys[0])))
                    }
                },
            },
            N::Type_::Fun(params, ret_) => Type::Fun(
                params
                    .iter()
                    .map(|ty| Self::from_ast(context, ty))
                    .collect(),
                Box::new(Self::from_ast(context, ret_)),
            ),
            N::Type_::Var(_) | N::Type_::Anything | N::Type_::UnresolvedError => Self::Any,
        }
    }
}

//**************************************************************************************************
// FromSource annotations
//**************************************************************************************************

fn doc_comment(doc: &DocComment) -> Option<String> {
    doc.comment().map(|c| c.to_string())
}

fn attributes(attributes: &E::Attributes) -> Vec<Attribute> {
    attributes
        .iter()
        .map(|(_, _, a)| attribute(&a.value))
        .collect()
}

fn ext_attribute(entry: &KA::ExternalAttributeEntry) -> Attribute {
    use KA::ExternalAttributeEntry_ as EAE;
    use KA::ExternalAttributeValue_ as EAV;
    match &entry.value {
        EAE::Name(n) => Attribute::Name(n.value),
        EAE::Assigned(n, boxed_val) => {
            let s = match &boxed_val.value {
                EAV::Value(v) => attribute_assigned_value(v),
                EAV::Address(a) => attribute_address(a),
                EAV::Module(m) => attribute_module_ident(m),
                EAV::ModuleAccess(ma) => attrribute_module_access(ma),
            };
            Attribute::Assigned(n.value, s)
        }
        EAE::Parameterized(n, nested) => {
            let inner: Attributes = nested.iter().map(|(_, _, e)| ext_attribute(e)).collect();
            Attribute::Parameterized(n.value, inner)
        }
    }
}

fn attribute(k: &KA::KnownAttribute) -> Attribute {
    match k {
        // --- name-only ---
        KA::KnownAttribute::BytecodeInstruction(_) => {
            Attribute::Name(KA::BytecodeInstructionAttribute::BYTECODE_INSTRUCTION.into())
        }
        KA::KnownAttribute::Testing(KA::TestingAttribute::Test) => {
            Attribute::Name(KA::TestingAttribute::TEST.into())
        }
        KA::KnownAttribute::Testing(KA::TestingAttribute::RandTest) => {
            Attribute::Name(KA::TestingAttribute::RAND_TEST.into())
        }
        KA::KnownAttribute::Mode(KA::ModeAttribute { modes }) => {
            let inner = modes
                .iter()
                .map(|(_, name)| Attribute::Name(*name))
                .collect();
            Attribute::Parameterized(KA::ModeAttribute::MODE.into(), inner)
        }

        // --- assigned or name ---
        KA::KnownAttribute::Deprecation(dep) => {
            if let Some(note) = &dep.note {
                let s = String::from_utf8_lossy(note).into_owned();
                Attribute::Parameterized(
                    KA::DeprecationAttribute::DEPRECATED.into(),
                    vec![Attribute::Assigned(
                        KA::DeprecationAttribute::NOTE.into(),
                        s,
                    )],
                )
            } else {
                Attribute::Name(KA::DeprecationAttribute::DEPRECATED.into())
            }
        }
        KA::KnownAttribute::Error(err) => {
            if let Some(code) = err.code {
                Attribute::Parameterized(
                    KA::ErrorAttribute::ERROR.into(),
                    vec![Attribute::Assigned(
                        KA::ErrorAttribute::CODE.into(),
                        format!("{}", code),
                    )],
                )
            } else {
                Attribute::Name(KA::ErrorAttribute::ERROR.into())
            }
        }

        // --- single-argument parameterized ---
        KA::KnownAttribute::DefinesPrimitive(dp) => {
            let inner = vec![Attribute::Name(dp.name.value)];
            Attribute::Parameterized(KA::DefinesPrimitiveAttribute::DEFINES_PRIM.into(), inner)
        }
        KA::KnownAttribute::Syntax(sx) => {
            let inner = vec![Attribute::Name(sx.kind.value)];
            Attribute::Parameterized(KA::SyntaxAttribute::SYNTAX.into(), inner)
        }

        // --- diagnostics ---
        KA::KnownAttribute::Diagnostic(diag_attr) => match diag_attr {
            KA::DiagnosticAttribute::Allow { allow_set } => {
                let mut inner = Vec::new();
                for (prefix_opt, name) in allow_set {
                    if let Some(pref) = prefix_opt {
                        let grp = vec![Attribute::Name(name.value)];
                        inner.push(Attribute::Parameterized(pref.value, grp));
                    } else {
                        inner.push(Attribute::Name(name.value));
                    }
                }
                Attribute::Parameterized(KA::DiagnosticAttribute::ALLOW.into(), inner)
            }
            KA::DiagnosticAttribute::LintAllow { allow_set } => {
                let inner = allow_set
                    .iter()
                    .map(|name| Attribute::Name(name.value))
                    .collect();
                Attribute::Parameterized(KA::DiagnosticAttribute::LINT_ALLOW.into(), inner)
            }
        },

        // --- expected_failure ---
        KA::KnownAttribute::Testing(KA::TestingAttribute::ExpectedFailure(ef)) => {
            let mut inner = Vec::new();
            match &**ef {
                KA::ExpectedFailure::Expected => {
                    Attribute::Name(KA::TestingAttribute::EXPECTED_FAILURE.into())
                }
                KA::ExpectedFailure::ExpectedWithCodeDEPRECATED(code) => {
                    inner.push(Attribute::Assigned(
                        KA::TestingAttribute::ABORT_CODE_NAME.into(),
                        code.to_string(),
                    ));
                    Attribute::Parameterized(KA::TestingAttribute::EXPECTED_FAILURE.into(), inner)
                }
                KA::ExpectedFailure::ExpectedWithError {
                    status_code,
                    minor_code,
                    location,
                } => {
                    let status_code = match status_code {
                        StatusCode::OUT_OF_GAS => KA::TestingAttribute::OUT_OF_GAS_NAME.to_owned(),
                        StatusCode::VECTOR_OPERATION_ERROR => {
                            KA::TestingAttribute::VECTOR_ERROR_NAME.to_owned()
                        }
                        StatusCode::ARITHMETIC_ERROR => {
                            KA::TestingAttribute::ARITHMETIC_ERROR_NAME.to_owned()
                        }
                        other => format!("{}", *other as u64),
                    };
                    inner.push(Attribute::Assigned(
                        KA::TestingAttribute::MAJOR_STATUS_NAME.into(),
                        status_code,
                    ));
                    if let Some(sp!(_, mc)) = minor_code {
                        let s = match mc {
                            KA::MinorCode_::Value(v) => format!("{}", v),
                            KA::MinorCode_::Constant(mident, name) => {
                                format!("{}::{}", mident, name)
                            }
                        };
                        inner.push(Attribute::Assigned(
                            KA::TestingAttribute::MINOR_STATUS_NAME.into(),
                            s,
                        ));
                    }
                    inner.push(Attribute::Assigned(
                        KA::TestingAttribute::ERROR_LOCATION.into(),
                        attribute_module_ident(location),
                    ));
                    Attribute::Parameterized(KA::TestingAttribute::EXPECTED_FAILURE.into(), inner)
                }
            }
        }

        // --- external ---
        KA::KnownAttribute::External(ext) => {
            let inner = ext
                .attrs
                .iter()
                .map(|(_, _, entry)| ext_attribute(entry))
                .collect();
            Attribute::Parameterized(KA::ExternalAttribute::EXTERNAL.into(), inner)
        }
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
