// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
    command_line::compiler::{Visitor, VisitorConstructor},
    diagnostics::{
        codes::DiagnosticsID,
        filter::{
            FILTER_ALL, FilterKind, FilterName, FilterPrefix, FilterScope, resolve_filter_names,
        },
    },
    editions::Flavor,
    linters::{self, LintLevel},
    shared::known_attributes::DiagnosticAttribute,
    sui_mode,
};

pub type KnownDiagnosticFilters = Vec<(FilterPrefix, Vec<(FilterName, Vec<DiagnosticsID>)>)>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagnosticFilterSettings {
    pub warnings: Option<DiagnosticFilterConfig>,
    pub lints: Option<DiagnosticFilterConfig>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagnosticFilterConfig {
    all: Option<FilterKind>,
    filters: BTreeMap<FilterName, FilterKind>,
}

pub fn known_diagnostic_filters(flavor: Flavor) -> KnownDiagnosticFilters {
    let mut known = vec![linters::known_filters()];
    match flavor {
        Flavor::Core => (),
        Flavor::Sui => known.push(sui_mode::linters::known_filters()),
    }
    known
}

impl DiagnosticFilterSettings {
    /// Builds the scope for settings validated against the supplied registry.
    pub fn filter_scope(&self, known: &KnownDiagnosticFilters) -> FilterScope {
        let configured = [
            (None, &self.warnings),
            (Some(DiagnosticAttribute::LINT_SYMBOL), &self.lints),
        ]
        .into_iter()
        .flat_map(|(prefix, config)| {
            config
                .iter()
                .flat_map(|config| config.iter())
                .map(move |(name, kind)| (prefix, name, kind))
        });
        resolve_filter_names(configured, known.iter().cloned())
            .expect("diagnostic settings must be validated against the supplied registry")
    }
}

impl DiagnosticFilterConfig {
    pub fn is_empty(&self) -> bool {
        self.all.is_none() && self.filters.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (FilterName, FilterKind)> + '_ {
        self.all
            .map(|kind| (FilterName::from(FILTER_ALL), kind))
            .into_iter()
            .chain(self.filters.iter().map(|(name, kind)| (*name, *kind)))
    }

    /// Checks the named setting, then `all`, then the supplied default.
    pub fn enabled(&self, name: &str, default_enabled: bool) -> bool {
        match self
            .filters
            .get(&FilterName::from(name))
            .copied()
            .or(self.all)
        {
            Some(FilterKind::Allow | FilterKind::Drop) => false,
            Some(FilterKind::Warn | FilterKind::Expect | FilterKind::Deny) => true,
            None => default_enabled,
        }
    }

    pub fn select_lints(
        &self,
        level: LintLevel,
        default_lints: &[(&str, VisitorConstructor)],
        optional_lints: &[(&str, VisitorConstructor)],
    ) -> Vec<Visitor> {
        let optional_enabled = match level {
            LintLevel::None => return vec![],
            LintLevel::Default => false,
            LintLevel::All => true,
        };
        default_lints
            .iter()
            .map(|lint| (true, lint))
            .chain(optional_lints.iter().map(|lint| (optional_enabled, lint)))
            .filter(|(default_enabled, (name, _))| self.enabled(name, *default_enabled))
            .map(|(_, (_, constructor))| constructor())
            .collect()
    }
}

impl FromIterator<(FilterName, FilterKind)> for DiagnosticFilterConfig {
    fn from_iter<T: IntoIterator<Item = (FilterName, FilterKind)>>(entries: T) -> Self {
        let mut config = Self::default();
        for (name, kind) in entries {
            if name.as_str() == FILTER_ALL {
                config.all = Some(kind);
            } else {
                config.filters.insert(name, kind);
            }
        }
        config
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::typing::visitor::TypingVisitor;

    #[test]
    fn enabled_uses_name_then_all_then_default() {
        let name = FilterName::from("abort_without_constant");
        let all = FilterName::from(FILTER_ALL);
        let empty = DiagnosticFilterConfig::default();
        assert!(!empty.enabled(name.as_str(), false));
        assert!(empty.enabled(name.as_str(), true));
        for kind in [FilterKind::Allow, FilterKind::Warn, FilterKind::Deny] {
            let config = DiagnosticFilterConfig::from_iter([(all, kind)]);
            for fallback in [false, true] {
                assert_eq!(
                    config.enabled(name.as_str(), fallback),
                    kind != FilterKind::Allow
                );
            }
            for specific in [FilterKind::Allow, FilterKind::Warn, FilterKind::Deny] {
                let config = DiagnosticFilterConfig::from_iter([(all, kind), (name, specific)]);
                for fallback in [false, true] {
                    assert_eq!(
                        config.enabled(name.as_str(), fallback),
                        specific != FilterKind::Allow
                    );
                }
            }
        }
    }

    #[test]
    fn selection_only_constructs_enabled_visitors() {
        static DEFAULT_CALLS: AtomicUsize = AtomicUsize::new(0);
        static OPTIONAL_CALLS: AtomicUsize = AtomicUsize::new(0);
        let defaults: &[(&str, VisitorConstructor)] = &[("default", || {
            DEFAULT_CALLS.fetch_add(1, Ordering::SeqCst);
            linters::constant_naming::ConstantNaming.visitor()
        })];
        let optional: &[(&str, VisitorConstructor)] = &[("optional", || {
            OPTIONAL_CALLS.fetch_add(1, Ordering::SeqCst);
            linters::constant_naming::ConstantNaming.visitor()
        })];
        let config = DiagnosticFilterConfig::from_iter([
            (FilterName::from(FILTER_ALL), FilterKind::Allow),
            (FilterName::from("optional"), FilterKind::Deny),
        ]);
        for level in [LintLevel::Default, LintLevel::All] {
            assert_eq!(config.select_lints(level, defaults, optional).len(), 1);
        }
        assert!(
            config
                .select_lints(LintLevel::None, defaults, optional)
                .is_empty()
        );
        assert_eq!(DEFAULT_CALLS.load(Ordering::SeqCst), 0);
        assert_eq!(OPTIONAL_CALLS.load(Ordering::SeqCst), 2);

        let empty = DiagnosticFilterConfig::default();
        assert_eq!(
            empty
                .select_lints(LintLevel::Default, defaults, optional)
                .len(),
            1
        );
        assert_eq!(
            empty.select_lints(LintLevel::All, defaults, optional).len(),
            2
        );
        assert_eq!(DEFAULT_CALLS.load(Ordering::SeqCst), 2);
        assert_eq!(OPTIONAL_CALLS.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn iteration_preserves_settings_without_duplicating_all() {
        let entries = [
            (FilterName::from(FILTER_ALL), FilterKind::Allow),
            (FilterName::from("abort_without_constant"), FilterKind::Deny),
        ];
        let config = DiagnosticFilterConfig::from_iter(entries);
        assert_eq!(config.iter().collect::<Vec<_>>(), entries);
        assert!(!config.is_empty());
        assert!(DiagnosticFilterConfig::default().is_empty());
    }
}
