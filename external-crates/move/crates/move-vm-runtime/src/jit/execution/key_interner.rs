// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::arena::{ArenaBox, ArenaBuilder},
    execution::dispatch_tables::VirtualTableKey,
    shared::vm_pointer::VMPointer,
};
use move_binary_format::errors::PartialVMResult;
use std::collections::{HashMap, hash_map::Entry};

/// Canonical keys belong to the package arena, not the translation-time lookup table.
#[derive(Default)]
pub(crate) struct VirtualTableKeyInterner {
    keys: HashMap<VirtualTableKey, ArenaBox<VirtualTableKey>>,
}

impl VirtualTableKeyInterner {
    pub(crate) fn intern(
        &mut self,
        arena: &ArenaBuilder,
        key: VirtualTableKey,
    ) -> PartialVMResult<VMPointer<VirtualTableKey>> {
        let key = match self.keys.entry(key) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let allocated = arena.alloc_box(entry.key().clone())?;
                entry.insert(allocated)
            }
        };
        Ok(VMPointer::from_ref(key.inner_ref()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::cache::identifier_interner::IdentifierInterner;
    use move_core_types::{account_address::AccountAddress, identifier::Identifier};
    use move_vm_config::runtime::VMConfig;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn key(address: AccountAddress, module: &str, member: &str) -> VirtualTableKey {
        let interner = IdentifierInterner::new();
        let module = interner.intern_identifier(&Identifier::new(module).unwrap());
        let member = interner.intern_identifier(&Identifier::new(member).unwrap());
        VirtualTableKey::from_parts(address, module, member)
    }

    #[test]
    fn repeated_keys_share_storage_after_translation() -> PartialVMResult<()> {
        let arena = ArenaBuilder::new_bounded(&VMConfig::new_for_test(false, None));
        let mut interner = VirtualTableKeyInterner::default();
        let key = key(AccountAddress::TWO, "module", "member");
        let first = interner.intern(&arena, key.clone())?;
        let second = interner.intern(&arena, key.clone())?;
        assert!(first.ptr_eq(&second));
        assert_eq!(interner.keys.len(), 1);
        drop(interner);
        let _arena = arena.finish();
        assert_eq!(*first, key);
        assert_eq!(*second, key);
        Ok(())
    }

    #[test]
    fn package_module_and_member_distinguish_keys() -> PartialVMResult<()> {
        let arena = ArenaBuilder::new_bounded(&VMConfig::new_for_test(false, None));
        let mut interner = VirtualTableKeyInterner::default();
        let names = IdentifierInterner::new();
        let module = names.intern_identifier(&Identifier::new("module").unwrap());
        let member = names.intern_identifier(&Identifier::new("member").unwrap());
        let other = names.intern_identifier(&Identifier::new("other").unwrap());
        let keys = [
            VirtualTableKey::from_parts(AccountAddress::ONE, module, member),
            VirtualTableKey::from_parts(AccountAddress::TWO, module, member),
            VirtualTableKey::from_parts(AccountAddress::ONE, other, member),
            VirtualTableKey::from_parts(AccountAddress::ONE, module, other),
        ];
        let pointers = keys
            .into_iter()
            .map(|key| interner.intern(&arena, key))
            .collect::<PartialVMResult<Vec<_>>>()?;
        for (index, left) in pointers.iter().enumerate() {
            for right in &pointers[index + 1..] {
                assert!(!left.ptr_eq(right));
                assert_ne!(left, right);
            }
        }
        Ok(())
    }

    #[test]
    fn equality_and_hashing_are_structural_across_arenas() -> PartialVMResult<()> {
        let config = VMConfig::new_for_test(false, None);
        let first_arena = ArenaBuilder::new_bounded(&config);
        let second_arena = ArenaBuilder::new_bounded(&config);
        let key = key(AccountAddress::TWO, "module", "member");
        let first = VirtualTableKeyInterner::default().intern(&first_arena, key.clone())?;
        let second = VirtualTableKeyInterner::default().intern(&second_arena, key)?;
        assert!(!first.ptr_eq(&second));
        assert_eq!(first, second);
        let hash = |key: &VMPointer<VirtualTableKey>| {
            let mut hasher = DefaultHasher::new();
            key.hash(&mut hasher);
            hasher.finish()
        };
        assert_eq!(hash(&first), hash(&second));
        Ok(())
    }

    #[test]
    fn failed_allocation_does_not_insert_key() {
        let mut config = VMConfig::new_for_test(false, None);
        config.runtime_limits_config.package_arena_size = Some(0);
        let arena = ArenaBuilder::new_bounded(&config);
        let mut interner = VirtualTableKeyInterner::default();
        let error = interner
            .intern(&arena, key(AccountAddress::TWO, "module", "member"))
            .unwrap_err();
        assert_eq!(
            error.major_status(),
            move_core_types::vm_status::StatusCode::PACKAGE_ARENA_LIMIT_REACHED
        );
        assert!(interner.keys.is_empty());
    }
}
