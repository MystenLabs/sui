// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    debug_display, diag,
    diagnostics::Diagnostic,
    expansion::{
        alias_map_builder::{LeadingAccessEntry, MemberEntry},
        ast as E,
    },
    naming::ast as N,
    parser::ast as P,
    shared::string_utils::format_oxford_list,
    shared::Name,
    typing::ast as T,
    unit_test::filter_test_members::UNIT_TEST_POISON_FUN_NAME,
};

use move_core_types::parsing::address::NumericalAddress;
use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

//*************************************************************************************************
// Types
//*************************************************************************************************

#[derive(Debug, Clone, Default)]
pub struct IDEInfo {
    pub(crate) annotations: Vec<(Loc, IDEAnnotation)>,
}

#[derive(Debug, Clone)]
/// An individual IDE annotation.
pub enum IDEAnnotation {
    /// A macro call site.
    MacroCallInfo(Box<MacroCallInfo>),
    /// An expanded lambda site.
    ExpandedLambda,
    /// Autocomplete information.
    DotAutocompleteInfo(Box<DotAutocompleteInfo>),
    /// Autocomplete information.
    PathAutocompleteInfo(Box<AliasAutocompleteInfo>),
    /// Match Missing Arm.
    MissingMatchArms(Box<MissingMatchArmsInfo>),
    /// Ellipsis Match Arm.
    EllipsisMatchEntries(Box<EllipsisMatchEntries>),
}

#[derive(Debug, Clone)]
pub struct MacroCallInfo {
    /// Module where the macro is defined
    pub module: E::ModuleIdent,
    /// Name of the macro function
    pub name: P::FunctionName,
    /// Optional method name if macro invoked as dot-call
    pub method_name: Option<Name>,
    /// Type params at macro's call site
    pub type_arguments: Vec<N::Type>,
    /// By-value args (at this point there should only be one, representing receiver arg)
    pub by_value_args: Vec<T::SequenceItem>,
}

#[derive(Debug, Clone, PartialOrd, Ord, PartialEq, Eq)]
pub struct AutocompleteMethod {
    pub method_name: Symbol,
    pub target_function: (E::ModuleIdent, P::FunctionName),
}

#[derive(Debug, Clone)]
pub struct DotAutocompleteInfo {
    /// Methods that are valid auto-completes
    pub methods: Vec<AutocompleteMethod>,
    /// Fields that are valid auto-completes (e.g., for a struct) along with their types
    pub fields: Vec<(Symbol, N::Type)>,
}

#[derive(Default, Debug, Clone)]
pub struct AliasAutocompleteInfo {
    /// Numerical addresses that are valid autocompletes
    pub addresses: BTreeMap<Symbol, NumericalAddress>,
    /// Modules that are valid autocompletes
    pub modules: BTreeMap<Symbol, E::ModuleIdent>,
    /// Members that are valid autocompletes
    pub members: BTreeSet<(Symbol, E::ModuleIdent, Name)>,
    /// Type parameters that are valid autocompletes
    pub type_params: BTreeSet<Symbol>,
}

#[derive(Debug, Clone)]
pub struct MissingMatchArmsInfo {
    /// A vector of arm patterns that can be inserted to make the match complete.
    /// Note the span information on these is _wrong_ and must be recomputed after insertion.
    pub arms: Vec<PatternSuggestion>,
}

/// Suggested new entries for a pattern. Note that any location information points to the
/// definition site. As this is largely suggested text, it lacks location information.
#[derive(Debug, Clone)]
pub enum PatternSuggestion {
    Wildcard,
    Binder(Symbol),
    Value(E::Value_),
    UnpackPositionalStruct {
        module: E::ModuleIdent,
        name: P::DatatypeName,
        /// The number of wildcards to generate.
        field_count: usize,
    },
    UnpackNamedStruct {
        module: E::ModuleIdent,
        name: P::DatatypeName,
        /// The fields, in order, to generate
        fields: Vec<Symbol>,
    },
    /// A tag-style variant that takes no arguments
    UnpackEmptyVariant {
        module: E::ModuleIdent,
        enum_name: P::DatatypeName,
        variant_name: P::VariantName,
    },
    UnpackPositionalVariant {
        module: E::ModuleIdent,
        enum_name: P::DatatypeName,
        variant_name: P::VariantName,
        /// The number of wildcards to generate.
        field_count: usize,
    },
    UnpackNamedVariant {
        module: E::ModuleIdent,
        enum_name: P::DatatypeName,
        variant_name: P::VariantName,
        /// The fields, in order, to generate
        fields: Vec<Symbol>,
    },
}

