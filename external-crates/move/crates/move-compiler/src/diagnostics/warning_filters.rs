// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diagnostics::{
        codes::{Category, DiagnosticInfo, ExternalPrefix, Severity, UnusedItem},
        Diagnostic, DiagnosticCode,
    },
    shared::{known_attributes, AstDebug},
};
use move_symbol_pool::Symbol;
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    hash::Hash,
    sync::Arc,
};

pub const FILTER_ALL: &str = "all";
pub const FILTER_UNUSED: &str = "unused";
pub const FILTER_MISSING_PHANTOM: &str = "missing_phantom";
pub const FILTER_UNUSED_USE: &str = "unused_use";
pub const FILTER_UNUSED_VARIABLE: &str = "unused_variable";
pub const FILTER_UNUSED_ASSIGNMENT: &str = "unused_assignment";
pub const FILTER_UNUSED_TRAILING_SEMI: &str = "unused_trailing_semi";
pub const FILTER_UNUSED_ATTRIBUTE: &str = "unused_attribute";
pub const FILTER_UNUSED_TYPE_PARAMETER: &str = "unused_type_parameter";
pub const FILTER_UNUSED_FUNCTION: &str = "unused_function";
pub const FILTER_UNUSED_STRUCT_FIELD: &str = "unused_field";
pub const FILTER_UNUSED_CONST: &str = "unused_const";
pub const FILTER_DEAD_CODE: &str = "dead_code";
pub const FILTER_UNUSED_LET_MUT: &str = "unused_let_mut";
pub const FILTER_UNUSED_MUT_REF: &str = "unused_mut_ref";
pub const FILTER_UNUSED_MUT_PARAM: &str = "unused_mut_parameter";
pub const FILTER_IMPLICIT_CONST_COPY: &str = "implicit_const_copy";
pub const FILTER_DUPLICATE_ALIAS: &str = "duplicate_alias";
pub const FILTER_DEPRECATED: &str = "deprecated_usage";
pub const FILTER_IDE_PATH_AUTOCOMPLETE: &str = "ide_path_autocomplete";
pub const FILTER_IDE_DOT_AUTOCOMPLETE: &str = "ide_dot_autocomplete";

macro_rules! known_code_filter {
    ($name:ident, $category:ident::$code:ident) => {{
        use crate::diagnostics::codes::*;
        (
            move_symbol_pool::Symbol::from($name),
            std::collections::BTreeSet::from([
                crate::diagnostics::warning_filters::WarningFilter::Code {
                    prefix: None,
                    category: Category::$category as u8,
                    code: $category::$code as u8,
                    name: Some($name),
                },
            ]),
        )
    }};
}
pub(crate) use known_code_filter;

//**************************************************************************************************
// Types
//**************************************************************************************************

/// None for the default 'allow'.
/// Some(prefix) for a custom set of warnings, e.g. 'allow(lint(_))'.
pub type FilterPrefix = Option<Symbol>;
pub type FilterName = Symbol;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct WarningFiltersScope(WarningFiltersScope_);

