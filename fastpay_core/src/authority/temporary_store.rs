use super::*;

pub type InnerTemporaryStore = (
    BTreeMap<ObjectID, Object>,
    Vec<ObjectRef>,
    BTreeMap<ObjectRef, Object>,
    Vec<ObjectRef>,
);

pub struct AuthorityTemporaryStore {
    object_store: Arc<AuthorityStore>,
    objects: BTreeMap<ObjectID, Object>,
    active_inputs: Vec<ObjectRef>, // Inputs that are not read only
    written: BTreeMap<ObjectRef, Object>, // Objects written
    deleted: Vec<ObjectRef>,       // Objects actively deleted
}

impl AuthorityTemporaryStore {
    /// Creates a new store associated with an authority store, and populates it with
    /// initial objects.
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
            written: BTreeMap::new(),
            deleted: Vec::new(),
        }
    }

    // Helpers to access private fields

    pub fn objects(&self) -> &BTreeMap<ObjectID, Object> {
        &self.objects
    }

    pub fn written(&self) -> &BTreeMap<ObjectRef, Object> {
        &self.written
    }

    pub fn deleted(&self) -> &Vec<ObjectRef> {
        &self.deleted
    }

    /// Break up the structure and return its internal stores (objects, active_inputs, written, deleted)
    pub fn into_inner(self) -> InnerTemporaryStore {
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
        status: ExecutionStatus,
    ) -> SignedOrderEffects {
        let effects = OrderEffects {
            status,
            transaction_digest: *transaction_digest,
            mutated: self.written.keys().cloned().collect(),
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
        // Now we are using a BTreeMap so by construction items in "written" are unique.

        // Check uniqueness in the 'deleted' set
        debug_assert!(
            {
                let mut used = HashSet::new();
                self.deleted.iter().all(move |elt| used.insert(elt.0))
            },
            "Duplicate object reference in self.deleted."
        );

        // Check not both deleted and written
        debug_assert!(
            {
                let mut used = HashSet::new();
                self.written.iter().all(|(elt, _)| used.insert(elt.0));
                self.deleted.iter().all(move |elt| used.insert(elt.0))
            },
            "Object both written and deleted."
        );

        // Check all mutable inputs are either written or deleted
        debug_assert!(
            {
                let mut used = HashSet::new();
                self.written.iter().all(|(elt, _)| used.insert(elt.0));
                self.deleted.iter().all(|elt| used.insert(elt.0));

                self.active_inputs.iter().all(|elt| !used.insert(elt.0))
            },
            "Mutable input neither written nor deleted."
        );
    }
}

impl Storage for AuthorityTemporaryStore {
    /// Resets any mutations and deletions recorded in the store.
    fn reset(&mut self) {
        self.active_inputs.clear();
        self.written.clear();
        self.deleted.clear();
    }

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

        self.written.insert(object.to_object_reference(), object);
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
                Data::Package(c) => Ok(c.get(module_id.name().as_str()).cloned()),
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
                Ok(Some(m.contents().to_vec()))
            }
            other => unimplemented!(
                "Bad object lookup: expected Move object, but got {:?}",
                other
            ),
        }
    }
}