#[derive(Debug, Clone)]
pub enum EllipsisMatchEntries {
    /// A number of wildcards inserted for the ellipsis for a positional match.
    Positional(Vec<Symbol>),
    /// A list of symbols mapped to wildcards that are added to a named match.
    Named(Vec<Symbol>),
}

//*************************************************************************************************
// Impls
//*************************************************************************************************

impl AutocompleteMethod {
    pub fn new(method_name: Symbol, target_function: (E::ModuleIdent, P::FunctionName)) -> Self {
        Self {
            method_name,
            target_function,
        }
    }
}

impl IDEInfo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_ide_annotation(&mut self, loc: Loc, info: IDEAnnotation) {
        self.annotations.push((loc, info));
    }

    pub fn extend(&mut self, mut other: Self) {
        self.annotations.append(&mut other.annotations);
    }

    pub fn is_empty(&self) -> bool {
        self.annotations.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, (Loc, IDEAnnotation)> {
        self.annotations.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, (Loc, IDEAnnotation)> {
        self.annotations.iter_mut()
    }
}

impl IntoIterator for IDEInfo {
    type Item = (Loc, IDEAnnotation);
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.annotations.into_iter()
    }
}

impl AliasAutocompleteInfo {
    pub fn new() -> Self {
        Self::default()
    }
}

impl
    From<(
        BTreeMap<Symbol, LeadingAccessEntry>,
        BTreeMap<Symbol, MemberEntry>,
    )> for AliasAutocompleteInfo
{
    fn from(
        (leading_names, member_names): (
            BTreeMap<Symbol, LeadingAccessEntry>,
            BTreeMap<Symbol, MemberEntry>,
        ),
    ) -> Self {
        let mut addresses: BTreeMap<Symbol, NumericalAddress> = BTreeMap::new();
        let mut modules: BTreeMap<Symbol, E::ModuleIdent> = BTreeMap::new();
        let mut members: BTreeSet<(Symbol, E::ModuleIdent, Name)> = BTreeSet::new();
        let mut type_params: BTreeSet<Symbol> = BTreeSet::new();

        for (symbol, entry) in leading_names
            .iter()
            .filter(|(symbol, _)| symbol.to_string() != UNIT_TEST_POISON_FUN_NAME.to_string())
        {
            match entry {
                LeadingAccessEntry::Address(addr) => {
                    addresses.insert(*symbol, *addr);
                }
                LeadingAccessEntry::Module(mident) => {
                    modules.insert(*symbol, *mident);
                }
                LeadingAccessEntry::Member(mident, name) => {
                    members.insert((*symbol, *mident, *name));
                }
                LeadingAccessEntry::TypeParam => {
                    type_params.insert(*symbol);
                }
            }
        }

        // The member names shadow, though this should be no issue as they should be identical.
        for (symbol, entry) in member_names
            .iter()
            .filter(|(symbol, _)| symbol.to_string() != UNIT_TEST_POISON_FUN_NAME.to_string())
        {
            match entry {
                MemberEntry::Member(mident, name) => {
                    members.insert((*symbol, *mident, *name));
                }
                MemberEntry::TypeParam => {
                    type_params.insert(*symbol);
                }
            }
        }

        AliasAutocompleteInfo {
            members,
            modules,
            addresses,
            type_params,
        }
    }
}

