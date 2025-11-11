// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::{ops::Deref, sync::Arc};

use anyhow::Context as _;
use async_graphql::{Context, Object, connection::Connection, dataloader::DataLoader};
use diesel::{QueryableByName, sql_types::BigInt};
use fastcrypto::encoding::{Base58, Encoding};
use futures::future::try_join_all;
use serde::{Deserialize, Serialize};

use sui_indexer_alt_reader::{
    checkpoints::CheckpointKey,
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
            sui_address::SuiAddress,
        },
        types::{
            available_range::AvailableRangeKey,
            checkpoint::filter::checkpoint_bounds,
            lookups::{CheckpointBounds, TxBoundsCursor},
            scan,
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

type DigestsByCheckpoint = std::collections::HashMap<CheckpointKey, Vec<TransactionDigest>>;
type TransactionsByDigest = std::collections::HashMap<TransactionDigest, NativeTransactionContents>;

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

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug, Copy)]
pub(crate) struct TransactionCursor {
    #[serde(rename = "t")]
    pub tx_sequence_number: u64,
    #[serde(rename = "c")]
    pub cp_sequence_number: u64,
}

impl PartialOrd for TransactionCursor {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TransactionCursor {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Order by checkpoint first, then by transaction index within checkpoint
        match self.cp_sequence_number.cmp(&other.cp_sequence_number) {
            std::cmp::Ordering::Equal => self.tx_sequence_number.cmp(&other.tx_sequence_number),
            other => other,
        }
    }
}

pub(crate) type SCTransaction = JsonCursor<TransactionCursor>;

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
    async fn kind(&self, ctx: &Context<'_>) -> Option<Result<TransactionKind, RpcError>> {
        async {
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
        .await
        .transpose()
    }

    #[graphql(flatten)]
    async fn contents(&self, ctx: &Context<'_>) -> Result<TransactionContents, RpcError> {
        self.contents.fetch(ctx, self.digest).await
    }
}

#[Object]
impl TransactionContents {
    /// This field is set by senders of a transaction block. It is an epoch reference that sets a deadline after which validators will no longer consider the transaction valid. By default, there is no deadline for when a transaction must execute.
    async fn expiration(&self) -> Option<Result<Epoch, RpcError>> {
        async {
            let Some(content) = &self.contents else {
                return Ok(None);
            };

            let transaction_data = content.data()?;
            match transaction_data.expiration() {
                TransactionExpiration::None => Ok(None),
                TransactionExpiration::Epoch(epoch_id) => {
                    Ok(Some(Epoch::with_id(self.scope.clone(), *epoch_id)))
                }
                TransactionExpiration::ValidDuring { max_epoch, .. } => {
                    if let Some(epoch_id) = max_epoch {
                        Ok(Some(Epoch::with_id(self.scope.clone(), *epoch_id)))
                    } else {
                        Ok(None)
                    }
                }
            }
        }
        .await
        .transpose()
    }

    /// The gas input field provides information on what objects were used as gas as well as the owner of the gas object(s) and information on the gas price and budget.
    async fn gas_input(&self) -> Option<Result<GasInput, RpcError>> {
        async {
            let Some(content) = &self.contents else {
                return Ok(None);
            };

            let transaction_data = content.data()?;
            Ok(Some(GasInput::from_gas_data(
                self.scope.clone(),
                transaction_data.gas_data().clone(),
            )))
        }
        .await
        .transpose()
    }

    /// The address corresponding to the public key that signed this transaction. System transactions do not have senders.
    async fn sender(&self) -> Option<Result<Address, RpcError>> {
        async {
            let Some(content) = &self.contents else {
                return Ok(None);
            };

            let sender = content.data()?.sender();
            Ok((sender != NativeSuiAddress::ZERO)
                .then(|| Address::with_address(self.scope.clone(), sender)))
        }
        .await
        .transpose()
    }

