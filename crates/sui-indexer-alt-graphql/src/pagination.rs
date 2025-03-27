// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

/// Configuration for page size limits, specifying a default page size for each paginated fields.
/// These values can be customized for specific fields, otherwise falling back to a blanket
/// default.
pub(crate) struct PaginationConfig {
    /// Fallback default page size.
    default_page_size: u32,

    /// Type and field name-specific overrides.
    overrides: BTreeMap<(&'static str, &'static str), u32>,
}

impl PaginationConfig {
    pub(crate) fn new(
        default_page_size: u32,
        overrides: BTreeMap<(&'static str, &'static str), u32>,
    ) -> Self {
        Self {
            default_page_size,
            overrides,
        }
    }

    /// Fetch the default page size for this type and field.
    pub(crate) fn default(&self, type_: &str, name: &str) -> u32 {
        self.overrides
            .get(&(type_, name))
            .map_or(self.default_page_size, |o| *o)
    }
}