impl From<(Loc, IDEAnnotation)> for Diagnostic {
    fn from((loc, ann): (Loc, IDEAnnotation)) -> Self {
        match ann {
            IDEAnnotation::MacroCallInfo(info) => {
                let MacroCallInfo {
                    module,
                    name,
                    method_name,
                    type_arguments,
                    by_value_args,
                } = *info;
                let mut diag = diag!(IDE::MacroCallInfo, (loc, "macro call info"));
                diag.add_note(format!("Called {module}::{name}"));
                if let Some(mname) = method_name {
                    diag.add_note(format!("as method call {mname}"));
                }
                if !type_arguments.is_empty() {
                    let tyargs_string = debug_display!(type_arguments).to_string();
                    diag.add_note(format!("Type arguments: {tyargs_string}"));
                }
                if let Some(entry) = by_value_args.first() {
                    let subject_arg_string = debug_display!(entry).to_string();
                    diag.add_note(format!("Subject arg: {subject_arg_string}"));
                }
                diag
            }
            IDEAnnotation::ExpandedLambda => {
                diag!(IDE::ExpandedLambda, (loc, "expanded lambda"))
            }
            IDEAnnotation::PathAutocompleteInfo(info) => {
                let AliasAutocompleteInfo {
                    members,
                    modules,
                    addresses,
                    type_params,
                } = *info;

                let members = members
                    .into_iter()
                    .map(|(name, m, f)| format!("{name} -> {m}::{f}"));
                let member_names = format_oxford_list!(ITER, "or", "'{}'", members);
                let modules = modules
                    .into_iter()
                    .map(|(name, m)| format!("{name} -> {m}"));
                let module_names = format_oxford_list!(ITER, "or", "'{}'", modules);
                let addrs = addresses
                    .into_iter()
                    .map(|(name, a)| format!("{name} -> {a}"));
                let address_names = format_oxford_list!(ITER, "or", "'{}'", addrs);
                let type_params = type_params.into_iter().map(|p| format!("{p}"));
                let type_param_names = format_oxford_list!(ITER, "or", "'{}'", type_params);
                let mut diag = diag!(IDE::PathAutocomplete, (loc, "Possible in-scope names"));
                diag.add_note(format!("members: {member_names}"));
                diag.add_note(format!("modules: {module_names}"));
                diag.add_note(format!("addresses: {address_names}"));
                diag.add_note(format!("type params: {type_param_names}"));
                diag
            }
            IDEAnnotation::DotAutocompleteInfo(info) => {
                let DotAutocompleteInfo { methods, fields } = *info;
                let names = methods
                    .into_iter()
                    .map(
                        |AutocompleteMethod {
                             method_name,
                             target_function: (mident, _),
                         }| format!("{mident}::{method_name}"),
                    )
                    .chain(fields.into_iter().map(|(n, _)| format!("{n}")))
                    .collect::<Vec<_>>();
                let msg = format!(
                    "Possible dot names: {}",
                    format_oxford_list!("or", "'{}'", names)
                );
                diag!(IDE::DotAutocomplete, (loc, msg))
            }
            IDEAnnotation::MissingMatchArms(info) => {
                let MissingMatchArmsInfo { arms } = *info;
                let msg = format!("Missing arms: {}", format_oxford_list!("and", "'{}'", arms));
                diag!(IDE::MissingMatchArms, (loc, msg))
            }
            IDEAnnotation::EllipsisMatchEntries(entries) => {
                let entries = match *entries {
                    EllipsisMatchEntries::Positional(entries) => entries
                        .iter()
                        .map(|name| format!("{}", name))
                        .collect::<Vec<_>>()
                        .join(", "),
                    EllipsisMatchEntries::Named(entries) => entries
                        .iter()
                        .map(|name| format!("{}: _", name))
                        .collect::<Vec<_>>()
                        .join(", "),
                };
                let msg = format!("Ellipsis expansion: {}", entries);
                diag!(IDE::EllipsisExpansion, (loc, msg))
            }
        }
    }
}

impl fmt::Display for PatternSuggestion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use PatternSuggestion as PS;
        match self {
            PS::Wildcard => write!(f, "_"),
            PS::Binder(n) => write!(f, "{n}"),
            PS::Value(v) => write!(f, "{v}"),
            PS::UnpackPositionalStruct {
                module,
                name,
                field_count,
            } => {
                write!(f, "{module}::{name}")?;
                let wildcards = std::iter::repeat("_")
                    .take(*field_count)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "({wildcards})")
            }
            PS::UnpackPositionalVariant {
                module,
                enum_name,
                variant_name,
                field_count,
            } => {
                write!(f, "{module}::{enum_name}::{variant_name}")?;
                let wildcards = std::iter::repeat("_")
                    .take(*field_count)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "({wildcards})")
            }
            PS::UnpackNamedStruct {
                module,
                name,
                fields,
            } => {
                write!(f, "{module}::{name} ")?;
                let field_names = fields
                    .iter()
                    .map(|name| format!("{}", name))
                    .collect::<Vec<_>>()
                    .join(" , ");
                write!(f, "{{ {field_names} }}")
            }
            PS::UnpackNamedVariant {
                module,
                enum_name,
                variant_name,
                fields,
            } => {
                write!(f, "{module}::{enum_name}::{variant_name} ")?;
                let field_names = fields
                    .iter()
                    .map(|name| format!("{}", name))
                    .collect::<Vec<_>>()
                    .join(" , ");
                write!(f, "{{ {field_names} }}")
            }
            PS::UnpackEmptyVariant {
                module,
                enum_name,
                variant_name,
            } => write!(f, "{module}::{enum_name}::{variant_name}"),
        }
    }
}