    /// The Base64-encoded BCS serialization of this transaction, as a `TransactionData`.
    async fn transaction_bcs(&self) -> Option<Result<Base64, RpcError>> {
        async {
            let Some(content) = &self.contents else {
                return Ok(None);
            };

            Ok(Some(Base64(content.raw_transaction()?)))
        }
        .await
        .transpose()
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
        let watermarks: &Arc<Watermarks> = ctx.data()?;
        let available_range_key = AvailableRangeKey {
            type_: "Query".to_string(),
            field: Some("transactions".to_string()),
            filters: Some(filter.active_filters()),
        };
        let reader_lo = available_range_key.reader_lo(watermarks)?;

        let Some(query) = filter.tx_bounds(ctx, &scope, reader_lo, &page).await? else {
            return Ok(Connection::new(false, false));
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

        let tx_sequence_numbers = if let Some(function) = function {
            tx_call(ctx, query, &page, function, sent_address).await?
        } else if let Some(kind) = kind {
            tx_kind(ctx, query, &page, kind, sent_address).await?
        } else if let Some(affected_object) = affected_object {
            tx_affected_object(ctx, query, &page, affected_object, sent_address).await?
        } else if let Some(address) = affected_address {
            tx_affected_address(ctx, query, &page, address, sent_address).await?
        } else if let Some(address) = sent_address {
            tx_affected_address(ctx, query, &page, address, sent_address).await?
        } else {
            tx_unfiltered(ctx, query, &page).await?
        };

        page.paginate_results(
            tx_digests(ctx, &tx_sequence_numbers).await?,
            |(s, _)| JsonCursor::new(*s),
            |(_, d)| Ok(Self::with_id(scope.clone(), d)),
        )
    }

    /// Scan through checkpoints using two-stage bloom filtering to find transactions that match the filters.
    ///
    /// 1. **Checkpoint bounds calculation**: Determines the range to scan based on filter
    /// 2. **Stage 1 (Blocked blooms)**: Filters millions of checkpoints to ~hundreds of candidates
    /// 3. **Stage 2 (Per-checkpoint blooms)**: Refines candidates to eliminate false positives
    /// 4. **Transaction loading**: Loads only transactions from candidate checkpoints
    /// 5. **Final filtering**: Applies exact filter match to eliminate bloom FPs
    pub(crate) async fn scan(
        ctx: &Context<'_>,
        scope: Scope,
        page: Page<SCTransaction>,
        filter: TransactionFilter,
    ) -> Result<Connection<String, Transaction>, RpcError> {
        let watermarks: &Arc<Watermarks> = ctx.data()?;
        let available_range_key = AvailableRangeKey {
            type_: "Query".to_string(),
            field: Some("transactions".to_string()),
            filters: Some(filter.active_filters()),
        };
        let reader_lo = available_range_key.reader_lo(watermarks)?;

        let Some(checkpoint_viewed_at) = scope.checkpoint_viewed_at() else {
            return Ok(Connection::new(false, false));
        };

        let Some(cp_bounds) = checkpoint_bounds(
            filter.after_checkpoint().map(u64::from),
            filter.at_checkpoint().map(u64::from),
            filter.before_checkpoint().map(u64::from),
            reader_lo,
            checkpoint_viewed_at,
        ) else {
            return Ok(Connection::new(false, false));
        };

        let cp_lo = page.after().map_or(*cp_bounds.start(), |a| {
            (*cp_bounds.start()).max(a.cp_sequence_number)
        });
        let cp_hi = page.before().map_or(*cp_bounds.end(), |b| {
            (*cp_bounds.end()).min(b.cp_sequence_number)
        });

        // Check if the range is still valid after applying cursor bounds
        if cp_lo > cp_hi {
            return Ok(Connection::new(false, false));
        }

        let filter_keys = filter.filter_keys();

        let blocked_candidates =
            scan::query_blocked_blooms(ctx, &filter_keys, cp_lo, cp_hi, &page).await?;

        let candidate_cps =
            scan::candidate_cp_blooms(ctx, &filter_keys, &blocked_candidates, &page).await?;

        // Load transaction data
        let (digests, native_transactions) = load_transaction_data(ctx, &candidate_cps).await?;

        // Apply final filter and build results
        let results = filter_and_build_results(
            candidate_cps,
            &digests,
            &native_transactions,
            &filter,
            &scope,
            &page,
        )
        .await?;

        page.paginate_results(results, |(s, _)| JsonCursor::new(*s), |(_, tx)| Ok(tx))
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

impl TxBoundsCursor for CTransaction {
    fn tx_sequence_number(&self) -> u64 {
        *self.deref()
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

pub(crate) async fn tx_digests(
    ctx: &Context<'_>,
    tx_sequence_numbers: &[u64],
) -> Result<Vec<(u64, TransactionDigest)>, RpcError> {
    let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;

    try_join_all(tx_sequence_numbers.iter().map(|&tx| async move {
        let stored = pg_loader
            .load_one(TxDigestKey(tx))
            .await
            .context("Failed to load transaction digest")?
            .context("Failed to find transaction digest")?;

        let digest = TransactionDigest::try_from(stored.tx_digest)
            .context("Failed to deserialize transaction digest")?;

        Ok((tx, digest))
    }))
    .await
}

async fn tx_affected_address(
    ctx: &Context<'_>,
    mut query: Query<'_>,
    page: &Page<CTransaction>,
    affected_address: SuiAddress,
    sent_address: Option<SuiAddress>,
) -> Result<Vec<u64>, RpcError> {
    query += query!(
        r#"
        SELECT
            tx_sequence_number
        FROM
            tx_affected_addresses
        WHERE
            affected = {Bytea}
        "#,
        affected_address.into_vec(),
    );

    if let Some(address) = sent_address {
        query += query!(" AND sender = {Bytea}", address.into_vec());
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
            affected = {Bytea}
        "#,
        affected_object.into_vec(),
    );

    if let Some(address) = sent_address {
        query += query!(" AND sender = {Bytea}", address.into_vec());
    }

    tx_sequence_numbers(ctx, query, page).await
}

async fn tx_call(
    ctx: &Context<'_>,
    mut query: Query<'_>,
    page: &Page<CTransaction>,
    function: FqNameFilter,
    sent_address: Option<SuiAddress>,
) -> Result<Vec<u64>, RpcError> {
    query += query!(
        r#"
        SELECT
            tx_sequence_number
        FROM
            tx_calls
        WHERE
            package = {Bytea}
        "#,
        function.package().into_vec(),
    );

    if let Some(module) = function.module() {
        query += query!(" AND module = {Text}", module.to_string());
    }

    if let Some(name) = function.name() {
        query += query!(" AND function = {Text}", name.to_string());
    }

    if let Some(address) = sent_address {
        query += query!(" AND sender = {Bytea}", address.into_vec());
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
        (TransactionKindInput::ProgrammableTx, Some(address)) => {
            tx_affected_address(ctx, query, page, address, Some(address)).await
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
                    tx_kind = {BigInt}
                "#,
                kind as i64,
            );

            tx_sequence_numbers(ctx, query, page).await
        }
    }
}

async fn tx_sequence_numbers(
    ctx: &Context<'_>,
    mut query: Query<'_>,
    page: &Page<CTransaction>,
) -> Result<Vec<u64>, RpcError> {
    let pg_reader: &PgReader = ctx.data()?;

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
    mut query: Query<'_>,
    page: &Page<CTransaction>,
) -> Result<Vec<u64>, RpcError> {
    let pg_reader: &PgReader = ctx.data()?;

    query += query!(
        r#"
        SELECT
            (SELECT tx_lo FROM tx_lo),
            (SELECT tx_hi FROM tx_hi)
        "#
    );

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

/// Loads transaction digests and full transaction data for candidate checkpoints.
async fn load_transaction_data(
    ctx: &Context<'_>,
    candidate_cps: &[u64],
) -> Result<(DigestsByCheckpoint, TransactionsByDigest), RpcError> {
    let kv_loader: &KvLoader = ctx.data()?;

    let digests = kv_loader
        .load_many_checkpoints_transactions(candidate_cps.to_vec())
        .await
        .context("Failed to load checkpoint transactions")?;

    let tx_digests_to_load: Vec<_> = digests.values().flatten().copied().collect();
    let native_transactions = kv_loader
        .load_many_transactions(tx_digests_to_load)
        .await
        .context("Failed to load transactions")?;

    Ok((digests, native_transactions))
}

/// Applies final filter and builds result list with cursors.
async fn filter_and_build_results(
    candidate_cps: Vec<u64>,
    digests: &DigestsByCheckpoint,
    native_transactions: &TransactionsByDigest,
    filter: &TransactionFilter,
    scope: &Scope,
    page: &Page<SCTransaction>,
) -> Result<Vec<(TransactionCursor, Transaction)>, RpcError> {
    let mut results: Vec<(TransactionCursor, Transaction)> = Vec::new();

    let after_cursor = page.after().map(|c| *c.deref());
    let before_cursor = page.before().map(|c| *c.deref());

    for cp_sequence_number in candidate_cps {
        let checkpoint_digests = digests
            .get(&CheckpointKey(cp_sequence_number))
            .context("Failed to load checkpoint transaction digests")?;

        // Create enumerated iterator with appropriate direction
        let tx_indices: Vec<usize> = if page.is_from_front() {
            (0..checkpoint_digests.len()).collect()
        } else {
            (0..checkpoint_digests.len()).rev().collect()
        };

        for idx in tx_indices {
            let digest = &checkpoint_digests[idx];

            let cursor = TransactionCursor {
                tx_sequence_number: idx as u64,
                cp_sequence_number,
            };

            // Apply cursor bounds - skip transactions outside the cursor window
            if after_cursor.is_some_and(|after| cursor <= after) {
                continue;
            }
            if before_cursor.is_some_and(|before| cursor >= before) {
                continue;
            }

            let native_transaction = native_transactions
                .get(digest)
                .context("Failed to load transaction")?;

            if filter.matches(native_transaction) {
                results.push((
                    cursor,
                    Transaction {
                        digest: *digest,
                        contents: TransactionContents {
                            scope: scope.clone(),
                            contents: Some(Arc::new(native_transaction.clone())),
                        },
                    },
                ));
            }

            if results.len() >= page.limit_with_overhead() {
                break;
            }
        }

        if results.len() >= page.limit_with_overhead() {
            break;
        }
    }

    // For backward pagination, reverse results to maintain ascending order
    // (paginate_results expects results in ascending cursor order)
    if !page.is_from_front() {
        results.reverse();
    }

    Ok(results)
}
