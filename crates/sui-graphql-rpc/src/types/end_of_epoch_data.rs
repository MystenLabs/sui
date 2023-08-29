// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::committee_member::CommitteeMember;
use async_graphql::*;

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct EndOfEpochData {
    new_committee: Option<Vec<CommitteeMember>>,
    next_protocol_version: Option<u64>,
}
