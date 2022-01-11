// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};

use move_binary_format::CompiledModule;
use move_core_types::{account_address::AccountAddress, language_storage::StructTag};

use crate::{
    base_types::{
        sha3_hash, BcsSignable, FastPayAddress, ObjectDigest, ObjectID, ObjectRef, SequenceNumber,
        TransactionDigest,
    },
    error::{FastPayError, FastPayResult},
    gas_coin::GasCoin,
};

#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
pub struct MoveObject {
    pub type_: StructTag,
    contents: Vec<u8>,
    read_only: bool,
}

/// Byte encoding of a 64 byte unsigned integer in BCS
type BcsU64 = [u8; 8];
/// Index marking the end of the object's ID + the beginning of its version
const ID_END_INDEX: usize = 16;
/// Index marking the end of the object's version + the beginning of type-specific data
const VERSION_END_INDEX: usize = 24;

impl MoveObject {
    pub fn new(type_: StructTag, contents: Vec<u8>) -> Self {
        Self {
            type_,
            contents,
            read_only: false,
        }
    }

    pub fn id(&self) -> ObjectID {
        AccountAddress::try_from(&self.contents[0..ID_END_INDEX]).unwrap()
    }

    pub fn version(&self) -> SequenceNumber {
        SequenceNumber::from(u64::from_le_bytes(*self.version_bytes()))
    }

    /// Contents of the object that are specific to its type--i.e., not its ID and version, which all objects have
    /// For example if the object was declared as `struct S has key { id: ID, f1: u64, f2: bool },
    /// this returns the slice containing `f1` and `f2`.
    pub fn type_specific_contents(&self) -> &[u8] {
        &self.contents[VERSION_END_INDEX..]
    }

    ///
    pub fn id_version_contents(&self) -> &[u8] {
        &self.contents[..VERSION_END_INDEX]
    }

    /// Update the contents of this object and increment its version
    pub fn update_contents(&mut self, new_contents: Vec<u8>) -> FastPayResult<()> {
        #[cfg(debug_assertions)]
        let old_id = self.id();
        #[cfg(debug_assertions)]
        let old_version = self.version();

        self.contents = new_contents;

        #[cfg(debug_assertions)]
        {
            // caller should never overwrite ID or version
            debug_assert_eq!(self.id(), old_id);
            debug_assert_eq!(self.version(), old_version);
        }

        self.increment_version()?;
        Ok(())
    }

    /// Increase the version of this object by one
    pub fn increment_version(&mut self) -> FastPayResult<()> {
        let new_version = self.version().increment()?;
        // TODO: better bit tricks are probably possible here. for now, just do the obvious thing
        self.version_bytes_mut()
            .copy_from_slice(bcs::to_bytes(&new_version).unwrap().as_slice());
        Ok(())
    }

    fn version_bytes(&self) -> &BcsU64 {
        self.contents[ID_END_INDEX..VERSION_END_INDEX]
            .try_into()
            .unwrap()
    }

    fn version_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.contents[ID_END_INDEX..VERSION_END_INDEX]
    }

    pub fn contents(&self) -> &[u8] {
        &self.contents
    }

    pub fn into_contents(self) -> Vec<u8> {
        self.contents
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn freeze(&mut self) {
        self.read_only = true;
    }
}

#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
#[allow(clippy::large_enum_variant)]
pub enum Data {
    /// An object whose governing logic lives in a published Move module
    Move(MoveObject),
    /// Raw bytes that deserialize to a published Move module
    Module(Vec<u8>),
    // ... FastX "native" types go here
}

impl Data {
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

#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
pub struct Object {
    /// The meat of the object
    pub data: Data,
    /// The authenticator that unlocks this object (eg. public key, or other)
    pub owner: FastPayAddress,
    /// The digest of the order that created or last mutated this object
    pub previous_transaction: TransactionDigest,
}

impl BcsSignable for Object {}

impl Object {
    /// Create a new Move object
    pub fn new_move(
        o: MoveObject,
        owner: FastPayAddress,
        previous_transaction: TransactionDigest,
    ) -> Self {
        Object {
            data: Data::Move(o),
            owner,
            previous_transaction,
        }
    }

    pub fn new_module(
        m: CompiledModule,
        owner: FastPayAddress,
        previous_transaction: TransactionDigest,
    ) -> Self {
        let mut bytes = Vec::new();
        m.serialize(&mut bytes).unwrap();
        Object {
            data: Data::Module(bytes),
            owner,
            previous_transaction,
        }
    }

    pub fn is_read_only(&self) -> bool {
        match &self.data {
            Data::Move(m) => m.is_read_only(),
            Data::Module(_) => true,
        }
    }

    pub fn to_object_reference(&self) -> ObjectRef {
        (self.id(), self.version(), self.digest())
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

    pub fn version(&self) -> SequenceNumber {
        use Data::*;

        match &self.data {
            Move(v) => v.version(),
            Module(_) => SequenceNumber::from(0), // modules are immutable, version is always 0
        }
    }

    pub fn type_(&self) -> Option<&StructTag> {
        self.data.type_()
    }

    pub fn digest(&self) -> ObjectDigest {
        ObjectDigest::new(sha3_hash(self))
    }

    /// Change the owner of `self` to `new_owner`
    pub fn transfer(&mut self, new_owner: FastPayAddress) -> Result<(), FastPayError> {
        // TODO: these should be raised FastPayError's instead of panic's
        assert!(!self.is_read_only(), "Cannot transfer an immutable object");
        match &mut self.data {
            Data::Move(m) => {
                assert!(
                    m.type_ == GasCoin::type_(),
                    "Invalid transfer: only transfer of GasCoin is supported"
                );

                self.owner = new_owner;
                m.increment_version()?;
                Ok(())
            }
            Data::Module(_) => panic!("Cannot transfer a module object"),
        }
    }

    pub fn with_id_owner_gas_for_testing(
        id: ObjectID,
        version: SequenceNumber,
        owner: FastPayAddress,
        gas: u64,
    ) -> Self {
        let data = Data::Move(MoveObject {
            type_: GasCoin::type_(),
            contents: GasCoin::new(id, version, gas).to_bcs_bytes(),
            read_only: false,
        });
        Self {
            owner,
            data,
            previous_transaction: TransactionDigest::genesis(),
        }
    }

    pub fn with_id_owner_for_testing(id: ObjectID, owner: FastPayAddress) -> Self {
        // For testing, we provide sufficient gas by default.
        Self::with_id_owner_gas_for_testing(id, SequenceNumber::new(), owner, 100000_u64)
    }
}