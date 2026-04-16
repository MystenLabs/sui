// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Backward-compatible re-exports. New code should use [`super::filter`] directly.

use super::filter::{EMPTY_FILTER_SCOPE, FilterScope, UNUSED_FOR_TEST_FILTER_SCOPE};

pub type WarningFiltersBuilder = FilterScope;

impl FilterScope {
    pub fn new_for_source() -> Self {
        *EMPTY_FILTER_SCOPE
    }

    pub fn unused_warnings_filter_for_test() -> Self {
        *UNUSED_FOR_TEST_FILTER_SCOPE
    }
}
