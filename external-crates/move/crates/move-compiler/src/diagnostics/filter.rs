// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Scope-based diagnostic filtering.
//!
//! A [`FilterStack`] holds a stack of [`FilterScope`]s. Each scope carries a set of filter entries
//! keyed by [`FilterTarget`] (which encodes specificity) and associated with a [`FilterKind`].
//!
//! Resolution walks the stack innermost-first; within a scope, the most specific match wins
//! (exact diagnostic > category > prefix > all-for-dependency).
//!
//! `#[expect]` fulfillment lives on the scope itself (one `AtomicBool` per expect entry),
//! so the same [`FilterScope`] re-pushed across compilation passes shares state.
//!
//! ## Allocation strategy
//!
//! Each [`FilterScope`] wraps an `Arc<FilterScopeData>`. Singletons (empty, all, test,
//! dependency-drop) are shared via `LazyLock`; per-item scopes are allocated individually.
//! Scopes are *not* deduplicated: each item gets its own `Arc` even when filters are
//! identical. This preserves per-item source locations on filter entries (needed for
//! `#[deny]` secondary labels and `#[expect]` unfulfilled diagnostics) and avoids the
//! complexity of an interning table.

use std::collections::BTreeMap;
use std::sync::{
    Arc, LazyLock,
    atomic::{AtomicBool, Ordering},
};

use move_ir_types::location::*;
use move_symbol_pool::Symbol;

use crate::diagnostics::{
    Diagnostic,
    codes::{
        Category, Declarations, DiagnosticsID, ExternalPrefix, Severity, TypeSafety, UnusedItem,
    },
};
use crate::shared::{format_allow_attr, known_attributes};

/// None for the default `allow`, `Some(prefix)` for a custom attribute set e.g. `lint`.
pub type FilterPrefix = Option<Symbol>;
pub type FilterName = Symbol;

//**************************************************************************************************
// Filter name constants
//**************************************************************************************************

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

//**************************************************************************************************
// Types
//**************************************************************************************************

/// The action a matching filter entries takes on a diagnostic.
/// These are ordered by increasing severity for conflict resolution: when multiple entries match
/// the same diagnostic at the same scope, the one further on this list wins.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum FilterKind {
    /// Suppress the diagnostic but retain it in `filtered_source_diagnostics`.
    Allow,
    /// Emit the diagnostic as a warning (the default level). Overrides `Allow`/`Deny`/`Expect`
    /// from an outer scope, restoring normal warning behavior.
    Warn,
    /// Suppress the diagnostic and mark this filter entry as fulfilled. Unfulfilled `Expect`
    /// entries emit an "unfulfilled expect" diagnostic at [`FilterScope::finalize`] time.
    Expect,
    /// Upgrade the diagnostic's severity to `NonblockingError`.
    Deny,
    /// Suppress the diagnostic and discard it entirely. Used by dependency compilation.
    Drop,
}

impl FilterKind {
    /// When two different filter kinds target the same diagnostic at the same scope, pick the
    /// stricter one. Variant ordering defines strictness: Allow < Warn < Expect < Deny < Drop.
    pub fn resolve_conflict(self, other: Self) -> Self {
        std::cmp::max(self, other)
    }
}

impl std::fmt::Display for FilterKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterKind::Allow => write!(f, "allow"),
            FilterKind::Warn => write!(f, "warn"),
            FilterKind::Deny => write!(f, "deny"),
            FilterKind::Expect => write!(f, "expect"),
            FilterKind::Drop => write!(f, "drop"),
        }
    }
}

/// What a filter entry matches against. Variants are ordered by decreasing specificity;
/// during resolution the most specific match wins within a scope.
///
/// This type is internal to the filter module. External code uses [`DiagnosticsID`] (with
/// wildcard sentinels); conversion happens via `From<DiagnosticsID>` at the filter boundary.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum FilterTarget {
    /// Matches a single diagnostic by its exact code ID.
    Diagnostic(DiagnosticsID),
    /// Matches all diagnostics in a category, scoped to a prefix.
    Category(ExternalPrefix, u8),
    /// Matches all diagnostics with the given prefix. `None` matches unprefixed diagnostics
    /// only — this is what user-facing `#[allow(all)]` maps to.
    Prefix(ExternalPrefix),
    /// Matches every diagnostic regardless of prefix. Used for dependency compilation.
    AllForDependency,
}

