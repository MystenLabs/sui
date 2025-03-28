// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use async_graphql::registry::MetaField;

/// Configuration for page size limits, specifying a default and max page size for each paginated
/// fields. These values can be customized for specific fields, otherwise falling back to a blanket
/// default.
pub(crate) struct PaginationConfig {
    /// Fallback configuration.
    fallback: PageLimits,

    /// Type and field name-specific overrides.
    overrides: BTreeMap<(&'static str, &'static str), PageLimits>,
}

/// The configuration for a single paginated field.
pub(crate) struct PageLimits {
    pub default: u32,
    pub max: u32,
}

impl PaginationConfig {
    pub(crate) fn new(
        fallback: PageLimits,
        overrides: BTreeMap<(&'static str, &'static str), PageLimits>,
    ) -> Self {
        Self {
            fallback,
            overrides,
        }
    }

    /// Fetch the default and max page size for this type and field.
    pub(crate) fn limits<'l, 't: 'l>(&'l self, type_: &'t str, name: &'t str) -> &'l PageLimits {
        self.overrides.get(&(type_, name)).unwrap_or(&self.fallback)
    }
}

/// Decides whether the field's return type is paginated.
pub(crate) fn is_connection(field: &MetaField) -> bool {
    let type_ = field.ty.as_str();
    type_.ends_with("Connection") || type_.ends_with("Connection!")
}
