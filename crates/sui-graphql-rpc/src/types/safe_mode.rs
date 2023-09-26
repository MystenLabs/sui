// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::gas::GasCostSummary;
use async_graphql::*;

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct SafeMode {
    pub enabled: Option<bool>,
    pub gas_summary: Option<GasCostSummary>,
}
