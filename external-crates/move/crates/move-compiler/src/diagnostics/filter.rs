// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Scope-based diagnostic filtering.
//!
//! A [`FilterStack`] holds a stack of [`FilterScope`]s. Each scope carries a sorted list of
//! overrides that match diagnostics by [`DiagnosticsID`] triples (with
//! [`DIAGNOSTIC_FILTER_WILDCARD`] wildcards) and associate a [`FilterKind`].
//!
//! Resolution walks the stack innermost-first; within a scope, the most specific match
//! wins (exact code > category wildcard > prefix wildcard).
//!
//! `#[expect]` fulfillment lives on the scope itself (one `AtomicBool` per expect
//! override), so the same [`FilterScope`] re-pushed across compilation passes shares state.
//! Pops are pure structural pops; [`FilterScope::finalize`] is called at end-of-pipeline on
//! each scope to emit diagnostics for unfulfilled `#[expect(...)]` overrides.
//!
//! ## Allocation strategy
//!
//! Each [`FilterScope`] is a leaked `&'static FilterScopeData` behind a `Copy` handle.
//! The compiler is run-once-and-exit, so the leak is harmless. Unlike the old
//! `WarningFiltersTable` interning approach, scopes are *not* deduplicated: each item gets
//! its own allocation even when filters are identical. This preserves per-item source
//! locations on overrides (needed for `#[deny]` secondary labels and `#[expect]` unfulfilled
//! diagnostics) and avoids the complexity of an interning table.

use std::collections::BTreeMap;
use std::sync::{
    LazyLock,
    atomic::{AtomicBool, Ordering},
};

use move_ir_types::location::*;
use move_symbol_pool::Symbol;

