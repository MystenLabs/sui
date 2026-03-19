// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diagnostics::{
        Diagnostic, DiagnosticCode,
        codes::{
            Category, ExternalPrefix, INTERNAL_FILTER_REVERSE, Severity, UnusedItem,
            internal_category_filter_range, internal_filter_index,
        },
    },
    linters::{
        LINT_CODE_CATEGORIES, LINT_FILTER_BASE, LINT_FILTER_REVERSE, LinterDiagnosticCategory,
        NUM_LINT_CODES, lint_code_filter_index,
    },
    shared::{AstDebug, CompilationEnv, known_attributes},
    sui_mode::linters::NUM_SUI_LINT_CODES,
};
use move_symbol_pool::Symbol;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

/// Number of u64 words in the internal bitset, derived from the total number of filter IDs.
const INTERNAL_BITSET_WORDS: usize = (LINT_FILTER_BASE + NUM_LINT_CODES + 63) / 64;
/// Total bit capacity of the internal bitset.
pub const INTERNAL_BITSET_CAPACITY: usize = INTERNAL_BITSET_WORDS * u64::BITS as usize;
/// Total bit capacity of the flavor bitset.
pub const FLAVOR_BITSET_CAPACITY: usize = u64::BITS as usize;

const _: () = assert!(
    NUM_SUI_LINT_CODES <= FLAVOR_BITSET_CAPACITY,
    "Sui lint codes exceed flavor bitset capacity"
);

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
pub const FILTER_LITERAL_ENFORCEMENT: &str = "untyped_literal";

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

//**************************************************************************************************
// Types
//**************************************************************************************************

/// None for the default 'allow'.
/// Some(prefix) for a custom set of warnings, e.g. 'allow(lint(_))'.
pub type FilterPrefix = Option<Symbol>;
pub type FilterName = Symbol;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct WarningFiltersScope {
    current: WarningFilters,
    stack: Vec<WarningFilters>,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, PartialOrd, Ord, Hash, Default)]
pub struct WarningFilters {
    /// Bitpacked filter for internal diagnostics AND non-flavor lint codes.
    internal: [u64; INTERNAL_BITSET_WORDS],
    /// Bitpacked filter for flavor-specific diagnostics (e.g., Sui lints).
    flavor: u64,
    /// Whether this filter applies to dependency code.
    for_dependency: bool,
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
    pub(crate) fn root(top_level: Option<WarningFilters>) -> Self {
        Self {
            current: top_level.unwrap_or_default(),
            stack: vec![],
        }
    }

    pub fn push(&mut self, filters: WarningFilters) {
        let mut merged = self.current;
        merged.union(&filters);
        self.stack
            .push(std::mem::replace(&mut self.current, merged));
    }

    pub fn pop(&mut self) {
        self.current = self.stack.pop().expect("pop on empty scope");
    }

    pub fn is_filtered(&self, diag: &Diagnostic) -> bool {
        self.current.is_filtered(diag)
    }

    pub fn is_filtered_for_dependency(&self) -> bool {
        self.current.for_dependency()
    }
}

impl WarningFilters {
    pub const fn new_for_source() -> Self {
        Self {
            internal: [0; INTERNAL_BITSET_WORDS],
            flavor: 0,
            for_dependency: false,
        }
    }

    pub const fn new_for_dependency() -> Self {
        Self {
            internal: [0; INTERNAL_BITSET_WORDS],
            flavor: 0,
            for_dependency: true,
        }
    }

    pub fn new_all_filter_alls(env: &CompilationEnv) -> Self {
        let mut f = Self::new_for_dependency();
        for prefix in env.known_filter_names() {
            for filter in env.filter_from_str(prefix, FILTER_ALL) {
                f.add(filter);
            }
        }
        f
    }

    pub fn is_filtered(&self, diag: &Diagnostic) -> bool {
        let info = diag.info();
        if info.severity() > Severity::Warning {
            return false;
        }
        match info.external_prefix() {
            None => match internal_filter_index(info.category(), info.code()) {
                Some(idx) => test_internal_bit(&self.internal, idx),
                None => false,
            },
            _ => {
                if is_flavor_category(info.category()) {
                    match flavor_filter_index(info.category(), info.code()) {
                        Some(idx) => self.flavor & (1u64 << idx) != 0,
                        None => false,
                    }
                } else {
                    match lint_code_filter_index(info.category(), info.code()) {
                        Some(idx) => test_internal_bit(&self.internal, idx),
                        None => false,
                    }
                }
            }
        }
    }

