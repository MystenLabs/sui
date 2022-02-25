// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, Result};
use move_binary_format::{
    file_format::{AddressIdentifierIndex, IdentifierIndex},
    CompiledModule,
};
use move_core_types::{
    account_address::AccountAddress, identifier::IdentStr, language_storage::ModuleId,
};
use std::collections::BTreeMap;

#[cfg(test)]
#[path = "unit_tests/bytecode_rewriter_tests.rs"]
mod bytecode_rewriter_tests;

/// A bytecode rewriting tool for substituting module handles
#[derive(Debug)]
pub struct ModuleHandleRewriter {
    /// For each k -> v pair, an instruction to replace k by v
    /// Domain and range of the map are disjoint
    sub_map: BTreeMap<ModuleId, ModuleId>,
}

impl ModuleHandleRewriter {
    /// Add an instruction to sub `old` for `new`. Returns the id previously bound to `old` (if any)
    /// Returns an error if `new` is in the domain of the map
    pub fn new(sub_map: BTreeMap<ModuleId, ModuleId>) -> Result<Self> {
        for v in sub_map.values() {
            if sub_map.contains_key(v) {
                bail!("Domain and range of the sub map must be disjoint")
            }
        }
        Ok(Self { sub_map })
    }

    /// Return the index of `a` in `m`'s address table, if there is one
    fn get_address(a: &AccountAddress, m: &CompiledModule) -> Option<AddressIdentifierIndex> {
        m.address_identifiers
            .iter()
            .position(|addr| a == addr)
            .map(|idx| AddressIdentifierIndex(idx as u16))
    }

    /// Return the index of `a` in `m`'s address table.
    /// If `a` is not already in `m`'s address table, add it
    fn get_or_create_address(a: &AccountAddress, m: &mut CompiledModule) -> AddressIdentifierIndex {
        Self::get_address(a, m).unwrap_or_else(|| {
            let next_idx = AddressIdentifierIndex(m.address_identifiers.len() as u16);
            m.address_identifiers.push(*a);
            next_idx
        })
    }

    /// Return the index of `i` in `m`'s identifier table, if there is one
    /// If `a` is not already in `m`'s identifier table, add it
    fn get_identifier(i: &IdentStr, m: &CompiledModule) -> Option<IdentifierIndex> {
        m.identifiers
            .iter()
            .position(|id| i == id.as_ident_str())
            .map(|idx| IdentifierIndex(idx as u16))
    }

    /// Return the index of `i` in `m`'s identifier table
    /// If `a` is not already in `m`'s identifier table, add it
    fn get_or_create_identifier(i: &IdentStr, m: &mut CompiledModule) -> IdentifierIndex {
        Self::get_identifier(i, m).unwrap_or_else(|| {
            let next_idx = IdentifierIndex(m.identifiers.len() as u16);
            m.identifiers.push(i.to_owned());
            next_idx
        })
    }

    /// Apply the module ID substitution in `self.sub_map` to `m`.
    /// Returns an error if the domain of `sub_map` contains a `ModuleID` without a corresponding handle in `m`
    pub fn sub_module_ids(&self, m: &mut CompiledModule) {
        let handles_to_sub = m
            .module_handles
            .iter()
            .enumerate()
            .filter_map(|(idx, h)| {
                let old_id = &m.module_id_for_handle(h);
                self.sub_map.get(old_id).map(|new_id| (idx, new_id))
            })
            .collect::<Vec<(usize, &ModuleId)>>();
        let friends_to_sub = m
            .friend_decls
            .iter()
            .enumerate()
            .filter_map(|(idx, h)| {
                let old_id = &m.module_id_for_handle(h);
                self.sub_map.get(old_id).map(|new_id| (idx, new_id))
            })
            .collect::<Vec<(usize, &ModuleId)>>();
        // substitute module handles
        for (idx, new_id) in handles_to_sub {
            let new_addr = Self::get_or_create_address(new_id.address(), m);
            let new_name = Self::get_or_create_identifier(new_id.name(), m);
            m.module_handles[idx].address = new_addr;
            m.module_handles[idx].name = new_name;
        }
        // substitute friends
        for (idx, new_id) in friends_to_sub {
            let new_addr = Self::get_or_create_address(new_id.address(), m);
            let new_name = Self::get_or_create_identifier(new_id.name(), m);
            m.friend_decls[idx].address = new_addr;
            m.friend_decls[idx].name = new_name;
        }
        // check that substitution did not create duplicate handles or friends. the Move verifier will
        // also check this later, but it's easier to understand what's going on if we raise the alarm
        // here.
        // TODO: if we wanted to, we could support rewriting the tables to eliminate duplicates,
        // but this is more involved + will never happen in Sui's usage of the rewriter
        // (modulo hash collisions in ID generation).
        #[cfg(debug_assertions)]
        {
            Self::check_no_duplicate_handles(m)
        }
    }

    #[cfg(debug_assertions)]
    fn check_no_duplicate_handles(m: &CompiledModule) {
        use std::collections::HashSet;

        let mut module_handles = HashSet::new();
        let mut friends = HashSet::new();

        // Note: it's also possible that the duplicate existed before rewriting. We don't check this
        debug_assert!(
            m.module_handles.iter().all(|h| module_handles.insert(h)),
            "Bytecode rewriting introduced duplicate module handle"
        );
        debug_assert!(
            m.friend_decls.iter().all(|h| friends.insert(h)),
            "Bytecode rewriting introduced duplicate friends"
        );
    }

    pub fn into_inner(self) -> BTreeMap<ModuleId, ModuleId> {
        self.sub_map
    }
}