use crate::diagnostics::{
    Diagnostic,
    codes::{Category, Declarations, DiagnosticsID, Severity, TypeSafety, UnusedItem},
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

/// The action a matching override takes on a diagnostic.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum FilterKind {
    /// Suppress the diagnostic but retain it in `filtered_source_diagnostics`.
    Allow,
    /// Upgrade the diagnostic's severity to `NonblockingError`.
    Deny,
    /// Suppress the diagnostic and mark this override as fulfilled. Unfulfilled `Expect`
    /// overrides emit an "unfulfilled expect" diagnostic at [`FilterScope::finalize`] time.
    Expect,
    /// Suppress the diagnostic and discard it entirely. Used by dependency compilation.
    Drop,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Override_ {
    pub filter: DiagnosticsID,
    pub kind: FilterKind,
}

pub type Override = Spanned<Override_>;

/// Fulfillment record for a single `#[expect(...)]` override. Interior-mutable so state
/// persists across passes that push the same [`FilterScope`].
///
/// Uses `AtomicBool` rather than `Cell<bool>` because `FilterScopeData` is stored behind
/// a `&'static` reference (via `Box::leak`), which requires `Sync`.
#[derive(Debug)]
pub struct ExpectState {
    pub loc: Loc,
    pub filter: DiagnosticsID,
    pub fulfilled: AtomicBool,
}

/// An immutable set of overrides plus expect-fulfillment side-car.
#[derive(Debug)]
pub struct FilterScopeData {
    overrides: BTreeMap<DiagnosticsID, Override>,
    expects: Vec<ExpectState>,
}

/// Opaque handle to a filter scope.
// Leaked `&'static` makes this `Copy`. The compiler is run-once-and-exit.
// `PartialEq`/`Eq` use pointer identity: same leaked allocation = same scope.
#[derive(Clone, Copy, Debug)]
pub struct FilterScope(pub(crate) &'static FilterScopeData);

// Scopes are only reflexively equal, since otherwise they may vary by location information.
// This is only relevant for adding them to ASTs which derive Eq.
impl PartialEq for FilterScope {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

impl Eq for FilterScope {}

/// Result of filtering a single diagnostic through the stack.
pub enum FilterResult {
    /// Diagnostic passed through (possibly with adjusted severity or added notes).
    Emit(Diagnostic),
    /// Diagnostic was suppressed by `Allow` (retained for tooling).
    Filtered(Diagnostic),
    /// Diagnostic was suppressed and discarded entirely (`Drop` or `Expect`).
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
    filter: DiagnosticsID,
}

//**************************************************************************************************
// Singletons
//**************************************************************************************************

static EMPTY_FILTER_SCOPE: LazyLock<FilterScope> = LazyLock::new(|| {
    FilterScope(Box::leak(Box::new(FilterScopeData {
        overrides: BTreeMap::new(),
        expects: vec![],
    })))
});

static ALL_FILTER_SCOPE: LazyLock<FilterScope> = LazyLock::new(|| {
    let filter = DiagnosticsID::all(None);
    let o = Override_ {
        filter,
        kind: FilterKind::Allow,
    };
    FilterScope(Box::leak(Box::new(FilterScopeData {
        overrides: BTreeMap::from([(filter, sp(Loc::invalid(), o))]),
        expects: vec![],
    })))
});

static UNUSED_FOR_TEST_FILTER_SCOPE: LazyLock<FilterScope> = LazyLock::new(|| {
    let cat = Category::UnusedItem as u8;
    let codes = [
        UnusedItem::Function,
        UnusedItem::StructField,
        UnusedItem::FunTypeParam,
        UnusedItem::Constant,
        UnusedItem::MutReference,
        UnusedItem::MutParam,
    ];
    let overrides = codes
        .into_iter()
        .map(|c| {
            let filter = DiagnosticsID::exact(None, cat, c as u8);
            let o = Override_ {
                filter,
                kind: FilterKind::Allow,
            };
            (filter, sp(Loc::invalid(), o))
        })
        .collect();
    FilterScope(Box::leak(Box::new(FilterScopeData {
        overrides,
        expects: vec![],
    })))
});

static DEPENDENCY_DROP_FILTER_SCOPE: LazyLock<FilterScope> = LazyLock::new(|| {
    let filter = DiagnosticsID::all(None);
    let o = Override_ {
        filter,
        kind: FilterKind::Drop,
    };
    FilterScope(Box::leak(Box::new(FilterScopeData {
        overrides: BTreeMap::from([(filter, sp(Loc::invalid(), o))]),
        expects: vec![],
    })))
});

/// Scope with no overrides: the common case for any item without lint attributes.
pub fn empty_filter_scope() -> FilterScope {
    *EMPTY_FILTER_SCOPE
}

/// Scope that allows every diagnostic and records them as filtered.
pub fn all_filter_scope() -> FilterScope {
    *ALL_FILTER_SCOPE
}

/// Scope that suppresses unused-item warnings commonly noisy in test contexts.
pub fn unused_for_test_filter_scope() -> FilterScope {
    *UNUSED_FOR_TEST_FILTER_SCOPE
}

/// Scope that drops every diagnostic entirely. Use for dependency compilation.
pub fn dependency_drop_filter_scope() -> FilterScope {
    *DEPENDENCY_DROP_FILTER_SCOPE
}

//**************************************************************************************************
// Known filter registration
//**************************************************************************************************

/// Expansion of a known filter name into the set of [`DiagnosticsID`] triples it covers.
/// Kind is supplied at attribute-resolution time, not stored here.
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
    /// Build a scope from the given overrides. Duplicates (by `DiagnosticsID`) are merged,
    /// keeping the last entry. Empty input returns the shared [`EMPTY_FILTER_SCOPE`] singleton.
    pub fn new(overrides: BTreeMap<DiagnosticsID, Override>) -> Self {
        if overrides.is_empty() {
            return *EMPTY_FILTER_SCOPE;
        }
        let expects = overrides
            .values()
            .filter(|sp!(_, o)| o.kind == FilterKind::Expect)
            .map(|sp!(loc, o)| ExpectState {
                loc: *loc,
                filter: o.filter,
                fulfilled: AtomicBool::new(false),
            })
            .collect();
        FilterScope(Box::leak(Box::new(FilterScopeData { overrides, expects })))
    }

    pub fn overrides(&self) -> &BTreeMap<DiagnosticsID, Override> {
        &self.0.overrides
    }

    pub fn expects(&self) -> &[ExpectState] {
        &self.0.expects
    }

    /// Emit diagnostics for every `#[expect(...)]` override that was never matched.
    /// Consumes the scope since finalization should only be done once.
    /// NB: `Copy` could ostensibly let you do this repeatedly times, but that would be a misuse of
    /// the API and also hopefully deduped by the diagnostic framework.
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
        self.stack.push(scope);
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
        let key = diag.info().id();

        // If this isn't a warning, then filtering doesn't apply: just emit it as-is. This also
        // prevents us from accidentally adding `#[allow(...)]` hints to errors.
        if diag.severity() != Severity::Warning {
            return FilterResult::Emit(diag);
        }

        let Some(resolved) = self.resolve(key) else {
            maybe_add_filter_hint(&mut diag, known_filter_names);
            return FilterResult::Emit(diag);
        };
        match resolved.kind {
            FilterKind::Allow => FilterResult::Filtered(diag),
            FilterKind::Deny => {
                if resolved.loc != Loc::invalid() {
                    diag.add_secondary_label((resolved.loc, "the lint level is defined here"));
                }
                FilterResult::Emit(diag)
            }
            FilterKind::Expect => {
                mark_expect_fulfilled(resolved.scope, resolved.filter);
                FilterResult::Discarded
            }
            FilterKind::Drop => FilterResult::Discarded,
        }
    }

    fn resolve(&self, key: DiagnosticsID) -> Option<Resolved<'_>> {
        let candidates = [
            key,
            DiagnosticsID::category(key.prefix, key.category),
            DiagnosticsID::all(key.prefix),
        ];
        for scope in self.stack.iter().rev() {
            let mut best: Option<(usize, &Override)> = None;
            for (specificity, candidate) in candidates.iter().enumerate() {
                if let Some(entry) = scope.0.overrides.get(candidate)
                    && best.is_none_or(|(s, _)| specificity < s)
                {
                    best = Some((specificity, entry));
                }
            }
            if let Some((_, sp!(loc, o))) = best {
                return Some(Resolved {
                    loc: *loc,
                    scope,
                    kind: o.kind,
                    filter: o.filter,
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

fn mark_expect_fulfilled(scope: &FilterScope, filter: DiagnosticsID) {
    for e in &scope.0.expects {
        if e.filter == filter {
            e.fulfilled.store(true, Ordering::SeqCst);
            return;
        }
    }
}

impl crate::shared::AstDebug for FilterScope {
    fn ast_debug(&self, w: &mut crate::shared::ast_debug::AstWriter) {
        let n = self.0.overrides.len();
        if n == 0 {
            w.write("(no filters)");
        } else {
            w.write(format!("({n} filter overrides)"));
        }
    }
}
