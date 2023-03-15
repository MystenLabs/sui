// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress},
    crypto::AccountKeyPair,
    messages::{TransactionData, TransactionDataAPI, VerifiedTransaction},
    object::Owner,
    utils::to_sender_signed_transaction,
};

use crate::ExecutionEffects;

/// A Sui account and all of the objects it owns
#[derive(Debug)]
pub struct SuiAccount {
    key: Arc<AccountKeyPair>,
    owned: BTreeMap<ObjectID, ObjectRef>,
    // TODO: optional type info
}

impl SuiAccount {
    pub fn new(key: Arc<AccountKeyPair>, objs: Vec<ObjectRef>) -> Self {
        let owned = objs.into_iter().map(|obj| (obj.0, obj)).collect();
        SuiAccount { key, owned }
    }

    /// Update the state associated with `obj`, adding it if it doesn't exist
    pub fn add_or_update(&mut self, obj: ObjectRef) -> Option<ObjectRef> {
        self.owned.insert(obj.0, obj)
    }

    /// Delete `id` and return the old value
    pub fn delete(&mut self, id: &ObjectID) -> Option<ObjectRef> {
        self.owned.remove(id)
    }
}

/// Utility struct tracking keys for known accounts, owned objects, shared objects, and immutable objects
#[derive(Debug, Default)]
pub struct InMemoryWallet {
    accounts: BTreeMap<SuiAddress, SuiAccount>, // TODO: track shared and immutable objects as well
}

impl InMemoryWallet {
    pub fn add_account(
        &mut self,
        addr: SuiAddress,
        key: Arc<AccountKeyPair>,
        objs: Vec<ObjectRef>,
    ) {
        self.accounts.insert(addr, SuiAccount::new(key, objs));
    }

    /// Apply updates from `effects` to `self`
    pub fn update(&mut self, effects: &ExecutionEffects) {
        for (obj, owner) in effects.mutated().into_iter().chain(effects.created()) {
            if let Owner::AddressOwner(a) = owner {
                if let Some(account) = self.accounts.get_mut(&a) {
                    account.add_or_update(obj);
                } // else, doesn't belong to an account we can spend from, we don't care
            } // TODO: support owned, shared objects
        }
        if let Some(sender_account) = self.accounts.get_mut(&effects.sender()) {
            for obj in effects.deleted() {
                // by construction, every deleted object either
                // 1. belongs to the tx sender directly (e.g., sender owned the object)
                // 2. belongs to the sender indirectly (e.g., deleted object was a dynamic field of a object the sender owned)
                // 3. is shared (though we do not yet support deletion of shared objects)
                // so, we just try to delete everything from the sender's account here, though it's
                // worth noting that (2) and (3) are possible.
                sender_account.delete(&obj.0);
            }
        } // else, tx sender is not an account we can spend from, we don't care
    }

    pub fn owned_object(&self, addr: &SuiAddress, id: &ObjectID) -> Option<&ObjectRef> {
        self.accounts.get(addr).and_then(|a| a.owned.get(id))
    }

    pub fn owned_objects(&self, addr: &SuiAddress) -> Option<impl Iterator<Item = &ObjectRef>> {
        self.accounts.get(addr).map(|a| a.owned.values())
    }

    pub fn create_tx(&self, data: TransactionData) -> VerifiedTransaction {
        let sender = data.sender();
        to_sender_signed_transaction(data, self.accounts.get(&sender).unwrap().key.as_ref())
    }

    pub fn keypair(&self, addr: &SuiAddress) -> Option<Arc<AccountKeyPair>> {
        self.accounts.get(addr).map(|a| a.key.clone())
    }

    pub fn num_addresses(&self) -> usize {
        self.accounts.len()
    }

    pub fn addresses(&self) -> impl Iterator<Item = &SuiAddress> {
        self.accounts.keys()
    }

    pub fn total_objects(&self) -> usize {
        let mut total = 0;
        for account in self.accounts.values() {
            total += account.owned.len()
        }
        total
    }
}
