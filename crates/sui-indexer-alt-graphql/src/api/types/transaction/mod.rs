// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{ops::Deref, ops::Range, sync::Arc};

use anyhow::Context as _;
use async_graphql::{
    connection::{Connection, CursorType, Edge},
    dataloader::DataLoader,
    Context, Object,
};
use diesel::{sql_types::BigInt, QueryableByName};
use fastcrypto::encoding::{Base58, Encoding};
use sui_indexer_alt_reader::{
    kv_loader::{KvLoader, TransactionContents as NativeTransactionContents},
    pg_reader::PgReader,
    tx_digests::TxDigestKey,
};
use sui_pg_db::query::Query;
use sui_sql_macro::query;
use sui_types::{
    base_types::SuiAddress as NativeSuiAddress,
    digests::TransactionDigest,
    transaction::{TransactionDataAPI, TransactionExpiration},
};

use super::{
    address::Address,
    checkpoint::filter::checkpoint_bounds,
    epoch::Epoch,
    gas_input::GasInput,
    transaction::filter::TransactionFilter,
    transaction_effects::{EffectsContents, TransactionEffects},
    transaction_kind::TransactionKind,
    user_signature::UserSignature,
};

use crate::{
    api::{
        scalars::{base64::Base64, cursor::JsonCursor, digest::Digest, sui_address::SuiAddress},
        types::{lookups::tx_bounds, transaction::filter::TransactionKindInput},
    },
    error::RpcError,
    pagination::Page,
    scope::Scope,
    task::watermark::Watermarks,
};

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
        TransactionFilter {
            after_checkpoint,
            at_checkpoint,
            before_checkpoint,
            kind,
            affected_address,
            sent_address,
        }: TransactionFilter,
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
            after_checkpoint.map(u64::from),
            at_checkpoint.map(u64::from),
            before_checkpoint.map(u64::from),
            reader_lo,
            checkpoint_viewed_at,
        ) else {
            return Ok(Connection::new(false, false));
        };

        let tx_bounds = tx_bounds(ctx, &cp_bounds, global_tx_hi, &page, |c| *c.deref()).await?;

        let tx_digest_keys = if let Some(kind) = kind {
            tx_kind(ctx, tx_bounds, &page, kind, sent_address).await?
        } else if affected_address.is_some() || sent_address.is_some() {
            tx_affected_address(ctx, tx_bounds, &page, affected_address, sent_address).await?
        } else {
            tx_unfiltered(tx_bounds, &page)
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

async fn tx_kind(
    ctx: &Context<'_>,
    tx_bounds: Range<u64>,
    page: &Page<CTransaction>,
    kind: TransactionKindInput,
    sent_address: Option<SuiAddress>,
) -> Result<Vec<u64>, RpcError> {
    match (kind, sent_address) {
        // We can simplify the query to just the `tx_affected_addresses` table if ProgrammableTX
        // and sender are specified.
        (TransactionKindInput::ProgrammableTx, Some(_)) => {
            tx_affected_address(ctx, tx_bounds, page, None, sent_address).await
        }
        (TransactionKindInput::SystemTx, Some(_)) => Ok(vec![]),
        // Otherwise, we can ignore the sender always, and just query the `tx_kinds` table.
        (_, None) => {
            let query = query!(
                r#"
    SELECT
        tx_sequence_number
    FROM
        tx_kinds
    WHERE
        tx_kind = {BigInt} /* kind */
    "#,
                kind as i64,
            );
            tx_sequence_numbers(ctx, tx_bounds, page, query).await
        }
    }
}

async fn tx_affected_address(
    ctx: &Context<'_>,
    tx_bounds: Range<u64>,
    page: &Page<CTransaction>,
    affected_address: Option<SuiAddress>,
    sent_address: Option<SuiAddress>,
) -> Result<Vec<u64>, RpcError> {
    // Use sent_address as affected_address if affected_address is not set to use PG index.
    let affected_address = affected_address.or(sent_address).unwrap();
    let mut query = query!(
        r#"
SELECT
    tx_sequence_number
FROM
    tx_affected_addresses
WHERE
    affected = {Bytea} /* affected_address */
"#,
        affected_address.into_vec(),
    );
    if let Some(sent_address) = sent_address {
        query += query!(
            r#"
AND sender = {Bytea} /* sent_address */
"#,
            sent_address.into_vec()
        );
    }
    tx_sequence_numbers(ctx, tx_bounds, page, query).await
}

async fn tx_sequence_numbers(
    ctx: &Context<'_>,
    Range {
        start: tx_lo,
        end: tx_hi,
    }: Range<u64>,
    page: &Page<CTransaction>,
    mut query: Query<'_>,
) -> Result<Vec<u64>, RpcError> {
    query += query!(
        r#"
    AND {BigInt} <= tx_sequence_number /* tx_lo */
    AND tx_sequence_number < {BigInt} /* tx_hi */
ORDER BY
    tx_sequence_number {} /* order_by_direction */
LIMIT
    {BigInt} /* limit_with_overhead */
"#,
        tx_lo as i64,
        tx_hi as i64,
        page.order_by_direction(),
        page.limit_with_overhead() as i64,
    );

    let pg_reader: &PgReader = ctx.data()?;

    #[derive(QueryableByName)]
    struct TxSequenceNumber {
        #[diesel(sql_type = BigInt)]
        tx_sequence_number: i64,
    }

    let mut conn = pg_reader
        .connect()
        .await
        .context("Failed to connect to database")?;

    let wrapped_tx_sequence_numbers: Vec<TxSequenceNumber> = conn
        .results(query)
        .await
        .context("Failed to execute query")?;

    let tx_sequence_numbers = if page.is_from_front() {
        wrapped_tx_sequence_numbers
            .iter()
            .map(|t| t.tx_sequence_number as u64)
            .collect()
    } else {
        wrapped_tx_sequence_numbers
            .iter()
            .rev()
            .map(|t| t.tx_sequence_number as u64)
            .collect()
    };

    Ok(tx_sequence_numbers)
}

/// The tx_sequence_numbers with cursors applied inclusively.
/// Results are limited to `page.limit() + 2` to allow has_previous_page and has_next_page calculations.
fn tx_unfiltered(tx_bounds: Range<u64>, page: &Page<CTransaction>) -> Vec<u64> {
    if page.is_from_front() {
        tx_bounds.take(page.limit_with_overhead()).collect()
    } else {
        // Graphql last syntax expects results to be in ascending order. If we are paginating backwards,
        // we reverse the results after applying limits.
        let mut results: Vec<_> = tx_bounds.rev().take(page.limit_with_overhead()).collect();
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
