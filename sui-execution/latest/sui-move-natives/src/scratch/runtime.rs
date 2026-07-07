// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use better_any::{Tid, TidAble};
use move_core_types::account_address::AccountAddress;
use move_vm_runtime::execution::{Type, values::Value};
use move_vm_runtime::natives::extensions::NativeExtensionMarker;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

/// A single scratch entry: the runtime type of the stored value alongside the value itself. The
/// type is retained so reads and removes can verify the caller's requested type matches what was
/// stored.
pub struct ScratchEntry {
    pub ty: Type,
    pub value: Value,
}

/// Per-transaction, in-memory scratch store. Entries are keyed by the address derived from the
/// `(key type, key value)` pair and live only for the duration of the transaction. The key type
/// and value are not stored directly, only the derived address.
/// A fresh
/// `ScratchRuntime` is installed per transaction, and the map is dropped at the end of it.
#[derive(Tid, Default)]
pub struct ScratchRuntime {
    entries: BTreeMap<AccountAddress, ScratchEntry>,
}

impl NativeExtensionMarker<'_> for ScratchRuntime {}

impl ScratchRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    /// Removes all entries. Used to reset the store at a transaction boundary in `test_scenario`,
    /// where a single set of native extensions is reused across simulated transactions.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Inserts a new entry.
    /// Returns `Ok(())` if the entry was not already present and was successfully inserted
    /// Returns or `Err((ty, value))` if an entry already existed for `key`
    pub fn add(
        &mut self,
        key: AccountAddress,
        ty: Type,
        value: Value,
    ) -> Result<(), (Type, Value)> {
        match self.entries.entry(key) {
            Entry::Vacant(v) => {
                v.insert(ScratchEntry { ty, value });
                Ok(())
            }
            Entry::Occupied(_) => Err((ty, value)),
        }
    }

    pub fn get(&self, key: &AccountAddress) -> Option<&ScratchEntry> {
        self.entries.get(key)
    }

    pub fn remove(&mut self, key: &AccountAddress) -> Option<ScratchEntry> {
        self.entries.remove(key)
    }

    /// Returns true if an entry exists for `key`, regardless of its stored type.
    pub fn contains(&self, key: &AccountAddress) -> bool {
        self.entries.contains_key(key)
    }
}