#[derive(PartialEq, Eq, Clone, Debug)]
enum WarningFiltersScope_ {
    /// Unsafe and should be used only for internal purposes, such as ide annotations
    Empty,
    /// The top-level warning filters given to the compiler instance. They are leaked as they will
    /// be needed for the lifetime of the compiler instance.
    Static(&'static WarningFiltersBuilder),
    /// A user-defined warning filter scope, with a reference to the previous scope
    Node(Arc<WarningFiltersScopeNode>),
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct WarningFiltersScopeNode {
    /// The warning filters for this scope
    filters: WarningFilters,
    /// The previous scope
    prev: WarningFiltersScope_,
}

#[derive(Debug, Clone)]
/// An intern table for warning filters. The underlying `Box` is not moved, so the pointer to the
/// filter is stable.
/// Safety: This table should not be dropped as long as any `WarningFilters` are alive
pub struct WarningFiltersTable(HashSet<Box<WarningFiltersBuilder>>);

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
/// An unsafe pointer into the intern table for warning filters.
/// Safety: The `WarningFiltersTable` must be held as long as any `WarningFilters`s are alive.
pub struct WarningFilters(*const WarningFiltersBuilder);
unsafe impl Send for WarningFilters {}
unsafe impl Sync for WarningFilters {}

#[derive(PartialEq, Eq, Clone, Debug, PartialOrd, Ord, Hash)]
/// Used to filter out diagnostics, specifically used for warning suppression
pub struct WarningFiltersBuilder {
    filters: BTreeMap<ExternalPrefix, UnprefixedWarningFilters>,
    for_dependency: bool, // if false, the filters are used for source code
}

#[derive(PartialEq, Eq, Clone, Debug, PartialOrd, Ord, Hash)]
/// Filters split by category and code
enum UnprefixedWarningFilters {
    /// Remove all warnings
    All,
    Specified {
        /// Remove all diags of this category with optional known name
        categories: BTreeMap<u8, Option<WellKnownFilterName>>,
        /// Remove specific diags with optional known filter name
        codes: BTreeMap<(u8, u8), Option<WellKnownFilterName>>,
    },
    /// No filter
    Empty,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, PartialOrd, Ord)]
/// Represents a single annotation for a diagnostic filter
pub enum WarningFilter {
    /// Filters all warnings
    All(ExternalPrefix),
    /// Filters all warnings of a specific category. Only known filters have names.
    Category {
        prefix: ExternalPrefix,
        category: u8,
        name: Option<WellKnownFilterName>,
    },
    /// Filters a single warning, as defined by codes below. Only known filters have names.
    Code {
        prefix: ExternalPrefix,
        category: u8,
        code: u8,
        name: Option<WellKnownFilterName>,
    },
}

/// The name for a well-known filter.
pub type WellKnownFilterName = &'static str;

//**************************************************************************************************
// impls
//**************************************************************************************************

impl WarningFiltersScope {
    /// Create a new scope with the given top-level warning filter, if any.
    /// A `&'static WarningFiltersBuilder` is used to avoid cloning the filter table for each
    /// new top-level scope needed
    pub(crate) const fn root(
        top_level_warning_filter_opt: Option<&'static WarningFiltersBuilder>,
    ) -> Self {
        match top_level_warning_filter_opt {
            None => WarningFiltersScope(WarningFiltersScope_::Empty),
            Some(top_level_warning_filter) => {
                WarningFiltersScope(WarningFiltersScope_::Static(top_level_warning_filter))
            }
        }
    }

    pub fn push(&mut self, filters: WarningFilters) {
        let node = Arc::new(WarningFiltersScopeNode {
            filters,
            prev: self.0.clone(),
        });
        *self = WarningFiltersScope(WarningFiltersScope_::Node(node))
    }

    pub fn pop(&mut self) {
        match std::mem::replace(&mut self.0, WarningFiltersScope_::Empty) {
            WarningFiltersScope_::Empty => panic!("pop on empty scope"),
            WarningFiltersScope_::Static(_) => panic!("pop on top level scope"),
            WarningFiltersScope_::Node(node) => self.0 = node.prev.clone(),
        }
    }

    pub fn is_filtered(&self, diag: &Diagnostic) -> bool {
        let mut scope = &self.0;
        loop {
            match scope {
                WarningFiltersScope_::Empty => return false,
                WarningFiltersScope_::Static(filters) => return filters.is_filtered(diag),
                WarningFiltersScope_::Node(node) => {
                    if node.filters.is_filtered(diag) {
                        return true;
                    }
                    scope = &node.prev;
                }
            }
        }
    }

