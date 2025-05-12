// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::linkage::analysis::ResolvedLinkage;
use move_core_types::account_address::AccountAddress;
use std::rc::Rc;

pub mod analysis;

#[derive(Clone)]
pub struct Linked<T> {
    pub linkage: Linkage,
    pub value: T,
}

#[derive(Clone, Debug)]
pub struct Linkage {
    pub link_context: AccountAddress,
    pub resolved_linkage: Rc<ResolvedLinkage>,
}

impl Linkage {
    pub const DEFAULT_LINK_CTX: AccountAddress = AccountAddress::ZERO;
    pub fn with_default_link_context(resolved_linkage: Rc<ResolvedLinkage>) -> Self {
        Self {
            link_context: Self::DEFAULT_LINK_CTX,
            resolved_linkage,
        }
    }

    pub fn linked<T>(self, value: T) -> Linked<T> {
        Linked {
            linkage: self,
            value,
        }
    }
}
