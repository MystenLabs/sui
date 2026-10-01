// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
    command_line::compiler::{Visitor, VisitorConstructor},
    diagnostics::{
        codes::DiagnosticsID,
        filter::{
            FILTER_ALL, FILTER_UNUSED_CONST, FILTER_UNUSED_FUNCTION,
            FILTER_UNUSED_FUNCTION_TYPE_PARAMETER, FILTER_UNUSED_MUT_PARAM, FILTER_UNUSED_MUT_REF,
            FILTER_UNUSED_STRUCT_FIELD, FilterKind, FilterName, FilterPrefix, FilterScope,
            resolve_filter_names,
        },
    },
    editions::Flavor,
    linters::{self, LintLevel},
    shared::known_attributes::DiagnosticAttribute,
    sui_mode,
};

pub type KnownDiagnosticFilterGroup = (FilterPrefix, Vec<(FilterName, Vec<DiagnosticsID>)>);
pub type KnownDiagnosticFilters = Vec<KnownDiagnosticFilterGroup>;

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
    /// Allows unused functions, fields, function type parameters, constants, mutable references, and
    /// mutable parameters in test fixtures.
    pub fn for_testing() -> Self {
        let warnings = [
            FILTER_UNUSED_FUNCTION,
            FILTER_UNUSED_STRUCT_FIELD,
            FILTER_UNUSED_FUNCTION_TYPE_PARAMETER,
            FILTER_UNUSED_CONST,
            FILTER_UNUSED_MUT_REF,
            FILTER_UNUSED_MUT_PARAM,
        ]
        .into_iter()
        .map(|name| (FilterName::from(name), FilterKind::Allow))
        .collect();
        Self {
            warnings: Some(warnings),
            lints: None,
        }
    }

    /// Builds the scope for settings validated against the supplied registry.
    pub(crate) fn filter_scope(&self, known: &KnownDiagnosticFilters) -> FilterScope {
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
        resolve_filter_names(configured, known.iter())
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