impl From<DiagnosticsID> for FilterTarget {
    fn from(id: DiagnosticsID) -> Self {
        use crate::diagnostics::codes::DIAGNOSTIC_FILTER_WILDCARD;
        match (id.category, id.code) {
            (DIAGNOSTIC_FILTER_WILDCARD, _) => FilterTarget::Prefix(id.prefix),
            (_, DIAGNOSTIC_FILTER_WILDCARD) => FilterTarget::Category(id.prefix, id.category),
            _ => FilterTarget::Diagnostic(id),
        }
    }
}

/// Fulfillment record for a single `#[expect(...)]` entry.
#[derive(Debug)]
struct ExpectState {
    loc: Loc,
    target: FilterTarget,
    fulfilled: AtomicBool,
}

/// An immutable set of entries plus expect-fulfillment side-car.
#[derive(Debug)]
pub(crate) struct FilterScopeData {
    filter_entries: BTreeMap<FilterTarget, Spanned<FilterKind>>,
    expects: Vec<ExpectState>,
}

/// Opaque handle to a filter scope.
#[derive(Clone, Debug)]
pub struct FilterScope(Arc<FilterScopeData>);

// Scopes are only reflexively equal, since otherwise they may vary by location information.
// This is only relevant for adding them to ASTs which derive Eq.
impl PartialEq for FilterScope {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for FilterScope {}

/// Result of filtering a single diagnostic through the stack.
pub enum FilterResult {
    /// Diagnostic passed through (possibly with adjusted severity or added notes).
    Emit(Diagnostic),
    /// Diagnostic was suppressed by `Allow` or `Expect` (retained for tooling).
    Filtered(Diagnostic),
    /// Diagnostic was suppressed and discarded entirely (`Drop`).
    Discarded,
}

/// A stack of [`FilterScope`]s. Push on item entry, pop on item exit.
#[derive(Clone, Debug, Default)]
pub struct FilterStack {
    stack: Vec<FilterScope>,
}

struct Resolved<'a> {
    loc: Loc,
    scope: &'a FilterScope,
    kind: FilterKind,
    target: FilterTarget,
}

//**************************************************************************************************
// Singletons
//**************************************************************************************************

static EMPTY_FILTER_SCOPE: LazyLock<FilterScope> = LazyLock::new(|| {
    FilterScope(Arc::new(FilterScopeData {
        filter_entries: BTreeMap::new(),
        expects: vec![],
    }))
});

static ALL_FILTER_SCOPE: LazyLock<FilterScope> = LazyLock::new(|| {
    let target = FilterTarget::Prefix(None);
    FilterScope(Arc::new(FilterScopeData {
        filter_entries: BTreeMap::from([(target, sp(Loc::invalid(), FilterKind::Allow))]),
        expects: vec![],
    }))
});

const UNUSED_ITEM_CATEGORY: u8 = Category::UnusedItem as u8;
const UNUSED_ITEM_CODES: [u8; 6] = [
    UnusedItem::Function as u8,
    UnusedItem::StructField as u8,
    UnusedItem::FunTypeParam as u8,
    UnusedItem::Constant as u8,
    UnusedItem::MutReference as u8,
    UnusedItem::MutParam as u8,
];

static UNUSED_FOR_TEST_FILTER_SCOPE: LazyLock<FilterScope> = LazyLock::new(|| {
    let filter_entries = UNUSED_ITEM_CODES
        .into_iter()
        .map(|c| {
            let target =
                FilterTarget::Diagnostic(DiagnosticsID::exact(None, UNUSED_ITEM_CATEGORY, c));
            (target, sp(Loc::invalid(), FilterKind::Allow))
        })
        .collect();
    FilterScope(Arc::new(FilterScopeData {
        filter_entries,
        expects: vec![],
    }))
});

static DEPENDENCY_DROP_FILTER_SCOPE: LazyLock<FilterScope> = LazyLock::new(|| {
    let target = FilterTarget::AllForDependency;
    FilterScope(Arc::new(FilterScopeData {
        filter_entries: BTreeMap::from([(target, sp(Loc::invalid(), FilterKind::Drop))]),
        expects: vec![],
    }))
});

