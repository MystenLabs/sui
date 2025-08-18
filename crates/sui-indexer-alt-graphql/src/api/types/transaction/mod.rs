// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{ops::RangeInclusive, sync::Arc};

use anyhow::Context as _;
use async_graphql::{
    connection::{Connection, CursorType, Edge},
    dataloader::DataLoader,
    Context, Object,
};
use diesel::{prelude::QueryableByName, sql_types::BigInt};
use fastcrypto::encoding::{Base58, Encoding};
use sui_indexer_alt_reader::{
    kv_loader::{KvLoader, TransactionContents as NativeTransactionContents},
    pg_reader::PgReader,
    tx_digests::TxDigestKey,
};
use sui_sql_macro::query;
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
    epoch::Epoch,
    gas_input::GasInput,
    transaction::filter::TransactionFilter,
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

#[derive(QueryableByName)]
struct TxBounds {
    #[diesel(sql_type = BigInt, column_name = "tx_lo")]
    tx_lo: i64,
    #[diesel(sql_type = BigInt, column_name = "tx_hi")]
    tx_hi: i64,
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

    /// Cursor based pagination through transactions based on filters.
    pub(crate) async fn paginate(
        ctx: &Context<'_>,
        scope: Scope,
        page: Page<CTransaction>,
        filter: TransactionFilter,
    ) -> Result<Connection<String, Transaction>, RpcError> {
        let mut conn = Connection::new(false, false);

        if page.limit() == 0 {
            return Ok(Connection::new(false, false));
        }

        let watermarks: &Arc<Watermarks> = ctx.data()?;

        let reader_lo = watermarks.pipeline_lo_watermark("tx_digests")?.checkpoint();

        let global_tx_hi = watermarks.high_watermark().transaction();

        let tx_digest_keys = if let Some(cp_bounds) =
            filter.checkpoint_bounds(reader_lo, scope.checkpoint_viewed_at())
        {
            tx_unfiltered(ctx, &cp_bounds, &page, global_tx_hi).await?
        } else {
            return Ok(Connection::new(false, false));
        };

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

/// The tx_sequence_numbers within checkpoint bounds and with cursors applied inclusively.
/// Results are limited to `page.limit() + 2` to allow has_previous_page and has_next_page calculations.
///
/// The checkpoint lower and upper bounds are used to determine the inclusive lower (tx_lo) and exclusive
/// upper (tx_hi) bounds of the sequence of tx_sequence_numbers to use in queries.
///
/// tx_lo: The cp_sequence_number of the checkpoint at the start of the bounds.
/// tx_hi: The tx_lo of the checkpoint directly after the cp_bounds.end(). If it does not exists,
///      at cp_bounds.end() fallback to the maximum tx_sequence_number in the context's watermark
///      (global_tx_hi).
///
/// NOTE: for consistency, assume that lowerbounds are inclusive and upperbounds are exclusive.
/// Bounds that do not follow this convention will be annotated explicitly (e.g. `lo_exclusive` or
/// `hi_inclusive`).
async fn tx_unfiltered(
    ctx: &Context<'_>,
    cp_bounds: &RangeInclusive<u64>,
    page: &Page<CTransaction>,
    global_tx_hi: u64,
) -> Result<Vec<u64>, RpcError> {
    let pg_reader: &PgReader = ctx.data()?;
    let query = query!(
        r#"
        WITH
        tx_lo AS (
            SELECT 
                tx_lo 
            FROM 
                cp_sequence_numbers 
            WHERE 
                cp_sequence_number = {BigInt}
            LIMIT 1
        ),

        -- tx_hi is the tx_lo of the checkpoint directly after the cp_bounds.end()
        tx_hi AS (
            SELECT 
                tx_lo AS tx_hi
            FROM 
                cp_sequence_numbers 
            WHERE 
                cp_sequence_number = {BigInt} + 1 
            LIMIT 1
        )

        SELECT
            (SELECT tx_lo FROM tx_lo) AS "tx_lo",
            -- If we cannot get the tx_hi from the checkpoint directly after the cp_bounds.end() we use global tx_hi.
            COALESCE((SELECT tx_hi FROM tx_hi), {BigInt}) AS "tx_hi";"#,
        *cp_bounds.start() as i64,
        *cp_bounds.end() as i64,
        global_tx_hi as i64
    );

    let mut conn = pg_reader
        .connect()
        .await
        .context("Failed to connect to database")?;

    let results: Vec<TxBounds> = conn
        .results(query)
        .await
        .context("Failed to execute query")?;

    let (tx_lo, tx_hi) = results
        .first()
        .context("No valid checkpoints found")
        .map(|bounds| (bounds.tx_lo as u64, bounds.tx_hi as u64))?;

    // Inclusive cursor bounds
    let pg_lo = page.after().map_or(tx_lo, |cursor| cursor.max(tx_lo));
    let pg_hi = page
        .before()
        .map(|cursor| cursor.saturating_add(1))
        .map_or(tx_hi, |cursor| cursor.min(tx_hi));

    const PAGINATION_OVERHEAD: usize = 2; // For has_previous_page and has_next_page calculations.

    Ok(if page.is_from_front() {
        (pg_lo..pg_hi)
            .take(page.limit() + PAGINATION_OVERHEAD)
            .collect()
    } else {
        // Graphql last syntax expects results to be in ascending order. If we are paginating backwards,
        // we reverse the results after applying limits.
        let mut results: Vec<_> = (pg_lo..pg_hi)
            .rev()
            .take(page.limit() + PAGINATION_OVERHEAD)
            .collect();
        results.reverse();
        results
    })
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