    pub fn union(&mut self, other: &Self) {
        for i in 0..INTERNAL_BITSET_WORDS {
            self.internal[i] |= other.internal[i];
        }
        self.flavor |= other.flavor;
        // if there is a dependency code filter on the stack, it means we are filtering dependent
        // code and this information must be preserved when stacking up additional filters (which
        // involves union of the current filter with the new one)
        self.for_dependency = self.for_dependency || other.for_dependency;
    }

    pub fn add(&mut self, filter: WarningFilter) {
        match filter {
            WarningFilter::All(prefix) => match prefix {
                None => self.internal = [u64::MAX; INTERNAL_BITSET_WORDS],
                _ => {
                    // Set all non-flavor lint bits in internal
                    for i in 0..NUM_LINT_CODES {
                        set_internal_bit(&mut self.internal, LINT_FILTER_BASE + i);
                    }
                    // Set all flavor bits
                    self.flavor = u64::MAX;
                }
            },
            WarningFilter::Category {
                prefix, category, ..
            } => match prefix {
                None => {
                    let (base, count) =
                        internal_category_filter_range(category).unwrap_or_else(|| {
                            panic!(
                                "ICE: unknown internal diagnostic category {category} \
                                 in warning filter"
                            )
                        });
                    for i in 0..count as usize {
                        set_internal_bit(&mut self.internal, base + i);
                    }
                }
                _ => {
                    if is_flavor_category(category) {
                        let (base, count) = flavor_category_range(category).unwrap_or_else(|| {
                            panic!("ICE: unknown flavor category {category} in warning filter")
                        });
                        for i in 0..count {
                            self.flavor |= 1u64 << (base + i);
                        }
                    } else {
                        // Filter all lint codes in this category
                        for (i, &cat) in LINT_CODE_CATEGORIES.iter().enumerate() {
                            if cat == category {
                                set_internal_bit(&mut self.internal, LINT_FILTER_BASE + i);
                            }
                        }
                    }
                }
            },
            WarningFilter::Code {
                prefix,
                category,
                code,
                ..
            } => match prefix {
                None => {
                    let idx = internal_filter_index(category, code).unwrap_or_else(|| {
                        panic!(
                            "ICE: unknown internal diagnostic ({category}, {code}) \
                             in warning filter"
                        )
                    });
                    set_internal_bit(&mut self.internal, idx);
                }
                _ => {
                    if is_flavor_category(category) {
                        let idx = flavor_filter_index(category, code).unwrap_or_else(|| {
                            panic!(
                                "ICE: unknown flavor diagnostic ({category}, {code}) \
                                 in warning filter"
                            )
                        });
                        self.flavor |= 1u64 << idx;
                    } else {
                        let idx = lint_code_filter_index(category, code).unwrap_or_else(|| {
                            panic!(
                                "ICE: unknown lint diagnostic ({category}, {code}) \
                                 in warning filter"
                            )
                        });
                        set_internal_bit(&mut self.internal, idx);
                    }
                }
            },
        }
    }

    pub fn add_all(&mut self, filters: impl IntoIterator<Item = WarningFilter>) {
        for filter in filters {
            self.add(filter);
        }
    }

    pub fn for_dependency(&self) -> bool {
        self.for_dependency
    }

    pub fn unused_warnings_filter_for_test() -> Self {
        let mut result = Self::new_for_source();
        for item in [
            UnusedItem::Function,
            UnusedItem::StructField,
            UnusedItem::FunTypeParam,
            UnusedItem::Constant,
            UnusedItem::MutReference,
            UnusedItem::MutParam,
        ] {
            let info = item.into_info();
            if let Some(idx) = internal_filter_index(info.category(), info.code()) {
                result.internal[idx / 64] |= 1u64 << (idx % 64);
            }
        }
        result
    }
}

const FLAVOR_CATEGORY: u8 = LinterDiagnosticCategory::Sui as u8;

fn flavor_filter_index(category: u8, code: u8) -> Option<usize> {
    if category == FLAVOR_CATEGORY {
        Some(code as usize)
    } else {
        None
    }
}

fn is_flavor_category(category: u8) -> bool {
    category == FLAVOR_CATEGORY
}

fn flavor_category_range(category: u8) -> Option<(usize, usize)> {
    if category == FLAVOR_CATEGORY {
        Some((0, NUM_SUI_LINT_CODES))
    } else {
        None
    }
}

fn flavor_filter_reverse(idx: usize) -> Option<(u8, u8)> {
    if idx < NUM_SUI_LINT_CODES {
        Some((FLAVOR_CATEGORY, idx as u8))
    } else {
        None
    }
}

