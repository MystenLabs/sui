// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, Result};
use move_binary_format::{
    access::ModuleAccess,
    file_format::{AddressIdentifierIndex, IdentifierIndex, ModuleHandle, ModuleHandleIndex},
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
    /// For each k -> v pair, an instruction to subsitute v for k
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

    /// Return the index of the `ModuleHandle` with ID `module_id` in `m`'s module handle table,
    /// if there is one
    fn get_module_handle(module_id: &ModuleId, m: &CompiledModule) -> Option<ModuleHandleIndex> {
        m.module_handles
            .iter()
            .position(|h| &m.module_id_for_handle(h) == module_id)
            .map(|idx| ModuleHandleIndex(idx as u16))
    }

    /// Return the index of the `ModuleHandle` with ID `module_id` in `m`'s module handle table
    /// If there is no module handle for `module_id` in `m`'s module handle table, add it
    fn get_or_create_module_handle(
        module_id: &ModuleId,
        m: &mut CompiledModule,
    ) -> ModuleHandleIndex {
        Self::get_module_handle(module_id, m).unwrap_or_else(|| {
            let address = Self::get_or_create_address(module_id.address(), m);
            let name = Self::get_or_create_identifier(module_id.name(), m);
            let handle = ModuleHandle { address, name };
            let next_handle_idx = ModuleHandleIndex(m.module_handles.len() as u16);
            m.module_handles.push(handle);
            debug_assert!(
                &m.module_id_for_handle(m.module_handle_at(next_handle_idx)) == module_id
            );
            next_handle_idx
        })
    }

    /// Apply the module ID substituion in `self.sub_map` to `m`.
    /// Returns an error if the domain of `sub_map` contains a `ModuleID` without a corresponding handle in `m`
    pub fn sub_module_ids(&self, m: &mut CompiledModule) -> Result<()> {
        let mut handle_index_sub_map = BTreeMap::new();
        let friends_to_sub = m
            .friend_decls
            .iter()
            .enumerate()
            .filter_map(|(idx, h)| {
                let old_id = &m.module_id_for_handle(h);
                self.sub_map.get(old_id).map(|new_id| (idx, new_id))
            })
            .collect::<Vec<(usize, &ModuleId)>>();

        for (old_id, new_id) in self.sub_map.iter() {
            let old_handle_index = match Self::get_module_handle(old_id, m) {
                Some(idx) => idx,
                None => {
                    if friends_to_sub.iter().any(|(friend_handle_idx, _)| {
                        &m.module_id_for_handle(&m.friend_decls[*friend_handle_idx]) == old_id
                    }) {
                        // old_id is in the friends table; we will sub for it later
                        continue;
                    } else {
                        // `old_id` is not in the module table, and not a friend that we will sub for later. fail
                        bail!(
                            "Module ID {:?} in the sub_map domain not found in module `m`",
                            old_id
                        )
                    }
                }
            };
            let new_handle_index = Self::get_or_create_module_handle(new_id, m);
            let res = handle_index_sub_map.insert(old_handle_index, new_handle_index);
            debug_assert!(
                res.is_none(),
                "There should be exactly one handle for each input ID"
            )
        }
        // maps are always the same size unless there is a friend-only sub
        debug_assert!(handle_index_sub_map.len() <= self.sub_map.len());
        debug_assert!(
            !friends_to_sub.is_empty() || handle_index_sub_map.len() == self.sub_map.len()
        );

        // handle_index_sub_map is ready. walk through the bytecode, find everywhere a handle index in the
        // domain of the map is used, and replace it with a handle index in the range of the map

        // substitute self address
        if let Some(new_handle_index) = handle_index_sub_map.get(&m.self_module_handle_idx) {
            m.self_module_handle_idx = *new_handle_index
        };
        // substitute function handles
        for h in m.function_handles.iter_mut() {
            if let Some(new_handle_index) = handle_index_sub_map.get(&h.module) {
                h.module = *new_handle_index
            }
        }
        // substitute struct handles
        for h in m.struct_handles.iter_mut() {
            if let Some(new_handle_index) = handle_index_sub_map.get(&h.module) {
                h.module = *new_handle_index
            }
        }
        // substitute friends
        for (idx, new_id) in friends_to_sub {
            let new_addr = Self::get_or_create_address(new_id.address(), m);
            let new_name = Self::get_or_create_identifier(new_id.name(), m);
            m.friend_decls[idx].address = new_addr;
            m.friend_decls[idx].name = new_name;
        }

        Ok(())
    }
}
