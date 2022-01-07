use super::*;

pub struct AuthorityTemporaryStore {
    object_store: Arc<AuthorityStore>,
    objects: BTreeMap<ObjectID, Object>,
    active_inputs: Vec<ObjectRef>, // Inputs that are not read only
    pub written: Vec<ObjectRef>,   // Objects written
    deleted: Vec<ObjectRef>,       // Objects actively deleted
}

impl AuthorityTemporaryStore {
    pub fn new(
        authority_state: &AuthorityState,
        _input_objects: &'_ [Object],
    ) -> AuthorityTemporaryStore {
        AuthorityTemporaryStore {
            object_store: authority_state._database.clone(),
            objects: _input_objects.iter().map(|v| (v.id(), v.clone())).collect(),
            active_inputs: _input_objects
                .iter()
                .filter(|v| !v.is_read_only())
                .map(|v| v.to_object_reference())
                .collect(),
            written: Vec::new(),
            deleted: Vec::new(),
        }
    }

    /// Break up the structure and return its internal stores (objects, active_inputs, written, deleted)
    pub fn into_inner(
        self,
    ) -> (
        BTreeMap<ObjectID, Object>,
        Vec<ObjectRef>,
        Vec<ObjectRef>,
        Vec<ObjectRef>,
    ) {
        #[cfg(debug_assertions)]
        {
            self.check_invariants();
        }
        (self.objects, self.active_inputs, self.written, self.deleted)
    }

    pub fn to_signed_effects(
        &self,
        authority_name: &AuthorityName,
        secret: &KeyPair,
        transaction_digest: &TransactionDigest,
        status: Result<(), FastPayError>,
    ) -> SignedOrderEffects {
        let effects = OrderEffects {
            status,
            transaction_digest: *transaction_digest,
            mutated: self.written.clone(),
            deleted: self.deleted.clone(),
        };
        let signature = Signature::new(&effects, secret);

        SignedOrderEffects {
            effects,
            authority: *authority_name,
            signature,
        }
    }

    /// An internal check of the invariants (will only fire in debug)
    #[cfg(debug_assertions)]
    fn check_invariants(&self) {
        // Check uniqueness in the 'written' set
        debug_assert!(
            {
                use std::collections::HashSet;
                let mut used = HashSet::new();
                self.written.iter().all(move |elt| used.insert(elt.0))
            },
            "Duplicate object reference in self.written."
        );

        // Check uniqueness in the 'deleted' set
        debug_assert!(
            {
                use std::collections::HashSet;
                let mut used = HashSet::new();
                self.deleted.iter().all(move |elt| used.insert(elt.0))
            },
            "Duplicate object reference in self.deleted."
        );

        // Check not both deleted and written
        debug_assert!(
            {
                use std::collections::HashSet;
                let mut used = HashSet::new();
                self.written.iter().all(|elt| used.insert(elt.0));
                self.deleted.iter().all(move |elt| used.insert(elt.0))
            },
            "Object both written and deleted."
        );

        // Check all mutable inputs are either written or deleted
        debug_assert!(
            {
                use std::collections::HashSet;
                let mut used = HashSet::new();
                self.written.iter().all(|elt| used.insert(elt.0));
                self.deleted.iter().all(|elt| used.insert(elt.0));

                self.active_inputs.iter().all(|elt| !used.insert(elt.0))
            },
            "Mutable input neither written nor deleted."
        );
    }
}

impl Storage for AuthorityTemporaryStore {
    fn read_object(&self, id: &ObjectID) -> Option<Object> {
        match self.objects.get(id) {
            Some(x) => Some(x.clone()),
            None => {
                let object = self.object_store.object_state(id);
                match object {
                    Ok(o) => Some(o),
                    Err(FastPayError::ObjectNotFound) => None,
                    _ => panic!("Cound not read object"),
                }
            }
        }
    }

    /*
        Invariant: A key assumption of the write-delete logic
        is that an entry is not both added and deleted by the
        caller.
    */

    fn write_object(&mut self, object: Object) {
        // Check it is not read-only
        #[cfg(test)] // Movevm should ensure this
        if let Some(existing_object) = self.read_object(&object.id()) {
            if existing_object.is_read_only() {
                // This is an internal invariant violation. Move only allows us to
                // mutate objects if they are &mut so they cannot be read-only.
                panic!("Internal invariant violation: Mutating a read-only object.")
            }
        }

        self.written.push(object.to_object_reference());
        self.objects.insert(object.id(), object);
    }

    fn delete_object(&mut self, id: &ObjectID) {
        // Check it is not read-only
        #[cfg(test)] // Movevm should ensure this
        if let Some(object) = self.read_object(id) {
            if object.is_read_only() {
                // This is an internal invariant violation. Move only allows us to
                // mutate objects if they are &mut so they cannot be read-only.
                panic!("Internal invariant violation: Deleting a read-only object.")
            }
        }

        // If it exists remove it
        if let Some(removed) = self.objects.remove(id) {
            self.deleted.push(removed.to_object_reference());
        } else {
            panic!("Internal invariant: object must exist to be deleted.")
        }
    }
}

impl ModuleResolver for AuthorityTemporaryStore {
    type Error = FastPayError;
    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        match self.read_object(module_id.address()) {
            Some(o) => match &o.data {
                Data::Module(c) => Ok(Some(c.clone())),
                _ => Err(FastPayError::BadObjectType {
                    error: "Expected module object".to_string(),
                }),
            },
            None => Ok(None),
        }
    }
}

impl ResourceResolver for AuthorityTemporaryStore {
    type Error = FastPayError;

    fn get_resource(
        &self,
        address: &AccountAddress,
        struct_tag: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        let object = match self.read_object(address) {
            Some(x) => x,
            None => match self.read_object(address) {
                None => return Ok(None),
                Some(x) => {
                    if !x.is_read_only() {
                        fp_bail!(FastPayError::ExecutionInvariantViolation);
                    }
                    x
                }
            },
        };

        match &object.data {
            Data::Move(m) => {
                assert!(struct_tag == &m.type_, "Invariant violation: ill-typed object in storage or bad object request from caller\
");
                Ok(Some(m.contents.clone()))
            }
            other => unimplemented!(
                "Bad object lookup: expected Move object, but got {:?}",
                other
            ),
        }
    }
}