fn set_internal_bit(internal: &mut [u64; INTERNAL_BITSET_WORDS], idx: usize) {
    let word = idx / 64;
    let bit = idx % 64;
    if word < INTERNAL_BITSET_WORDS {
        internal[word] |= 1u64 << bit;
    }
}

fn test_internal_bit(internal: &[u64; INTERNAL_BITSET_WORDS], idx: usize) -> bool {
    let word = idx / 64;
    let bit = idx % 64;
    word < INTERNAL_BITSET_WORDS && (internal[word] & (1u64 << bit)) != 0
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
            known_code_filter!(FILTER_LITERAL_ENFORCEMENT, TypeSafety::MissingLiteralType),
        ])
    }

    pub fn ide_known_filters() -> BTreeMap<FilterName, BTreeSet<WarningFilter>> {
        BTreeMap::from([
            known_code_filter!(FILTER_IDE_PATH_AUTOCOMPLETE, IDE::PathAutocomplete),
            known_code_filter!(FILTER_IDE_DOT_AUTOCOMPLETE, IDE::DotAutocomplete),
        ])
    }
}

static FILTER_NAME_LOOKUP: LazyLock<BTreeMap<(ExternalPrefix, u8, u8), &'static str>> =
    LazyLock::new(|| {
        WarningFilter::compiler_known_filters()
            .into_values()
            .chain(WarningFilter::ide_known_filters().into_values())
            .flatten()
            .filter_map(|f| match f {
                WarningFilter::Code {
                    prefix,
                    category,
                    code,
                    name: Some(name),
                } => Some(((prefix, category, code), name)),
                WarningFilter::Category {
                    prefix,
                    category,
                    name: Some(name),
                } => Some(((prefix, category, 0), name)),
                _ => None,
            })
            .collect()
    });

fn iter_set_bits(words: &[u64; INTERNAL_BITSET_WORDS]) -> impl Iterator<Item = usize> + '_ {
    words.iter().enumerate().flat_map(|(word_idx, &word)| {
        (0..64u32).filter_map(move |bit| {
            if word & (1u64 << bit) != 0 {
                Some(word_idx * 64 + bit as usize)
            } else {
                None
            }
        })
    })
}

fn format_filter(prefix: ExternalPrefix, cat: u8, code: u8) -> String {
    match FILTER_NAME_LOOKUP.get(&(prefix, cat, code)) {
        Some(n) => (*n).to_owned(),
        None => format!("({},{})", cat, code),
    }
}

impl AstDebug for WarningFilters {
    fn ast_debug(&self, w: &mut crate::shared::ast_debug::AstWriter) {
        let prefix_str = known_attributes::DiagnosticAttribute::ALLOW;

        // Internal bitset: partition into internal (prefix=None) and lint (prefix=Some) items
        if self.internal == [u64::MAX; INTERNAL_BITSET_WORDS] {
            w.write(format!("#[{}(all)]", prefix_str));
        } else {
            let mut internal_items = vec![];
            let mut lint_items = vec![];
            for idx in iter_set_bits(&self.internal) {
                if idx < LINT_FILTER_BASE {
                    if let Some(&(cat, code)) = INTERNAL_FILTER_REVERSE.get(idx) {
                        internal_items.push(format_filter(None, cat, code));
                    }
                } else {
                    let lint_idx = idx - LINT_FILTER_BASE;
                    if let Some(&(cat, code)) = LINT_FILTER_REVERSE.get(lint_idx) {
                        lint_items.push(format_filter(Some(""), cat, code));
                    }
                }
            }
            if !internal_items.is_empty() {
                w.write(format!("#[{}(", prefix_str));
                w.list(&internal_items, ",", |w, item| {
                    w.write(item);
                    false
                });
                w.write(")]");
            }
            if !lint_items.is_empty() {
                w.write(format!("#[lint("));
                w.list(&lint_items, ",", |w, item| {
                    w.write(item);
                    false
                });
                w.write(")]");
            }
        }

        // Flavor filters
        if self.flavor != 0 {
            let mut items = vec![];
            for bit in 0..64u32 {
                if self.flavor & (1u64 << bit) != 0 {
                    if let Some((cat, code)) = flavor_filter_reverse(bit as usize) {
                        items.push(format_filter(Some(""), cat, code));
                    }
                }
            }
            if !items.is_empty() {
                w.write(format!("#[lint("));
                w.list(&items, ",", |w, item| {
                    w.write(item);
                    false
                });
                w.write(")]");
            }
        }
    }
}