/// Scope with no filter entries: the common case for any item without lint attributes.
pub fn empty_filter_scope() -> FilterScope {
    EMPTY_FILTER_SCOPE.clone()
}

/// Scope that allows all unprefixed diagnostics (used for `--silence-warnings`).
pub fn all_filter_scope() -> FilterScope {
    ALL_FILTER_SCOPE.clone()
}

/// Scope that suppresses unused-item warnings commonly noisy in test contexts.
pub fn unused_for_test_filter_scope() -> FilterScope {
    UNUSED_FOR_TEST_FILTER_SCOPE.clone()
}

/// Scope that drops every diagnostic entirely. Used for dependency compilation.
pub fn dependency_drop_filter_scope() -> FilterScope {
    DEPENDENCY_DROP_FILTER_SCOPE.clone()
}

//**************************************************************************************************
// Known filter registration
//**************************************************************************************************

/// Expansion of a known filter name into the set of [`DiagnosticsID`] triples it covers.
/// Wildcard sentinels in the IDs are converted to [`FilterTarget`] variants at the filter
/// boundary. Kind is supplied at attribute-resolution time, not stored here.
pub type KnownFilterExpansion = &'static [DiagnosticsID];

/// Built-in filter names recognized by `#[allow(...)]` etc. in source code.
pub static COMPILER_KNOWN_FILTERS: LazyLock<Vec<(&'static str, KnownFilterExpansion)>> =
    LazyLock::new(|| {
        let cat_unused = Category::UnusedItem as u8;
        macro_rules! code {
            ($cat:ident :: $code:ident) => {
                DiagnosticsID::exact(None, Category::$cat as u8, $cat::$code as u8)
            };
        }
        // Deliberate leak: these slices live for the process lifetime (behind LazyLock)
        // and are referenced by every FilterScope. The total size is small and bounded.
        fn leak(v: Vec<DiagnosticsID>) -> KnownFilterExpansion {
            Box::leak(v.into_boxed_slice())
        }
        vec![
            (FILTER_ALL, leak(vec![DiagnosticsID::all(None)])),
            (
                FILTER_UNUSED,
                leak(vec![DiagnosticsID::category(None, cat_unused)]),
            ),
            (
                FILTER_MISSING_PHANTOM,
                leak(vec![code!(Declarations::InvalidNonPhantomUse)]),
            ),
            (FILTER_UNUSED_USE, leak(vec![code!(UnusedItem::Alias)])),
            (
                FILTER_UNUSED_VARIABLE,
                leak(vec![code!(UnusedItem::Variable)]),
            ),
            (
                FILTER_UNUSED_ASSIGNMENT,
                leak(vec![code!(UnusedItem::Assignment)]),
            ),
            (
                FILTER_UNUSED_TRAILING_SEMI,
                leak(vec![code!(UnusedItem::TrailingSemi)]),
            ),
            (
                FILTER_UNUSED_ATTRIBUTE,
                leak(vec![code!(UnusedItem::Attribute)]),
            ),
            (
                FILTER_UNUSED_FUNCTION,
                leak(vec![code!(UnusedItem::Function)]),
            ),
            (
                FILTER_UNUSED_STRUCT_FIELD,
                leak(vec![code!(UnusedItem::StructField)]),
            ),
            (
                FILTER_UNUSED_TYPE_PARAMETER,
                leak(vec![
                    DiagnosticsID::exact(None, cat_unused, UnusedItem::StructTypeParam as u8),
                    DiagnosticsID::exact(None, cat_unused, UnusedItem::FunTypeParam as u8),
                ]),
            ),
            (FILTER_UNUSED_CONST, leak(vec![code!(UnusedItem::Constant)])),
            (FILTER_DEAD_CODE, leak(vec![code!(UnusedItem::DeadCode)])),
            (
                FILTER_UNUSED_LET_MUT,
                leak(vec![code!(UnusedItem::MutModifier)]),
            ),
            (
                FILTER_UNUSED_MUT_REF,
                leak(vec![code!(UnusedItem::MutReference)]),
            ),
            (
                FILTER_UNUSED_MUT_PARAM,
                leak(vec![code!(UnusedItem::MutParam)]),
            ),
            (
                FILTER_IMPLICIT_CONST_COPY,
                leak(vec![code!(TypeSafety::ImplicitConstantCopy)]),
            ),
            (
                FILTER_DUPLICATE_ALIAS,
                leak(vec![code!(Declarations::DuplicateAlias)]),
            ),
            (
                FILTER_DEPRECATED,
                leak(vec![code!(TypeSafety::DeprecatedUsage)]),
            ),
            (
                FILTER_LITERAL_ENFORCEMENT,
                leak(vec![code!(TypeSafety::MissingLiteralType)]),
            ),
        ]
    });

pub static IDE_KNOWN_FILTERS: LazyLock<Vec<(&'static str, KnownFilterExpansion)>> =
    LazyLock::new(|| {
        use crate::diagnostics::codes::IDE;
        fn leak(v: Vec<DiagnosticsID>) -> KnownFilterExpansion {
            Box::leak(v.into_boxed_slice())
        }
        let cat_ide = Category::IDE as u8;
        vec![
            (
                FILTER_IDE_PATH_AUTOCOMPLETE,
                leak(vec![DiagnosticsID::exact(
                    None,
                    cat_ide,
                    IDE::PathAutocomplete as u8,
                )]),
            ),
            (
                FILTER_IDE_DOT_AUTOCOMPLETE,
                leak(vec![DiagnosticsID::exact(
                    None,
                    cat_ide,
                    IDE::DotAutocomplete as u8,
                )]),
            ),
        ]
    });

//**************************************************************************************************
// Impls
//**************************************************************************************************

impl FilterScope {
    /// Build a scope from the given filter entries. [`DiagnosticsID`] keys (with wildcard
    /// sentinels) are translated into internal [`FilterTarget`] variants. Empty input returns the
    /// shared
    /// [`EMPTY_FILTER_SCOPE`] singleton.
    pub fn new(input: BTreeMap<DiagnosticsID, Spanned<FilterKind>>) -> Self {
        if input.is_empty() {
            return EMPTY_FILTER_SCOPE.clone();
        }
        let filter_entries: BTreeMap<FilterTarget, Spanned<FilterKind>> = input
            .into_iter()
            .map(|(id, sp!(loc, kind))| (FilterTarget::from(id), sp(loc, kind)))
            .collect();
        let expects = filter_entries
            .iter()
            .filter(|(_, sp!(_, kind))| *kind == FilterKind::Expect)
            .map(|(target, sp!(loc, _))| ExpectState {
                loc: *loc,
                target: *target,
                fulfilled: AtomicBool::new(false),
            })
            .collect();
        FilterScope(Arc::new(FilterScopeData {
            filter_entries,
            expects,
        }))
    }

    /// Iterate over the scope's filter entries as the external format, with loc information.
    pub fn filter_entries(
        &self,
    ) -> impl Iterator<Item = (DiagnosticsID, Spanned<FilterKind>)> + '_ {
        self.0.filter_entries.iter().map(|(target, kind)| {
            let id = match target {
                FilterTarget::Diagnostic(id) => *id,
                FilterTarget::Category(prefix, cat) => DiagnosticsID::category(*prefix, *cat),
                FilterTarget::Prefix(prefix) => DiagnosticsID::all(*prefix),
                FilterTarget::AllForDependency => DiagnosticsID::all(None),
            };
            (id, *kind)
        })
    }

    /// Emit diagnostics for every `#[expect(...)]` entry that was never matched. TODO: decide if
    /// we want to finalize beyond `to_bytecode` in case compilation fails earlier.
    pub fn finalize(self) -> Vec<Diagnostic> {
        self.0
            .expects
            .iter()
            .filter(|e| !e.fulfilled.load(Ordering::SeqCst))
            .map(|e| {
                let mut d = crate::diag!(
                    Attributes::UnfulfilledExpectation,
                    (e.loc, "Expected this warning to be emitted, but it was not")
                );
                d.add_note(
                    "The '#[expect(...)]' attribute will only suppress a warning that \
                     actually occurs. Remove the attribute if the warning is intentionally fixed.",
                );
                d
            })
            .collect()
    }
}

impl FilterStack {
    pub fn new() -> Self {
        Self { stack: vec![] }
    }

    pub fn root(scope: FilterScope) -> Self {
        Self { stack: vec![scope] }
    }

    pub fn push(&mut self, scope: FilterScope) {
        if self.stack.iter().any(|s| {
            s.0.filter_entries
                .contains_key(&FilterTarget::AllForDependency)
        }) {
            // If the stack already contains a dependency-drop scope, push empty to avoid
            // spurious expect-fulfillment tracking in dependency code.
            self.stack.push(EMPTY_FILTER_SCOPE.clone());
        } else {
            self.stack.push(scope);
        }
    }

    pub fn pop(&mut self) {
        debug_assert!(self.stack.pop().is_some(), "ICE: popped empty filter stack");
    }

    /// Resolve a diagnostic against the active scope stack.
    ///
    /// Warnings that pass through get an `#[allow(...)]` hint note when a known filter
    /// name exists. `Deny` upgrades severity to `NonblockingError`.
    pub fn filter(
        &self,
        mut diag: Diagnostic,
        known_filter_names: &BTreeMap<DiagnosticsID, (FilterPrefix, FilterName)>,
    ) -> FilterResult {
        if diag.severity() > Severity::Warning {
            return FilterResult::Emit(diag);
        }

        let Some(resolved) = self.resolve(diag.info().id()) else {
            maybe_add_filter_hint(&mut diag, known_filter_names);
            return FilterResult::Emit(diag);
        };

        match resolved.kind {
            FilterKind::Drop => FilterResult::Discarded,
            FilterKind::Allow => FilterResult::Filtered(diag),
            FilterKind::Warn => {
                diag = diag.set_severity(Severity::Warning);
                if resolved.loc != Loc::invalid() {
                    diag.add_secondary_label((resolved.loc, "the lint level is defined here"));
                }
                FilterResult::Emit(diag)
            }
            FilterKind::Deny => {
                diag = diag.set_severity(Severity::NonblockingError);
                if resolved.loc != Loc::invalid() {
                    diag.add_secondary_label((resolved.loc, "the lint level is defined here"));
                }
                FilterResult::Emit(diag)
            }
            FilterKind::Expect => {
                mark_expect_fulfilled(resolved.scope, resolved.target);
                FilterResult::Filtered(diag)
            }
        }
    }

    fn resolve(&self, key: DiagnosticsID) -> Option<Resolved<'_>> {
        let candidates = [
            FilterTarget::Diagnostic(key),
            FilterTarget::Category(key.prefix, key.category),
            FilterTarget::Prefix(key.prefix),
            FilterTarget::AllForDependency,
        ];
        for scope in self.stack.iter().rev() {
            let mut best: Option<(usize, FilterTarget, &Spanned<FilterKind>)> = None;
            for (specificity, candidate) in candidates.iter().enumerate() {
                if let Some(entry) = scope.0.filter_entries.get(candidate)
                    && best.is_none_or(|(s, ..)| specificity < s)
                {
                    best = Some((specificity, *candidate, entry));
                }
            }
            if let Some((_, target, sp!(loc, kind))) = best {
                return Some(Resolved {
                    loc: *loc,
                    scope,
                    kind: *kind,
                    target,
                });
            }
        }
        None
    }
}