    pub fn is_filtered_for_dependency(&self) -> bool {
        let mut scope = &self.0;
        loop {
            match scope {
                WarningFiltersScope_::Empty => return false,
                WarningFiltersScope_::Static(filters) => return filters.for_dependency(),
                WarningFiltersScope_::Node(node) => {
                    if node.filters.for_dependency() {
                        return true;
                    }
                    scope = &node.prev;
                }
            }
        }
    }
}

impl WarningFiltersTable {
    pub fn new() -> Self {
        Self(HashSet::new())
    }

    pub fn add(&mut self, filters: WarningFiltersBuilder) -> WarningFilters {
        let boxed = Box::new(filters);
        let wf = {
            let pinned_ref: &WarningFiltersBuilder = &boxed;
            WarningFilters(pinned_ref as *const WarningFiltersBuilder)
        };
        match self.0.get(&boxed) {
            Some(existing) => {
                let existing_ref: &WarningFiltersBuilder = existing;
                WarningFilters(existing_ref as *const WarningFiltersBuilder)
            }
            None => {
                self.0.insert(boxed);
                wf
            }
        }
    }
}

impl WarningFilters {
    pub fn is_filtered(&self, diag: &Diagnostic) -> bool {
        self.borrow().is_filtered(diag)
    }

    pub fn for_dependency(&self) -> bool {
        self.borrow().for_dependency()
    }

    fn borrow(&self) -> &WarningFiltersBuilder {
        unsafe { &*self.0 }
    }
}

impl WarningFiltersBuilder {
    pub const fn new_for_source() -> Self {
        Self {
            filters: BTreeMap::new(),
            for_dependency: false,
        }
    }

    pub const fn new_for_dependency() -> Self {
        Self {
            filters: BTreeMap::new(),
            for_dependency: true,
        }
    }

    pub fn is_filtered(&self, diag: &Diagnostic) -> bool {
        self.is_filtered_by_info(&diag.info)
    }

    fn is_filtered_by_info(&self, info: &DiagnosticInfo) -> bool {
        let prefix = info.external_prefix();
        self.filters
            .get(&prefix)
            .is_some_and(|filters| filters.is_filtered_by_info(info))
    }

    pub fn union(&mut self, other: &Self) {
        for (prefix, filters) in &other.filters {
            self.filters
                .entry(*prefix)
                .or_insert_with(UnprefixedWarningFilters::new)
                .union(filters);
        }
        // if there is a dependency code filter on the stack, it means we are filtering dependent
        // code and this information must be preserved when stacking up additional filters (which
        // involves union of the current filter with the new one)
        self.for_dependency = self.for_dependency || other.for_dependency;
    }

    pub fn add(&mut self, filter: WarningFilter) {
        let (prefix, category, code, name) = match filter {
            WarningFilter::All(prefix) => {
                self.filters.insert(prefix, UnprefixedWarningFilters::All);
                return;
            }
            WarningFilter::Category {
                prefix,
                category,
                name,
            } => (prefix, category, None, name),
            WarningFilter::Code {
                prefix,
                category,
                code,
                name,
            } => (prefix, category, Some(code), name),
        };
        self.filters
            .entry(prefix)
            .or_insert(UnprefixedWarningFilters::Empty)
            .add(category, code, name)
    }

    pub fn unused_warnings_filter_for_test() -> Self {
        Self {
            filters: BTreeMap::from([(
                None,
                UnprefixedWarningFilters::unused_warnings_filter_for_test(),
            )]),
            for_dependency: false,
        }
    }

    pub fn for_dependency(&self) -> bool {
        self.for_dependency
    }
}

impl UnprefixedWarningFilters {
    pub fn new() -> Self {
        Self::Empty
    }

    fn is_filtered_by_info(&self, info: &DiagnosticInfo) -> bool {
        match self {
            Self::All => info.severity() <= Severity::Warning,
            Self::Specified { categories, codes } => {
                info.severity() <= Severity::Warning
                    && (categories.contains_key(&info.category())
                        || codes.contains_key(&(info.category(), info.code())))
            }
            Self::Empty => false,
        }
    }

