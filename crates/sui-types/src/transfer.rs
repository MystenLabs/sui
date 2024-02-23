// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{binary_views::BinaryIndexedView, file_format::SignatureToken};
use move_bytecode_utils::resolve_struct;
use move_core_types::{
    account_address::AccountAddress,
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
};
use serde::{Deserialize, Serialize};

use crate::{
    base_types::{ObjectID, SequenceNumber},
    id::ID,
    SUI_FRAMEWORK_ADDRESS,
};

const TRANSFER_MODULE_NAME: &IdentStr = ident_str!("transfer");
const RECEIVING_STRUCT_NAME: &IdentStr = ident_str!("Receiving");

pub const RESOLVED_RECEIVING_STRUCT: (&AccountAddress, &IdentStr, &IdentStr) = (
    &SUI_FRAMEWORK_ADDRESS,
    TRANSFER_MODULE_NAME,
    RECEIVING_STRUCT_NAME,
);

/// Rust version of the Move sui::transfer::Receiving type
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Receiving {
    pub id: ID,
    pub version: SequenceNumber,
}

impl Receiving {
    pub fn new(id: ObjectID, version: SequenceNumber) -> Self {
        Self {
            id: ID::new(id),
            version,
        }
    }

    pub fn to_bcs_bytes(&self) -> Vec<u8> {
        bcs::to_bytes(self).expect("Value representation is owned and should always serialize")
    }

    pub fn struct_tag() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: TRANSFER_MODULE_NAME.to_owned(),
            name: RECEIVING_STRUCT_NAME.to_owned(),
            // TODO: this should really include the type parameters eventually when we add type
            // parameters to the other polymorphic types like this.
            type_params: vec![],
        }
    }

    pub fn type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(Self::struct_tag()))
    }

    pub fn is_receiving(view: &BinaryIndexedView<'_>, s: &SignatureToken) -> bool {
        use SignatureToken as S;
        match s {
            S::MutableReference(inner) | S::Reference(inner) => Self::is_receiving(view, inner),
            S::DatatypeInstantiation(idx, type_args) => {
                let struct_tag = resolve_struct(view, *idx);
                struct_tag == RESOLVED_RECEIVING_STRUCT && type_args.len() == 1
            }
            _ => false,
        }
    }
}
