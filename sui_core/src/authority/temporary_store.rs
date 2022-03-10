// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use move_core_types::account_address::AccountAddress;
use sui_types::event::Event;

use super::*;

pub type InnerTemporaryStore = (
    BTreeMap<ObjectID, Object>,
    Vec<ObjectRef>,
    BTreeMap<ObjectID, Object>,
    BTreeMap<ObjectID, (SequenceNumber, DeleteKind)>,
    Vec<Event>,
);

pub struct AuthorityTemporaryStore {
    object_store: Arc<AuthorityStore>,
    tx_digest: TransactionDigest,
    objects: BTreeMap<ObjectID, Object>,
    active_inputs: Vec<ObjectRef>, // Inputs that are not read only
    // TODO: We need to study whether it's worth to optimize the lookup of
    // object reference by caching object reference in the map as well.
    // Object reference calculation involves hashing which could be expensive.
    written: BTreeMap<ObjectID, Object>, // Objects written
    /// Objects actively deleted.
    deleted: BTreeMap<ObjectID, (SequenceNumber, DeleteKind)>,
    /// Ordered sequence of events emitted by execution
    events: Vec<Event>,
}

impl AuthorityTemporaryStore {
    /// Creates a new store associated with an authority store, and populates it with
    /// initial objects.
    pub fn new(
        authority_state: &AuthorityState,
        _input_objects: &'_ [Object],
        tx_digest: TransactionDigest,
    ) -> AuthorityTemporaryStore {
        AuthorityTemporaryStore {
            object_store: authority_state._database.clone(),
            tx_digest,
            objects: _input_objects.iter().map(|v| (v.id(), v.clone())).collect(),
            active_inputs: _input_objects
                .iter()
                .filter(|v| !v.is_read_only())
                .map(|v| v.to_object_reference())
                .collect(),
            written: BTreeMap::new(),
            deleted: BTreeMap::new(),
            events: Vec::new(),
        }
    }

    // Helpers to access private fields
    pub fn objects(&self) -> &BTreeMap<ObjectID, Object> {
        &self.objects
    }

    pub fn written(&self) -> &BTreeMap<ObjectID, Object> {
        &self.written
    }

    pub fn deleted(&self) -> &BTreeMap<ObjectID, (SequenceNumber, DeleteKind)> {
        &self.deleted
    }

    /// Break up the structure and return its internal stores (objects, active_inputs, written, deleted)
    pub fn into_inner(self) -> InnerTemporaryStore {
        #[cfg(debug_assertions)]
        {
            self.check_invariants();
        }
        (
            self.objects,
            self.active_inputs,
            self.written,
            self.deleted,
            self.events,
        )
    }

    /// For every object from active_inputs (i.e. all mutable objects), if they are not
    /// mutated during the transaction execution, force mutating them by incrementing the
    /// sequence number. This is required to achieve safety.
    pub fn ensure_active_inputs_mutated(&mut self) {
        for (id, _seq, _) in self.active_inputs.iter() {
            if !self.written.contains_key(id) && !self.deleted.contains_key(id) {
                let mut object = self.objects[id].clone();
                // Active input object must be Move object.
                object.data.try_as_move_mut().unwrap().increment_version();
                self.written.insert(*id, object);
            }
        }
    }

    /// We need to special handle objects that was wrapped in the past and now unwrapped.
    /// When an object was wrapped at version `v`, we added an record into `parent_sync`
    /// with version `v+1` along with OBJECT_DIGEST_WRAPPED. Now when the object is unwrapped,
    /// it will also have version `v+1`, leading to a violation of the invariant that any
    /// object_id and version pair must be unique. Hence for any object that's just unwrapped,
    /// we force incrementing its version number again to make it `v+2` before writing to the store.
    pub fn patch_unwrapped_objects(&mut self, unwrapped_object_ids: &HashSet<ObjectID>) {
        for id in unwrapped_object_ids {
            // Unwrapped object could show up in either written or deleted.
            if let Some(object) = self.written.get_mut(id) {
                object.data.try_as_move_mut().unwrap().increment_version();
            } else {
                // unwrap safe because we constructed unwrapped_object_ids from written and deleted.
                // If the object is not in written, it must be in deleted.
                let entry = self.deleted.get_mut(id).unwrap();
                entry.0 = entry.0.increment();
            }
        }
        // self.deleted contains all object IDs that were passed through ID::delete_id.
        // However that doesn't necessarily indicate an object was deleted, if the object
        // didn't show up in the input. There are two cases, one is that the object just got
        // unwrapped, and another is just deletion of an ID that doesn't belong to a previous
        // existing object. The second case can be filtered out.
        self.deleted.retain(|id, (_version, kind)| {
            kind != &DeleteKind::NotExistInInput || unwrapped_object_ids.contains(id)
        });
    }

