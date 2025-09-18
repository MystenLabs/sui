// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{ops::Deref, sync::Arc};

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

use crate::{
    api::{
        scalars::{
            base64::Base64, cursor::JsonCursor, digest::Digest, fq_name_filter::FqNameFilter,
            module_filter::ModuleFilter, sui_address::SuiAddress,
        },
        types::{
            lookups::{CheckpointBounds, TxBoundsCursor},
            transaction::filter::TransactionKindInput,
        },
    },
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
    transaction_kind::TransactionKind,
    user_signature::UserSignature,
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
        filter: TransactionFilter,
    ) -> Result<Connection<String, Transaction>, RpcError> {
        let mut conn = Connection::new(false, false);

        let watermarks: &Arc<Watermarks> = ctx.data()?;

        let reader_lo = watermarks.pipeline_lo_watermark("tx_digests")?.checkpoint();

        let Some(query) = filter.tx_bounds(ctx, &scope, reader_lo, &page).await? else {
            return Ok(conn);
        };

        let TransactionFilter {
            after_checkpoint: _,
            at_checkpoint: _,
            before_checkpoint: _,
            function,
            kind,
            affected_address,
            affected_object,
            sent_address,
        } = filter;
        let tx_digest_keys = if let Some(function) = function {
            tx_call(ctx, query, &page, function, sent_address).await?
        } else if let Some(kind) = kind {
            tx_kind(ctx, query, &page, kind, sent_address).await?
        } else if affected_address.is_some() || sent_address.is_some() {
            tx_affected_address(ctx, query, &page, affected_address, sent_address).await?
        } else if let Some(affected_object) = affected_object {
            tx_affected_object(ctx, query, &page, affected_object, sent_address).await?
        } else {
            tx_unfiltered(ctx, query, &page).await?
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

impl TxBoundsCursor for CTransaction {
    fn tx_sequence_number(&self) -> u64 {
        *self.deref()
    }
}

async fn tx_call(
    ctx: &Context<'_>,
    mut query: Query<'_>,
    page: &Page<CTransaction>,
    function: FqNameFilter,
    sent_address: Option<SuiAddress>,
) -> Result<Vec<u64>, RpcError> {
    let (package, module, member) = match function {
        FqNameFilter::Module(module_filter) => match module_filter {
            ModuleFilter::Package(package) => (package, None, None),
            ModuleFilter::Module(package, module) => (package, Some(module), None),
        },
        FqNameFilter::FqName(package, module, member) => (package, Some(module), Some(member)),
    };

    query += query!(
        r#"
        SELECT
            tx_sequence_number
        FROM
            tx_calls
        WHERE
            package = {Bytea} /* package */
        "#,
        package.into_vec(),
    );

    if let Some(module) = module {
        query += query!(" AND module = {Text}", module);

        if let Some(member) = member {
            query += query!(" AND function = {Text}", member);
        }
    }
    if let Some(sent_address) = sent_address {
        query += query!(" AND sender = {Bytea}", sent_address.into_vec());
    }
    tx_sequence_numbers(ctx, query, page).await
}

async fn tx_kind(
    ctx: &Context<'_>,
    mut query: Query<'_>,
    page: &Page<CTransaction>,
    kind: TransactionKindInput,
    sent_address: Option<SuiAddress>,
) -> Result<Vec<u64>, RpcError> {
    match (kind, sent_address) {
        // We can simplify the query to just the `tx_affected_addresses` table if ProgrammableTX
        // and sender are specified.
        (TransactionKindInput::ProgrammableTx, Some(_)) => {
            tx_affected_address(ctx, query, page, None, sent_address).await
        }
        (TransactionKindInput::SystemTx, Some(_)) => Ok(vec![]),
        // Otherwise, we can ignore the sender always, and just query the `tx_kinds` table.
        (_, None) => {
            query += query!(
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
            tx_sequence_numbers(ctx, query, page).await
        }
    }
}

async fn tx_affected_address(
    ctx: &Context<'_>,
    mut query: Query<'_>,
    page: &Page<CTransaction>,
    affected_address: Option<SuiAddress>,
    sent_address: Option<SuiAddress>,
) -> Result<Vec<u64>, RpcError> {
    // Use sent_address as affected_address if affected_address is not set to use PG index.
    let affected_address = affected_address.or(sent_address).unwrap();
    query += query!(
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
        query += filter_sender(sent_address);
    }
    tx_sequence_numbers(ctx, query, page).await
}

async fn tx_affected_object(
    ctx: &Context<'_>,
    mut query: Query<'_>,
    page: &Page<CTransaction>,
    affected_object: SuiAddress,
    sent_address: Option<SuiAddress>,
) -> Result<Vec<u64>, RpcError> {
    query += query!(
        r#"
        SELECT
            tx_sequence_number
        FROM
            tx_affected_objects
        WHERE
            affected = {Bytea} /* affected_object */
        "#,
        affected_object.into_vec(),
    );
    if let Some(sent_address) = sent_address {
        query += filter_sender(sent_address);
    }
    tx_sequence_numbers(ctx, query, page).await
}

fn filter_sender(sent_address: SuiAddress) -> Query<'static> {
    query!(" AND sender = {Bytea}", sent_address.into_vec())
}

async fn tx_sequence_numbers(
    ctx: &Context<'_>,
    mut query: Query<'_>,
    page: &Page<CTransaction>,
) -> Result<Vec<u64>, RpcError> {
    query += query!(
        r#"
            AND (SELECT tx_lo FROM tx_lo) <= tx_sequence_number
            AND tx_sequence_number < (SELECT tx_hi FROM tx_hi)
        ORDER BY
            tx_sequence_number {} /* order_by_direction */
        LIMIT
            {BigInt} /* limit_with_overhead */
        "#,
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
async fn tx_unfiltered(
    ctx: &Context<'_>,
    tx_bounds_query: Query<'_>,
    page: &Page<CTransaction>,
) -> Result<Vec<u64>, RpcError> {
    let query = tx_bounds_query
        + query!(
            r#"
            SELECT
                (SELECT tx_lo FROM tx_lo),
                (SELECT tx_hi FROM tx_hi)
            "#
        );

    let pg_reader: &PgReader = ctx.data()?;

    #[derive(QueryableByName)]
    struct TxBounds {
        #[diesel(sql_type = BigInt)]
        tx_hi: i64,

        #[diesel(sql_type = BigInt)]
        tx_lo: i64,
    }

    let mut conn = pg_reader
        .connect()
        .await
        .context("Failed to connect to database")?;

    let results: Vec<TxBounds> = conn
        .results(query)
        .await
        .context("Failed to execute query")?;

    let (mut tx_lo, mut tx_hi) = results
        .first()
        .context("No valid checkpoints found")
        .map(|bounds| (bounds.tx_lo as u64, bounds.tx_hi as u64))?;

    let limit = page.limit_with_overhead() as u64;
    if page.is_from_front() {
        tx_hi = tx_hi.min(tx_lo.saturating_add(limit));
    } else {
        tx_lo = tx_lo.max(tx_hi.saturating_sub(limit));
    }
    let tx_sequence_numbers = (tx_lo..tx_hi).collect();
    Ok(tx_sequence_numbers)
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
