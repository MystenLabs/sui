// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{InputObject, Object};
use fastcrypto::encoding::{Base58, Encoding};
use sui_types::{
    base_types::{SequenceNumber, SuiAddress as NativeSuiAddress},
    digests::ObjectDigest,
};

use crate::{
    api::scalars::{sui_address::SuiAddress, uint53::UInt53},
    error::RpcError,
};

pub(crate) struct Object {
    address: NativeSuiAddress,
    version: SequenceNumber,
    digest: ObjectDigest,
}

/// Identifies a specific version of an object.
#[derive(InputObject, Debug, Clone, Eq, PartialEq)]
pub(crate) struct ObjectKey {
    pub address: SuiAddress,
    pub version: UInt53,
}

/// An Object on Sui is either a typed value (a Move Object) or a Package (modules containing functions and types).
///
/// Every object on Sui is identified by a unique address, and has a version number that increases with every modification. Objects also hold metadata detailing their current owner (who can sign for access to the object and whether that access can modify and/or delete the object), and the digest of the last transaction that modified the object.
#[Object]
impl Object {
    /// The Object's ID.
    async fn address(&self) -> SuiAddress {
        self.address.into()
    }

    /// The version of this object that this content comes from.
    async fn version(&self) -> UInt53 {
        self.version.into()
    }

    /// 32-byte hash that identifies the object's contents, encoded in Base58.
    async fn digest(&self) -> String {
        Base58::encode(self.digest.inner())
    }
}

impl Object {
    /// Construct an object that is represented by just its identifier (its object reference). This
    /// does not check whether the object exists, so should not be used to "fetch" an object based
    /// on an address and/or version provided as user input.
    pub(crate) fn with_ref(
        address: NativeSuiAddress,
        version: SequenceNumber,
        digest: ObjectDigest,
    ) -> Self {
        Self {
            address,
            version,
            digest,
        }
    }

    /// Load the object at the given ID and version from the store, and return it fully inflated
    /// (with contents already fetched). Returns `None` if the object does not exist (either never
    /// existed or was pruned from the store).
    pub(crate) async fn fetch(
        address: SuiAddress,
        version: UInt53,
    ) -> Result<Option<Self>, RpcError> {
        // TODO: Actually fetch the transaction to check whether it exists.
        Ok(Some(Object::with_ref(
            address.into(),
            version.into(),
            ObjectDigest::random(),
        )))
    }
}
