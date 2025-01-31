// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{account_address::AccountAddress, identifier::Identifier, language_storage::ModuleId};
use anyhow::{bail, Result};
use std::collections::btree_map::{self, BTreeMap};

/// A storage operation.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum Op<T> {
    /// Inserts some new data into an empty slot.
    New(T),
    /// Modifies some data that currently exists.
    Modify(T),
    /// Deletes some data that currently exists.
    Delete,
}

impl<T> Op<T> {
    pub fn as_ref(&self) -> Op<&T> {
        use Op::*;

        match self {
            New(data) => New(data),
            Modify(data) => Modify(data),
            Delete => Delete,
        }
    }

    pub fn map<F, U>(self, f: F) -> Op<U>
    where
        F: FnOnce(T) -> U,
    {
        use Op::*;

        match self {
            New(data) => New(f(data)),
            Modify(data) => Modify(f(data)),
            Delete => Delete,
        }
    }

    pub fn ok(self) -> Option<T> {
        use Op::*;

        match self {
            New(data) | Modify(data) => Some(data),
            Delete => None,
        }
    }
}

/// A collection of resource and module operations on a Move account.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct AccountChangeSet {
    runtime_id: AccountAddress,
    modules: BTreeMap<Identifier, Op<Vec<u8>>>,
}

impl AccountChangeSet {
    pub fn from_modules(
        runtime_id: AccountAddress,
        modules: BTreeMap<Identifier, Op<Vec<u8>>>,
    ) -> Self {
        Self {
            runtime_id,
            modules,
        }
    }

    pub fn new(runtime_id: AccountAddress) -> Self {
        Self {
            runtime_id,
            modules: BTreeMap::new(),
        }
    }

    pub fn into_inner(self) -> BTreeMap<Identifier, Op<Vec<u8>>> {
        self.modules
    }

    pub fn into_modules(self) -> BTreeMap<Identifier, Op<Vec<u8>>> {
        self.modules
    }

    pub fn modules(&self) -> &BTreeMap<Identifier, Op<Vec<u8>>> {
        &self.modules
    }

    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    pub fn runtime_id(&self) -> AccountAddress {
        self.runtime_id
    }
}

// TODO: ChangeSet does not have a canonical representation so the derived Ord is not sound.

/// A collection of changes to a Move state. Each AccountChangeSet in the domain of `accounts`
/// is guaranteed to be nonempty
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct ChangeSet {
    accounts: BTreeMap<AccountAddress, AccountChangeSet>,
}

impl Default for ChangeSet {
    fn default() -> Self {
        Self::new()
    }
}

impl ChangeSet {
    pub fn new() -> Self {
        Self {
            accounts: BTreeMap::new(),
        }
    }

    pub fn add_account_changeset(
        &mut self,
        addr: AccountAddress,
        account_changeset: AccountChangeSet,
    ) -> Result<()> {
        match self.accounts.entry(addr) {
            btree_map::Entry::Occupied(_) => bail!(
                "Failed to add account change set. Account {} already exists.",
                addr
            ),
            btree_map::Entry::Vacant(entry) => {
                entry.insert(account_changeset);
            }
        }

        Ok(())
    }

    pub fn accounts(&self) -> &BTreeMap<AccountAddress, AccountChangeSet> {
        &self.accounts
    }

    pub fn into_inner(self) -> BTreeMap<AccountAddress, AccountChangeSet> {
        self.accounts
    }

    pub fn into_modules(self) -> impl Iterator<Item = (ModuleId, Op<Vec<u8>>)> {
        self.accounts.into_iter().flat_map(|(addr, account)| {
            account
                .modules
                .into_iter()
                .map(move |(module_name, blob_opt)| (ModuleId::new(addr, module_name), blob_opt))
        })
    }

    pub fn modules(&self) -> impl Iterator<Item = (AccountAddress, &Identifier, Op<&[u8]>)> {
        self.accounts.iter().flat_map(|(addr, account)| {
            let addr = *addr;
            account
                .modules
                .iter()
                .map(move |(module_name, op)| (addr, module_name, op.as_ref().map(|v| v.as_ref())))
        })
    }
}