fn maybe_add_filter_hint(
    diag: &mut Diagnostic,
    known_filter_names: &BTreeMap<DiagnosticsID, (FilterPrefix, FilterName)>,
) {
    if diag.info().severity() != Severity::Warning {
        return;
    }
    if let Some((prefix, name)) = known_filter_names.get(&diag.info().id()) {
        let help = format!(
            "This warning can be suppressed with '#[{}({})]' \
             applied to the 'module' or module member ('const', 'fun', or 'struct')",
            known_attributes::DiagnosticAttribute::ALLOW,
            format_allow_attr(*prefix, *name),
        );
        diag.add_note(help);
    }
}

// NB: We never allow wildcard expects, so equality by target is sufficient to find the matching
// expect to mark fulfilled.
fn mark_expect_fulfilled(scope: &FilterScope, target: FilterTarget) {
    for e in &scope.0.expects {
        if e.target == target {
            e.fulfilled.store(true, Ordering::SeqCst);
            return;
        }
    }
}

impl crate::shared::AstDebug for FilterScope {
    fn ast_debug(&self, w: &mut crate::shared::ast_debug::AstWriter) {
        let n = self.0.filter_entries.len();
        if n == 0 {
            w.write("(no filters)");
        } else {
            w.write(format!("({n} filter entries)"));
        }
    }
}
