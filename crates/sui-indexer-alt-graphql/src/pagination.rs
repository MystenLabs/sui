// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use async_graphql::registry::MetaField;

/// Configuration for page size limits, specifying a max multi-get size, as well as a default and
/// max page size for each paginated fields. Page limits can be customized for specific fields,
/// otherwise falling back to a blanket default.
pub(crate) struct PaginationConfig {
    /// Maximum number of keys that can be fetched in a single multi-get.
    max_multi_get_size: u32,

    /// Fallback page limit configuration.
    fallback: PageLimits,

    /// Type and field name-specific overrides for page limits.
    overrides: BTreeMap<(&'static str, &'static str), PageLimits>,
}

/// The configuration for a single paginated field.
pub(crate) struct PageLimits {
    pub default: u32,
    pub max: u32,
}

impl PaginationConfig {
    pub(crate) fn new(
        max_multi_get_size: u32,
        fallback: PageLimits,
        overrides: BTreeMap<(&'static str, &'static str), PageLimits>,
    ) -> Self {
        Self {
            max_multi_get_size,
            fallback,
            overrides,
        }
    }

    /// Maximum number of keys that can be fetched in a single multi-get.
    pub(crate) fn max_multi_get_size(&self) -> u32 {
        self.max_multi_get_size
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
