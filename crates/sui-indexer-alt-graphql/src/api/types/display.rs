// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::SimpleObject;

use crate::api::scalars::json::Json;

/// A rendered JSON blob based on an on-chain template.
#[derive(SimpleObject)]
pub(crate) struct Display {
    /// Output for all successfully substituted display fields. Unsuccessful fields will be `null`, and will be accompanied by a field in `errors`, explaining the error.
    pub(crate) output: Option<Json>,

    /// If any fields failed to render, this will contain a mapping from failed field names to error messages. If all fields succeed, this will be `null`.
    pub(crate) errors: Option<Json>,
}
