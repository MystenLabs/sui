// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

use move_binary_format::CompiledModule;
use move_core_types::{account_address::AccountAddress, language_storage::StructTag};

use crate::{
    base_types::{
        sha3_hash, BcsSignable, FastPayAddress, ObjectDigest, ObjectID, ObjectRef, SequenceNumber,
        TransactionDigest,
    },
    gas_coin::GasCoin,
};

#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize)]
pub struct MoveObject {
    pub type_: StructTag,
    pub contents: Vec<u8>,
}

impl MoveObject {
    pub fn new(type_: StructTag, contents: Vec<u8>) -> Self {
        Self { type_, contents }
    }

    pub fn id(&self) -> ObjectID {
        AccountAddress::try_from(&self.contents[0..16]).unwrap()
    }
}

#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize)]
#[allow(clippy::large_enum_variant)]
pub enum Data {
    /// An object whose governing logic lives in a published Move module
    Move(MoveObject),
    /// Raw bytes that deserialize to a published Move module
    Module(Vec<u8>),
    // ... FastX "native" types go here
}

impl Data {
    pub fn is_read_only(&self) -> bool {
        use Data::*;
        match self {
            Move(_) => false,
            Module { .. } => true,
        }
    }

    pub fn try_as_move(&self) -> Option<&MoveObject> {
        use Data::*;
        match self {
            Move(m) => Some(m),
            Module(_) => None,
        }
    }

    pub fn try_as_move_mut(&mut self) -> Option<&mut MoveObject> {
        use Data::*;
        match self {
            Move(m) => Some(m),
            Module(_) => None,
        }
    }

    pub fn try_as_module(&self) -> Option<CompiledModule> {
        use Data::*;
        match self {
            Move(_) => None,
            Module(bytes) => CompiledModule::deserialize(bytes).ok(),
        }
    }

    pub fn type_(&self) -> Option<&StructTag> {
        use Data::*;
        match self {
            Move(m) => Some(&m.type_),
            Module(_) => None,
        }
    }
}

#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize)]
pub struct Object {
    /// The meat of the object
    pub data: Data,
    /// The authenticator that unlocks this object (eg. public key, or other)
    pub owner: FastPayAddress,
    /// The version of this object, starting at zero
    pub next_sequence_number: SequenceNumber,
    /// The digest of the order that created or last mutated this object
    pub previous_transaction: TransactionDigest,
}

impl BcsSignable for Object {}

impl Object {
    /// Create a new Move object
    pub fn new_move(
        o: MoveObject,
        owner: FastPayAddress,
        next_sequence_number: SequenceNumber,
        previous_transaction: TransactionDigest,
    ) -> Self {
        Object {
            data: Data::Move(o),
            owner,
            next_sequence_number,
            previous_transaction,
        }
    }

    pub fn new_module(
        m: CompiledModule,
        owner: FastPayAddress,
        next_sequence_number: SequenceNumber,
        previous_transaction: TransactionDigest,
    ) -> Self {
        let mut bytes = Vec::new();
        m.serialize(&mut bytes).unwrap();
        Object {
            data: Data::Module(bytes),
            owner,
            next_sequence_number,
            previous_transaction,
        }
    }

    pub fn is_read_only(&self) -> bool {
        self.data.is_read_only()
    }

    pub fn to_object_reference(&self) -> ObjectRef {
        (self.id(), self.next_sequence_number, self.digest())
    }

    pub fn id(&self) -> ObjectID {
        use Data::*;

        match &self.data {
            Move(v) => v.id(),
            Module(m) => {
                // TODO: extract ID by peeking into the bytes instead of deserializing
                *CompiledModule::deserialize(m).unwrap().self_id().address()
            }
        }
    }

    pub fn type_(&self) -> Option<&StructTag> {
        self.data.type_()
    }

    pub fn digest(&self) -> ObjectDigest {
        ObjectDigest::new(sha3_hash(self))
    }

    /// Change the owner of `self` to `new_owner`
    pub fn transfer(&mut self, new_owner: FastPayAddress) {
        // TODO: these should be raised FastPayError's instead of panic's
        assert!(
            !self.data.is_read_only(),
            "Cannot transfer an immutable object"
        );
        match self.type_() {
            Some(t) => {
                assert!(
                    t == &GasCoin::type_(),
                    "Invalid transfer: only transfer of GasCoin is supported"
                );
                self.owner = new_owner;
            }
            None => panic!("Cannot transfer a module object"),
        }
    }

    pub fn with_id_owner_gas_for_testing(id: ObjectID, owner: FastPayAddress, gas: u64) -> Self {
        let data = Data::Move(MoveObject {
            type_: GasCoin::type_(),
            contents: GasCoin::new(id, gas).to_bcs_bytes(),
        });
        let next_sequence_number = SequenceNumber::new();
        Self {
            owner,
            data,
            next_sequence_number,
            previous_transaction: TransactionDigest::genesis(),
        }
    }

    pub fn with_id_owner_for_testing(id: ObjectID, owner: FastPayAddress) -> Self {
        Self::with_id_owner_gas_for_testing(id, owner, 0)
    }

    // TODO: this should be test-only, but it's still used in bench and server
    pub fn with_id_for_testing(id: ObjectID) -> Self {
        let owner = FastPayAddress::default();
        Self::with_id_owner_for_testing(id, owner)
    }
}
