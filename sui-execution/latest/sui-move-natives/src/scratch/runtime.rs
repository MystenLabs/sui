// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use better_any::{Tid, TidAble};
use move_core_types::account_address::AccountAddress;
use move_vm_runtime::execution::{Type, values::Value};
use move_vm_runtime::natives::extensions::NativeExtensionMarker;
use std::collections::BTreeMap;
use sui_protocol_config::ProtocolConfig;

/// A single scratch entry: the runtime type of the stored value alongside the value itself. The
/// type is retained so reads and removes can verify the caller's requested type matches what was
/// stored.
pub struct ScratchEntry {
    pub ty: Type,
    pub value: Value,
}

/// Per-transaction, in-memory scratch store. Entries are keyed by the address derived from the
/// `(key type, key value)` pair and live only for the duration of the transaction: a fresh
/// `ScratchRuntime` is installed per transaction, and the map is dropped at the end of it.
#[derive(Tid)]
pub struct ScratchRuntime<'a> {
    protocol_config: &'a ProtocolConfig,
    entries: BTreeMap<AccountAddress, ScratchEntry>,
}

/// The outcome of an `add`.
pub enum AddResult {
    /// The entry was inserted.
    Inserted,
    /// An entry already existed for the key, so nothing was inserted.
    Duplicate,
    /// The store is already at `max_scratch_pad_size` entries, so nothing was inserted.
    LimitExceeded,
}

impl<'a> NativeExtensionMarker<'a> for ScratchRuntime<'a> {}

impl<'a> ScratchRuntime<'a> {
    pub fn new(protocol_config: &'a ProtocolConfig) -> Self {
        Self {
            protocol_config,
            entries: BTreeMap::new(),
        }
    }

    /// Removes all entries. Used to reset the store at a transaction boundary in `test_scenario`,
    /// where a single set of native extensions is reused across simulated transactions.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Inserts a new entry, enforcing the per-transaction entry limit.
    /// Returns `Duplicate` if an entry already exists for `key` (regardless of its type)
    /// Returns `LimitExceeded` if inserting would exceed `max_scratch_pad_size`
    pub fn add(&mut self, key: AccountAddress, ty: Type, value: Value) -> AddResult {
        if self.entries.contains_key(&key) {
            return AddResult::Duplicate;
        }
        if self.entries.len() as u64 >= self.protocol_config.max_scratch_pad_size() {
            return AddResult::LimitExceeded;
        }
        self.entries.insert(key, ScratchEntry { ty, value });
        AddResult::Inserted
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