    pub fn union(&mut self, other: &Self) {
        match (self, other) {
            // if self is empty, just take the other filter
            (s @ Self::Empty, _) => *s = other.clone(),
            // if other is empty, or self is ALL, no change to the filter
            (_, Self::Empty) => (),
            (Self::All, _) => (),
            // if other is all, self is now all
            (s, Self::All) => *s = Self::All,
            // category and code level union
            (
                Self::Specified { categories, codes },
                Self::Specified {
                    categories: other_categories,
                    codes: other_codes,
                },
            ) => {
                categories.extend(other_categories);
                // remove any codes covered by the category level filter
                codes.extend(
                    other_codes
                        .iter()
                        .filter(|((category, _), _)| !categories.contains_key(category)),
                );
            }
        }
    }

    /// Add a specific filter to the filter map.
    /// If filter_code is None, then the filter applies to all codes in the filter_category.
    fn add(
        &mut self,
        filter_category: u8,
        filter_code: Option<u8>,
        filter_name: Option<WellKnownFilterName>,
    ) {
        match self {
            Self::All => (),
            Self::Empty => {
                *self = Self::Specified {
                    categories: BTreeMap::new(),
                    codes: BTreeMap::new(),
                };
                self.add(filter_category, filter_code, filter_name)
            }
            Self::Specified { categories, .. } if categories.contains_key(&filter_category) => (),
            Self::Specified { categories, codes } => {
                if let Some(filter_code) = filter_code {
                    codes.insert((filter_category, filter_code), filter_name);
                } else {
                    categories.insert(filter_category, filter_name);
                    codes.retain(|(category, _), _| *category != filter_category);
                }
            }
        }
    }

    pub fn unused_warnings_filter_for_test() -> Self {
        let filtered_codes = [
            (UnusedItem::Function, FILTER_UNUSED_FUNCTION),
            (UnusedItem::StructField, FILTER_UNUSED_STRUCT_FIELD),
            (UnusedItem::FunTypeParam, FILTER_UNUSED_TYPE_PARAMETER),
            (UnusedItem::Constant, FILTER_UNUSED_CONST),
            (UnusedItem::MutReference, FILTER_UNUSED_MUT_REF),
            (UnusedItem::MutParam, FILTER_UNUSED_MUT_PARAM),
        ]
        .into_iter()
        .map(|(item, filter)| {
            let info = item.into_info();
            ((info.category(), info.code()), Some(filter))
        })
        .collect();
        Self::Specified {
            categories: BTreeMap::new(),
            codes: filtered_codes,
        }
    }
}

impl WarningFilter {
    pub fn to_str(self) -> Option<&'static str> {
        match self {
            Self::All(_) => Some(FILTER_ALL),
            Self::Category { name, .. } | Self::Code { name, .. } => name,
        }
    }

    pub fn code(
        prefix: ExternalPrefix,
        category: u8,
        code: u8,
        name: Option<WellKnownFilterName>,
    ) -> Self {
        Self::Code {
            prefix,
            category,
            code,
            name,
        }
    }

    pub fn category(
        prefix: ExternalPrefix,
        category: u8,
        name: Option<WellKnownFilterName>,
    ) -> Self {
        Self::Category {
            prefix,
            category,
            name,
        }
    }