    pub fn to_signed_effects(
        &self,
        authority_name: &AuthorityName,
        secret: &dyn signature::Signer<AuthoritySignature>,
        transaction_digest: &TransactionDigest,
        transaction_dependencies: Vec<TransactionDigest>,
        status: ExecutionStatus,
        gas_object_id: &ObjectID,
        unwrapped_object_ids: HashSet<ObjectID>,
    ) -> SignedTransactionEffects {
        let gas_object = &self.written[gas_object_id];
        let effects = TransactionEffects {
            status,
            transaction_digest: *transaction_digest,
            created: self
                .written
                .iter()
                .filter(|(id, _)| {
                    !self.objects.contains_key(*id) && !unwrapped_object_ids.contains(*id)
                })
                .map(|(_, object)| (object.to_object_reference(), object.owner))
                .collect(),
            mutated: self
                .written
                .iter()
                .filter(|(id, _)| self.objects.contains_key(*id))
                .map(|(_, object)| (object.to_object_reference(), object.owner))
                .collect(),
            unwrapped: self
                .written
                .iter()
                .filter(|(id, _)| {
                    !self.objects.contains_key(*id) && unwrapped_object_ids.contains(*id)
                })
                .map(|(_, object)| (object.to_object_reference(), object.owner))
                .collect(),
            deleted: self
                .deleted
                .iter()
                .filter_map(|(id, (version, kind))| {
                    if kind != &DeleteKind::Wrap {
                        Some((*id, *version, ObjectDigest::OBJECT_DIGEST_DELETED))
                    } else {
                        None
                    }
                })
                .collect(),
            wrapped: self
                .deleted
                .iter()
                .filter_map(|(id, (version, kind))| {
                    if kind == &DeleteKind::Wrap {
                        Some((*id, *version, ObjectDigest::OBJECT_DIGEST_WRAPPED))
                    } else {
                        None
                    }
                })
                .collect(),
            gas_object: (gas_object.to_object_reference(), gas_object.owner),
            events: self.events.clone(),
            dependencies: transaction_dependencies,
        };
        let signature = AuthoritySignature::new(&effects, secret);

        SignedTransactionEffects {
            effects,
            authority: *authority_name,
            signature,
        }
    }

    /// An internal check of the invariants (will only fire in debug)
    #[cfg(debug_assertions)]
    fn check_invariants(&self) {
        // Check not both deleted and written
        debug_assert!(
            {
                let mut used = HashSet::new();
                self.written.iter().all(|(elt, _)| used.insert(elt));
                self.deleted.iter().all(move |elt| used.insert(elt.0))
            },
            "Object both written and deleted."
        );

        // Check all mutable inputs are either written or deleted
        debug_assert!(
            {
                let mut used = HashSet::new();
                self.written.iter().all(|(elt, _)| used.insert(elt));
                self.deleted.iter().all(|elt| used.insert(elt.0));

                self.active_inputs.iter().all(|elt| !used.insert(&elt.0))
            },
            "Mutable input neither written nor deleted."
        );
    }
}

impl Storage for AuthorityTemporaryStore {
    /// Resets any mutations and deletions recorded in the store.
    fn reset(&mut self) {
        self.written.clear();
        self.deleted.clear();
        self.events.clear();
    }

    fn read_object(&self, id: &ObjectID) -> Option<Object> {
        // there should be no read after delete
        debug_assert!(self.deleted.get(id) == None);
        match self.written.get(id) {
            Some(x) => Some(x.clone()),
            None => match self.objects.get(id) {
                Some(x) => Some(x.clone()),
                None => match self.object_store.get_object(id) {
                    Ok(o) => o,
                    Err(e) => panic!("Could not read object {}", e),
                },
            },
        }
    }

    /*
        Invariant: A key assumption of the write-delete logic
        is that an entry is not both added and deleted by the
        caller.
    */

    fn write_object(&mut self, mut object: Object) {
        // there should be no write after delete
        debug_assert!(self.deleted.get(&object.id()) == None);
        // Check it is not read-only
        #[cfg(test)] // Movevm should ensure this
        if let Some(existing_object) = self.read_object(&object.id()) {
            if existing_object.is_read_only() {
                // This is an internal invariant violation. Move only allows us to
                // mutate objects if they are &mut so they cannot be read-only.
                panic!("Internal invariant violation: Mutating a read-only object.")
            }
        }

        // The adapter is not very disciplined at filling in the correct
        // previous transaction digest, so we ensure it is correct here.
        object.previous_transaction = self.tx_digest;
        self.written.insert(object.id(), object);
    }

    fn delete_object(&mut self, id: &ObjectID, version: SequenceNumber, kind: DeleteKind) {
        // there should be no deletion after write
        debug_assert!(self.written.get(id) == None);
        // Check it is not read-only
        #[cfg(test)] // Movevm should ensure this
        if let Some(object) = self.read_object(id) {
            if object.is_read_only() {
                // This is an internal invariant violation. Move only allows us to
                // mutate objects if they are &mut so they cannot be read-only.
                panic!("Internal invariant violation: Deleting a read-only object.")
            }
        }

        // For object deletion, we increment their version so that they will
        // eventually show up in the parent_sync table with an updated version.
        self.deleted.insert(*id, (version.increment(), kind));
    }

    fn log_event(&mut self, event: Event) {
        self.events.push(event)
    }
}

impl ModuleResolver for AuthorityTemporaryStore {
    type Error = SuiError;
    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        match self.read_object(&ObjectID::from(*module_id.address())) {
            Some(o) => match &o.data {
                Data::Package(c) => Ok(c
                    .serialized_module_map()
                    .get(module_id.name().as_str())
                    .cloned()
                    .map(|m| m.into_vec())),
                _ => Err(SuiError::BadObjectType {
                    error: "Expected module object".to_string(),
                }),
            },
            None => Ok(None),
        }
    }
}

impl ResourceResolver for AuthorityTemporaryStore {
    type Error = SuiError;

    fn get_resource(
        &self,
        address: &AccountAddress,
        struct_tag: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        let object = match self.read_object(&ObjectID::from(*address)) {
            Some(x) => x,
            None => match self.read_object(&ObjectID::from(*address)) {
                None => return Ok(None),
                Some(x) => {
                    if !x.is_read_only() {
                        fp_bail!(SuiError::ExecutionInvariantViolation);
                    }
                    x
                }
            },
        };

        match &object.data {
            Data::Move(m) => {
                assert!(struct_tag == &m.type_, "Invariant violation: ill-typed object in storage or bad object request from caller\
");
                Ok(Some(m.contents().to_vec()))
            }
            other => unimplemented!(
                "Bad object lookup: expected Move object, but got {:?}",
                other
            ),
        }
    }
}
