// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Object;
use fastcrypto::encoding::{Base58, Encoding};
use sui_types::digests::TransactionDigest;

use crate::{api::scalars::digest::Digest, error::RpcError};

pub(crate) struct Transaction {
    digest: TransactionDigest,
}

/// Description of a transaction, the unit of activity on Sui.
#[Object]
impl Transaction {
    /// A 32-byte hash that uniquely identifies the transaction contents, encoded in Base58.
    async fn digest(&self) -> String {
        Base58::encode(self.digest)
    }
}

impl Transaction {
    /// Construct a transaction that is represented by just its identifier (its transaction
    /// digest). This does not check whether the transaction exists, so should not be used to
    /// "fetch" a transaction based on a digest provided as user input.
    pub(crate) fn with_id(digest: TransactionDigest) -> Self {
        Self { digest }
    }

    /// Load the transaction from the store, and return it fully inflated (with contents already
    /// fetched). Returns `None` if the transaction does not exist (either never existed or was
    /// pruned from the store).
    pub(crate) async fn fetch(digest: Digest) -> Result<Option<Self>, RpcError> {
        // TODO: Actually fetch the transaction to check whether it exists.
        Ok(Some(Transaction::with_id(digest.into())))
    }
}
