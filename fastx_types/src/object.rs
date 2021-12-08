// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use std::convert::TryFrom;

use move_binary_format::CompiledModule;
use move_core_types::{account_address::AccountAddress, language_storage::StructTag};

use crate::base_types::{FastPayAddress, ObjectID, ObjectRef, SequenceNumber};

#[derive(Eq, PartialEq, Debug, Clone)]
pub struct MoveObject {
    pub type_: StructTag,
    pub contents: Vec<u8>,
}

#[derive(Eq, PartialEq, Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Data {
    /// An object whose governing logic lives in a published Move module
    Move(MoveObject),
    /// A published Move module
    Module(CompiledModule),
    // ... FastX "native" types go here
}

impl Data {
    pub fn is_read_only(&self) -> bool {
        use Data::*;
        match self {
            Move(_) => false,
            Module(_) => true,
        }
    }
}

#[derive(Eq, PartialEq, Debug, Clone)]
pub struct Object {
    /// The meat of the object
    pub data: Data,
    /// The authenticator that unlocks this object (eg. public key, or other)
    pub owner: FastPayAddress,
    pub next_sequence_number: SequenceNumber,
}

impl Object {
    pub fn to_object_reference(&self) -> ObjectRef {
        (self.id(), self.next_sequence_number)
    }

    pub fn id(&self) -> ObjectID {
        use Data::*;

        match &self.data {
            Move(v) => AccountAddress::try_from(&v.contents[0..16]).unwrap(), //unimplemented!("parse ID from bytes"), // TODO: parse from v
            Module(m) => *m.self_id().address(),
        }
    }

    /// Change the owner of `self` to `new_owner`
    // TODO: we do not want to support unconditional transfers of all objects. eliminate
    pub fn transfer(&mut self, new_owner: FastPayAddress) {
        // TODO: probably want to enforce imutability in type system instead of with dynamic checks
        assert!(
            !self.data.is_read_only(),
            "Cannot transfer an immutable object"
        );
        self.owner = new_owner;
    }

    // TODO: this should be test-only, but it's still used in bench and server
    pub fn with_id_for_testing(id: ObjectID) -> Self {
        use crate::base_types::PublicKeyBytes;
        use move_core_types::identifier::Identifier;

        let owner = PublicKeyBytes([0; 32]);
        let module = Identifier::new("Test").unwrap();
        let name = Identifier::new("Struct").unwrap();
        let type_params = Vec::new();
        let data = Data::Move(MoveObject {
            type_: StructTag {
                address: AccountAddress::new([0u8; AccountAddress::LENGTH]),
                module,
                name,
                type_params,
            },
            contents: id.to_vec(),
        });
        let next_sequence_number = SequenceNumber::new();
        Self {
            owner,
            data,
            next_sequence_number,
        }
    }
}
