// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod analysis;

use crate::static_programmable_transactions::linkage::analysis::ResolvedLinkage;
use move_core_types::account_address::AccountAddress;

#[derive(Clone, Debug)]
pub struct Linkage {
    pub link_context: AccountAddress,
    pub resolved_linkage: ResolvedLinkage,
}

impl Linkage {
    pub fn new(link_context: AccountAddress, resolved_linkage: ResolvedLinkage) -> Linkage {
        Self {
            link_context,
            resolved_linkage,
        }
    }

    pub fn new_with_default_context(resolved_linkage: ResolvedLinkage) -> Linkage {
        Self {
            link_context: AccountAddress::ZERO,
            resolved_linkage,
        }
    }
}
