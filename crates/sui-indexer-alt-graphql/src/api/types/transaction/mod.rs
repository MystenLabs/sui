// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{ops::Range, sync::Arc};

use anyhow::Context as _;
use async_graphql::{
    connection::{Connection, CursorType, Edge},
    dataloader::DataLoader,
    Context, Object,
};
use fastcrypto::encoding::{Base58, Encoding};
use sui_indexer_alt_reader::{
    kv_loader::{KvLoader, TransactionContents as NativeTransactionContents},
    pg_reader::PgReader,
    tx_digests::TxDigestKey,
};

use sui_types::{
    base_types::SuiAddress as NativeSuiAddress,
    digests::TransactionDigest,
    transaction::{TransactionDataAPI, TransactionExpiration},
};

use crate::{
    api::scalars::{base64::Base64, cursor::JsonCursor, digest::Digest},
    error::RpcError,
    pagination::Page,
    scope::Scope,
    task::watermark::Watermarks,
};

use super::{
    address::Address,
    checkpoint::filter::checkpoint_bounds,
    epoch::Epoch,
    gas_input::GasInput,
    transaction::filter::{tx_bounds, TransactionFilter},
    transaction_effects::{EffectsContents, TransactionEffects},
    user_signature::UserSignature,
};

use super::transaction_kind::TransactionKind;

pub(crate) mod filter;

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

pub(crate) type CTransaction = JsonCursor<u64>;

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

    /// The type of this transaction as well as the commands and/or parameters comprising the transaction of this kind.
    async fn kind(&self, ctx: &Context<'_>) -> Result<Option<TransactionKind>, RpcError> {
        let contents = self.contents.fetch(ctx, self.digest).await?;
        let Some(content) = &contents.contents else {
            return Ok(None);
        };

        let transaction_data = content.data()?;
        Ok(TransactionKind::from(
            transaction_data.kind().clone(),
            contents.scope.clone(),
        ))
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

    /// User signatures for this transaction.
    async fn signatures(&self) -> Result<Vec<UserSignature>, RpcError> {
        let Some(content) = &self.contents else {
            return Ok(vec![]);
        };

        let signatures = content.signatures()?;
        Ok(signatures
            .into_iter()
            .map(UserSignature::from_generic_signature)
            .collect())
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

    /// Cursor based pagination through transactions with filters applied.
    ///
    /// Returns empty results when no checkpoint is set in scope (e.g. execution scope).
    pub(crate) async fn paginate(
        ctx: &Context<'_>,
        scope: Scope,
        page: Page<CTransaction>,
        filter: TransactionFilter,
    ) -> Result<Connection<String, Transaction>, RpcError> {
        let Some(checkpoint_viewed_at) = scope.checkpoint_viewed_at() else {
            return Ok(Connection::new(false, false));
        };

        let mut conn = Connection::new(false, false);

        if page.limit() == 0 {
            return Ok(Connection::new(false, false));
        }

        let watermarks: &Arc<Watermarks> = ctx.data()?;

        let reader_lo = watermarks.pipeline_lo_watermark("tx_digests")?.checkpoint();

        let global_tx_hi = watermarks.high_watermark().transaction();

        let Some(cp_bounds) = checkpoint_bounds(
            filter.after_checkpoint.map(u64::from),
            filter.at_checkpoint.map(u64::from),
            filter.before_checkpoint.map(u64::from),
            reader_lo,
            checkpoint_viewed_at,
        ) else {
            return Ok(Connection::new(false, false));
        };

        let tx_bounds = tx_bounds(ctx, &cp_bounds, global_tx_hi).await?;
        let tx_digest_keys = tx_unfiltered(&tx_bounds, &page);

        // Paginate the resulting tx_sequence_numbers and create cursor objects for pagination.
        let (prev, next, results) = page.paginate_results(tx_digest_keys, |&t| JsonCursor::new(t));

        let results: Vec<_> = results.collect();
        let tx_digest_keys: Vec<TxDigestKey> =
            results.iter().map(|(_, sq)| TxDigestKey(*sq)).collect();

        // Load the transaction digests for the paginated tx_sequence_numbers
        let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;
        let digest_map = pg_loader
            .load_many(tx_digest_keys)
            .await
            .context("Failed to load transaction digests")?;

        // Convert the transaction digests to Transaction objects
        for (cursor, tx_sequence_number) in results {
            let key = TxDigestKey(tx_sequence_number);
            if let Some(stored) = digest_map.get(&key) {
                let transaction_digest = TransactionDigest::try_from(stored.tx_digest.clone())
                    .context("Failed to deserialize transaction digest")?;
                let transaction = Self::with_id(scope.clone(), transaction_digest);
                conn.edges
                    .push(Edge::new(cursor.encode_cursor(), transaction));
            }
        }

        conn.has_previous_page = prev;
        conn.has_next_page = next;

        Ok(conn)
    }
}

/// The tx_sequence_numbers with cursors applied inclusively.
/// Results are limited to `page.limit() + 2` to allow has_previous_page and has_next_page calculations.
fn tx_unfiltered(tx_bounds: &Range<u64>, page: &Page<CTransaction>) -> Vec<u64> {
    // Inclusive cursor bounds
    let pg_lo = page
        .after()
        .map_or(tx_bounds.start, |cursor| cursor.max(tx_bounds.start));
    let pg_hi = page
        .before()
        .map(|cursor: &JsonCursor<u64>| cursor.saturating_add(1))
        .map_or(tx_bounds.end, |cursor| cursor.min(tx_bounds.end));

    if page.is_from_front() {
        (pg_lo..pg_hi).take(page.limit_with_overhead()).collect()
    } else {
        // Graphql last syntax expects results to be in ascending order. If we are paginating backwards,
        // we reverse the results after applying limits.
        let mut results: Vec<_> = (pg_lo..pg_hi)
            .rev()
            .take(page.limit_with_overhead())
            .collect();
        results.reverse();
        results
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
        let Some(checkpoint_viewed_at) = self.scope.checkpoint_viewed_at() else {
            return Ok(self.clone());
        };

        let kv_loader: &KvLoader = ctx.data()?;
        let Some(transaction) = kv_loader
            .load_one_transaction(digest)
            .await
            .context("Failed to fetch transaction contents")?
        else {
            return Ok(self.clone());
        };

        // Discard the loaded result if we are viewing it at a checkpoint before it existed.
        let cp_num = transaction
            .cp_sequence_number()
            .context("Any transaction fetched from the DB should have a checkpoint set")?;
        if cp_num > checkpoint_viewed_at {
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
