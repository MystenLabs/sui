// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{Context, Object};
use fastcrypto::encoding::{Base58, Encoding};
use sui_indexer_alt_reader::kv_loader::{
    KvLoader, TransactionContents as NativeTransactionContents,
};
use sui_types::{
    base_types::SuiAddress as NativeSuiAddress,
    digests::TransactionDigest,
    transaction::{TransactionDataAPI, TransactionExpiration},
};

use crate::{
    api::{
        scalars::{base64::Base64, digest::Digest},
        types::epoch::Epoch,
    },
    error::RpcError,
    scope::Scope,
};

use super::{
    address::Address,
    gas_input::GasInput,
    transaction_effects::{EffectsContents, TransactionEffects},
};

#[derive(Clone)]
pub(crate) struct Transaction {
    pub(crate) digest: TransactionDigest,
    pub(crate) contents: TransactionContents,
}

#[derive(Clone)]
pub(crate) struct TransactionContents {
    pub(crate) scope: Scope,
    pub(crate) contents: Option<Arc<NativeTransactionContents>>,
}

/// Description of a transaction, the unit of activity on Sui.
#[Object]
impl Transaction {
    /// A 32-byte hash that uniquely identifies the transaction contents, encoded in Base58.
    async fn digest(&self) -> String {
        Base58::encode(self.digest)
    }

    /// The results to the chain of executing this transaction.
    async fn effects(&self) -> Option<TransactionEffects> {
        Some(TransactionEffects::from(self.clone()))
    }

    #[graphql(flatten)]
    async fn contents(&self, ctx: &Context<'_>) -> Result<TransactionContents, RpcError> {
        self.contents.fetch(ctx, self.digest).await
    }
}

#[Object]
impl TransactionContents {
    /// This field is set by senders of a transaction block. It is an epoch reference that sets a deadline after which validators will no longer consider the transaction valid. By default, there is no deadline for when a transaction must execute.
    async fn expiration(&self) -> Result<Option<Epoch>, RpcError> {
        let Some(content) = &self.contents else {
            return Ok(None);
        };

        let transaction_data = content.data()?;
        match transaction_data.expiration() {
            TransactionExpiration::None => Ok(None),
            TransactionExpiration::Epoch(epoch_id) => {
                Ok(Some(Epoch::with_id(self.scope.clone(), *epoch_id)))
            }
        }
    }

    /// The gas input field provides information on what objects were used as gas as well as the owner of the gas object(s) and information on the gas price and budget.
    async fn gas_input(&self) -> Result<Option<GasInput>, RpcError> {
        let Some(content) = &self.contents else {
            return Ok(None);
        };

        let transaction_data = content.data()?;
        Ok(Some(GasInput::from_gas_data(
            self.scope.clone(),
            transaction_data.gas_data().clone(),
        )))
    }

    /// The address corresponding to the public key that signed this transaction. System transactions do not have senders.
    async fn sender(&self) -> Result<Option<Address>, RpcError> {
        let Some(content) = &self.contents else {
            return Ok(None);
        };

        let sender = content.data()?.sender();
        Ok((sender != NativeSuiAddress::ZERO)
            .then(|| Address::with_address(self.scope.clone(), sender)))
    }

    /// The Base64-encoded BCS serialization of this transaction, as a `TransactionData`.
    async fn transaction_bcs(&self) -> Result<Option<Base64>, RpcError> {
        let Some(content) = &self.contents else {
            return Ok(None);
        };

        Ok(Some(Base64(content.raw_transaction()?)))
    }
}

impl Transaction {
    /// Construct a transaction that is represented by just its identifier (its transaction
    /// digest). This does not check whether the transaction exists, so should not be used to
    /// "fetch" a transaction based on a digest provided as user input.
    pub(crate) fn with_id(scope: Scope, digest: TransactionDigest) -> Self {
        Self {
            digest,
            contents: TransactionContents::empty(scope),
        }
    }

    /// Load the transaction from the store, and return it fully inflated (with contents already
    /// fetched). Returns `None` if the transaction does not exist (either never existed or was
    /// pruned from the store).
    pub(crate) async fn fetch(
        ctx: &Context<'_>,
        scope: Scope,
        digest: Digest,
    ) -> Result<Option<Self>, RpcError> {
        let contents = TransactionContents::empty(scope)
            .fetch(ctx, digest.into())
            .await?;

        let Some(tx) = &contents.contents else {
            return Ok(None);
        };

        Ok(Some(Self {
            digest: tx.digest()?,
            contents,
        }))
    }
}

impl TransactionContents {
    fn empty(scope: Scope) -> Self {
        Self {
            scope,
            contents: None,
        }
    }

    /// Attempt to fill the contents. If the contents are already filled, returns a clone,
    /// otherwise attempts to fetch from the store. The resulting value may still have an empty
    /// contents field, because it could not be found in the store.
    pub(crate) async fn fetch(
        &self,
        ctx: &Context<'_>,
        digest: TransactionDigest,
    ) -> Result<Self, RpcError> {
        if self.contents.is_some() {
            return Ok(self.clone());
        }

        let kv_loader: &KvLoader = ctx.data()?;
        let Some(transaction) = kv_loader
            .load_one_transaction(digest)
            .await
            .context("Failed to fetch transaction contents")?
        else {
            return Ok(self.clone());
        };

        // Discard the loaded result if we are viewing it at a checkpoint before it existed.
        if transaction.cp_sequence_number() > self.scope.checkpoint_viewed_at() {
            return Ok(self.clone());
        }

        Ok(Self {
            scope: self.scope.clone(),
            contents: Some(Arc::new(transaction)),
        })
    }
}

impl From<TransactionEffects> for Transaction {
    fn from(fx: TransactionEffects) -> Self {
        let EffectsContents { scope, contents } = fx.contents;

        Self {
            digest: fx.digest,
            contents: TransactionContents { scope, contents },
        }
    }
}
