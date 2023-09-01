// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct CommitteeMember {
    pub authority_name: Option<String>,
    pub stake_unit: Option<u64>,
}