    pub fn compiler_known_filters() -> BTreeMap<FilterName, BTreeSet<WarningFilter>> {
        BTreeMap::from([
            (
                FILTER_ALL.into(),
                BTreeSet::from([WarningFilter::All(None)]),
            ),
            (
                FILTER_UNUSED.into(),
                BTreeSet::from([WarningFilter::Category {
                    prefix: None,
                    category: Category::UnusedItem as u8,
                    name: Some(FILTER_UNUSED),
                }]),
            ),
            known_code_filter!(FILTER_MISSING_PHANTOM, Declarations::InvalidNonPhantomUse),
            known_code_filter!(FILTER_UNUSED_USE, UnusedItem::Alias),
            known_code_filter!(FILTER_UNUSED_VARIABLE, UnusedItem::Variable),
            known_code_filter!(FILTER_UNUSED_ASSIGNMENT, UnusedItem::Assignment),
            known_code_filter!(FILTER_UNUSED_TRAILING_SEMI, UnusedItem::TrailingSemi),
            known_code_filter!(FILTER_UNUSED_ATTRIBUTE, UnusedItem::Attribute),
            known_code_filter!(FILTER_UNUSED_FUNCTION, UnusedItem::Function),
            known_code_filter!(FILTER_UNUSED_STRUCT_FIELD, UnusedItem::StructField),
            (
                FILTER_UNUSED_TYPE_PARAMETER.into(),
                BTreeSet::from([
                    WarningFilter::Code {
                        prefix: None,
                        category: Category::UnusedItem as u8,
                        code: UnusedItem::StructTypeParam as u8,
                        name: Some(FILTER_UNUSED_TYPE_PARAMETER),
                    },
                    WarningFilter::Code {
                        prefix: None,
                        category: Category::UnusedItem as u8,
                        code: UnusedItem::FunTypeParam as u8,
                        name: Some(FILTER_UNUSED_TYPE_PARAMETER),
                    },
                ]),
            ),
            known_code_filter!(FILTER_UNUSED_CONST, UnusedItem::Constant),
            known_code_filter!(FILTER_DEAD_CODE, UnusedItem::DeadCode),
            known_code_filter!(FILTER_UNUSED_LET_MUT, UnusedItem::MutModifier),
            known_code_filter!(FILTER_UNUSED_MUT_REF, UnusedItem::MutReference),
            known_code_filter!(FILTER_UNUSED_MUT_PARAM, UnusedItem::MutParam),
            known_code_filter!(FILTER_IMPLICIT_CONST_COPY, TypeSafety::ImplicitConstantCopy),
            known_code_filter!(FILTER_DUPLICATE_ALIAS, Declarations::DuplicateAlias),
            known_code_filter!(FILTER_DEPRECATED, TypeSafety::DeprecatedUsage),
        ])
    }

    pub fn ide_known_filters() -> BTreeMap<FilterName, BTreeSet<WarningFilter>> {
        BTreeMap::from([
            known_code_filter!(FILTER_IDE_PATH_AUTOCOMPLETE, IDE::PathAutocomplete),
            known_code_filter!(FILTER_IDE_DOT_AUTOCOMPLETE, IDE::DotAutocomplete),
        ])
    }
}

impl AstDebug for WarningFilters {
    fn ast_debug(&self, w: &mut crate::shared::ast_debug::AstWriter) {
        self.borrow().ast_debug(w);
    }
}

impl AstDebug for WarningFiltersBuilder {
    fn ast_debug(&self, w: &mut crate::shared::ast_debug::AstWriter) {
        for (prefix, filters) in &self.filters {
            let prefix_str = prefix.unwrap_or(known_attributes::DiagnosticAttribute::ALLOW);
            match filters {
                UnprefixedWarningFilters::All => w.write(format!(
                    "#[{}({})]",
                    prefix_str,
                    WarningFilter::All(*prefix).to_str().unwrap(),
                )),
                UnprefixedWarningFilters::Specified { categories, codes } => {
                    w.write(format!("#[{}(", prefix_str));
                    let items = categories
                        .iter()
                        .map(|(cat, n)| WarningFilter::Category {
                            prefix: *prefix,
                            category: *cat,
                            name: *n,
                        })
                        .chain(codes.iter().map(|((cat, code), n)| WarningFilter::Code {
                            prefix: *prefix,
                            category: *cat,
                            code: *code,
                            name: *n,
                        }));
                    w.list(items, ",", |w, filter| {
                        w.write(filter.to_str().unwrap());
                        false
                    });
                    w.write(")]")
                }
                UnprefixedWarningFilters::Empty => (),
            }
        }
    }
}
