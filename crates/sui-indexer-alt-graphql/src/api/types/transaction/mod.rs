// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::Context;
use async_graphql::Object;
use async_graphql::connection::Connection;
use async_graphql::connection::CursorType;
use async_graphql::connection::Edge;
use async_graphql::connection::EmptyFields;
use async_graphql::connection::PageInfo;
use async_graphql::dataloader::DataLoader;
use diesel::QueryableByName;
use diesel::sql_types::BigInt;
use fastcrypto::encoding::Base58;
use fastcrypto::encoding::Encoding;
use futures::future::try_join_all;
use prost_types::FieldMask;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::AlphaLedgerGrpcReader;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::StreamPage;
use sui_indexer_alt_reader::kv_loader::KvLoader;
use sui_indexer_alt_reader::kv_loader::TransactionContents as NativeTransactionContents;
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_indexer_alt_reader::tx_digests::TxDigestKey;
use sui_pg_db::query::Query;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2;
use sui_rpc::proto::sui::rpc::v2alpha;
use sui_rpc_cursor::CursorToken;
use sui_rpc_cursor::QueryType;
use sui_sql_macro::query;
use sui_types::base_types::SuiAddress as NativeSuiAddress;
use sui_types::digests::TransactionDigest;
use sui_types::transaction::TransactionDataAPI;
use sui_types::transaction::TransactionExpiration;

use crate::api::scalars::base64::Base64;
use crate::api::scalars::cursor::ByteCursor;
use crate::api::scalars::digest::Digest;
use crate::api::scalars::fq_name_filter::FqNameFilter;
use crate::api::scalars::id::Id;
use crate::api::scalars::json::Json;
use crate::api::scalars::sui_address::SuiAddress;
use crate::api::types::address::Address;
use crate::api::types::available_range::AvailableRangeKey;
use crate::api::types::checkpoint::filter::checkpoint_bounds;
use crate::api::types::epoch::Epoch;
use crate::api::types::gas_input::GasInput;
use crate::api::types::lookups::CheckpointBounds;
use crate::api::types::lookups::TxBoundsCursor;
use crate::api::types::transaction::filter::TransactionFilter;
use crate::api::types::transaction::filter::TransactionKindInput;
use crate::api::types::transaction_effects::EffectsContents;
use crate::api::types::transaction_effects::TransactionEffects;
use crate::api::types::transaction_kind::TransactionKind;
use crate::api::types::user_signature::UserSignature;
use crate::error::RpcError;
use crate::error::bad_user_input;
use crate::error::upcast;
use crate::extensions::query_limits;
use crate::pagination::Page;
use crate::scope::Scope;
use crate::task::streaming::ProcessedTransaction;
use crate::task::watermark::Watermarks;

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

pub type CTransaction = ByteCursor;

/// Custom `Connection` for transactions to support partially-filled pages.
pub(crate) struct TransactionConnection {
    pub edges: Vec<Edge<String, Transaction, EmptyFields>>,
    pub page_info: PageInfo,
}

#[derive(thiserror::Error, Debug, Clone)]
pub(crate) enum Error {
    #[error("Invalid input cursor")]
    BadCursor,
}

/// Description of a transaction, the unit of activity on Sui.
#[Object]
impl Transaction {
    /// The transaction's globally unique identifier, which can be passed to `Query.node` to refetch it.
    pub(crate) async fn id(&self) -> Id {
        Id::Transaction(self.digest)
    }

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
impl TransactionConnection {
    /// Information to aid in pagination.
    async fn page_info(&self) -> &PageInfo {
        &self.page_info
    }

    /// A list of edges.
    async fn edges(&self) -> &[Edge<String, Transaction, EmptyFields>] {
        &self.edges
    }

    /// A list of nodes.
    async fn nodes(&self) -> Vec<&Transaction> {
        self.edges.iter().map(|e| &e.node).collect()
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

    /// The transaction as a JSON blob, matching the gRPC proto format (excluding BCS).
    async fn transaction_json(&self) -> Option<Result<Json, RpcError>> {
        async {
            let Some(content) = &self.contents else {
                return Ok(None);
            };

            let mut proto_transaction = content.proto_transaction()?;
            // Clear the bcs field as transactionJson is intended to provide a full structured output
            proto_transaction.bcs = None;
            let json_value = serde_json::to_value(&proto_transaction)
                .context("Failed to serialize transaction to JSON")?;
            Ok(Some(json_value.try_into()?))
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
    pub(crate) fn with_digest(scope: Scope, digest: TransactionDigest) -> Self {
        Self {
            digest,
            contents: TransactionContents::empty(scope.with_active_transaction_digest(digest)),
        }
    }

    /// Construct a fully-inflated transaction with already-hydrated contents. The digest is
    /// read from `contents`, which keeps it consistent with the contents anchored on the scope.
    pub(crate) fn with_contents(
        scope: Scope,
        contents: Arc<NativeTransactionContents>,
    ) -> Result<Self, RpcError> {
        let digest = contents.digest()?;
        Ok(Self {
            digest,
            contents: TransactionContents {
                scope: scope.with_active_transaction_contents(digest, contents.clone()),
                contents: Some(contents),
            },
        })
    }

    /// Paginate over pre-loaded transactions, applying in-memory filtering.
    ///
    /// Used when transaction data is already available (e.g. from streaming) and doesn't
    /// require database queries. Cursors encode `tx_sequence_number` for consistency with
    /// the query API, enabling clients to continue paginating via queries.
    ///
    // TODO(DVX-2068): Add cursor consistency test between subscriptions and query API.
    pub(crate) fn paginate_preloaded_transactions(
        scope: Scope,
        transactions: &[ProcessedTransaction],
        page: &Page<CTransaction>,
        filter: TransactionFilter,
    ) -> Result<TransactionConnection, RpcError<Error>> {
        let after = page
            .after()
            .map(|c| CursorToken::decode(c))
            .transpose()
            .map_err(|_| bad_user_input(Error::BadCursor))?
            .map(|c| c.position);
        let before = page
            .before()
            .map(|c| CursorToken::decode(c))
            .transpose()
            .map_err(|_| bad_user_input(Error::BadCursor))?
            .map(|c| c.position);

        let filtered: Vec<_> = transactions
            .iter()
            .filter(|tx| filter.matches(&tx.contents))
            .filter(|tx| after.is_none_or(|a| tx.tx_sequence_number >= a))
            .filter(|tx| before.is_none_or(|b| tx.tx_sequence_number <= b))
            .take(page.limit_with_overhead())
            .collect();

        page.paginate_results(
            filtered,
            |tx| {
                ByteCursor::new(
                    CursorToken::item(QueryType::Transactions, 0, tx.tx_sequence_number)
                        .encode()
                        .to_vec(),
                )
            },
            |tx| Transaction::with_contents(scope.clone(), tx.contents.clone()),
        )
        .map(Into::into)
        .map_err(upcast)
    }

    /// Load the transaction from the store, and return it fully inflated (with contents already
    /// fetched). Returns `None` if the transaction does not exist (either never existed or was
    /// pruned from the store).
    pub(crate) async fn fetch(
        ctx: &Context<'_>,
        scope: Scope,
        digest: Digest,
    ) -> Result<Option<Self>, RpcError> {
        let fetched = TransactionContents::empty(scope.clone())
            .fetch(ctx, digest.into())
            .await?;

        let Some(contents) = fetched.contents else {
            return Ok(None);
        };

        Ok(Some(Self::with_contents(scope, contents)?))
    }

    /// Cursor based pagination through transactions with filters applied.
    ///
    /// Returns empty results when no checkpoint is set in scope (e.g. execution scope).
    pub(crate) async fn paginate(
        ctx: &Context<'_>,
        scope: Scope,
        page: Page<CTransaction>,
        filter: TransactionFilter,
    ) -> Result<TransactionConnection, RpcError> {
        if let Some(reader) = ctx.data_opt::<AlphaLedgerGrpcReader>() {
            return Self::paginate_bitmap(reader, scope, page, filter).await;
        }

        let watermarks: &Arc<Watermarks> = ctx.data()?;
        let available_range_key = AvailableRangeKey {
            type_: "Query".to_string(),
            field: Some("transactions".to_string()),
            filters: Some(filter.active_filters()),
        };
        let reader_lo = available_range_key.reader_lo(watermarks)?;

        let Some(query) = filter.tx_bounds(ctx, &scope, reader_lo, &page).await? else {
            return Ok(TransactionConnection::empty());
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
            |(s, _)| {
                ByteCursor::new(
                    CursorToken::item(QueryType::Transactions, 0, *s)
                        .encode()
                        .to_vec(),
                )
            },
            |(_, d)| Ok(Self::with_digest(scope.clone(), d)),
        )
        .map(Into::into)
    }

    /// Serve transaction pagination by streaming the roaring-bitmap index. Returns pages that may
    /// be partially filled, with valid cursors if there are more pages to paginate through.
    async fn paginate_bitmap(
        reader: &AlphaLedgerGrpcReader,
        scope: Scope,
        page: Page<CTransaction>,
        filter: TransactionFilter,
    ) -> Result<TransactionConnection, RpcError> {
        if page.limit() == 0 {
            return Ok(Connection::new(false, false).into());
        }

        // Consistency upper bound; empty when scope has no checkpoint set.
        let Some(checkpoint_viewed_at) = scope.checkpoint_viewed_at() else {
            return Ok(Connection::new(false, false).into());
        };

        // TODO: LedgerService expose available checkpoint range for `reader_lo`.
        let reader_lo = 0;

        let Some(cp_bounds) = checkpoint_bounds(
            filter.after_checkpoint().map(u64::from),
            filter.at_checkpoint().map(u64::from),
            filter.before_checkpoint().map(u64::from),
            reader_lo,
            checkpoint_viewed_at,
        ) else {
            return Ok(Connection::new(false, false).into());
        };

        // Cursors are opaque pass-through bytes: the server minted them (an encoded gRPC
        // `CursorToken`), so we hand them straight back as the scan bounds.
        let after = page.after().map(|c| c.to_vec());
        let before = page.before().map(|c| c.to_vec());

        let mut options = v2alpha::QueryOptions::default();
        options.limit_items = Some(page.limit() as u32);
        options.after = after.map(|cursor| cursor.into());
        options.before = before.map(|cursor| cursor.into());
        options.ordering = if page.is_from_front() {
            v2alpha::Ordering::Ascending as i32
        } else {
            v2alpha::Ordering::Descending as i32
        };

        let mut request = v2alpha::ListTransactionsRequest::default();
        // Digest only — contents hydrate lazily via `KvLoader` on field access.
        request.read_mask = Some(FieldMask::from_paths(["digest"]));
        request.start_checkpoint = Some(*cp_bounds.start());
        // `cp_bounds` end is inclusive; the request bound is exclusive.
        request.end_checkpoint = Some(cp_bounds.end().saturating_add(1));
        request.filter = filter.to_bitmap_filter();
        request.options = Some(options);

        let result = reader
            .list_transactions(request)
            .await
            .context("Failed to list transactions")?;

        build_bitmap_connection(scope, &page, result)
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

        // Reuse contents anchored on the scope by a parent resolver (streaming and indexed
        // alike both anchor with hydrated contents when they have them).
        if let Some(contents) = self.scope.active_transaction_contents_for(digest) {
            return Ok(Self {
                scope: self.scope.clone(),
                contents: Some(contents.clone()),
            });
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

impl TransactionConnection {
    fn empty() -> Self {
        Self {
            edges: vec![],
            page_info: PageInfo {
                has_previous_page: false,
                has_next_page: false,
                start_cursor: None,
                end_cursor: None,
            },
        }
    }
}

impl TxBoundsCursor for CTransaction {
    fn tx_sequence_number(&self) -> u64 {
        CursorToken::decode(self)
            .expect("cursor already validated as ByteCursor")
            .position
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

impl From<Connection<String, Transaction>> for TransactionConnection {
    /// Convert a stock async-graphql `Connection` (as produced by the PG path's
    /// `Page::paginate_results`) into the custom shape. Cursors are derived from edges, matching
    /// stock semantics.
    fn from(conn: Connection<String, Transaction>) -> Self {
        let start_cursor = conn.edges.first().map(|e| e.cursor.clone());
        let end_cursor = conn.edges.last().map(|e| e.cursor.clone());
        Self {
            edges: conn.edges,
            page_info: PageInfo {
                has_previous_page: conn.has_previous_page,
                has_next_page: conn.has_next_page,
                start_cursor,
                end_cursor,
            },
        }
    }
}

/// Build a `TransactionConnection` from draining a bitmap-scan page.
///
/// Edges are returned in ascending order.
fn build_bitmap_connection(
    scope: Scope,
    page: &Page<CTransaction>,
    result: StreamPage<v2::ExecutedTransaction>,
) -> Result<TransactionConnection, RpcError> {
    let more = result.has_more();
    let start = result.first_cursor().cloned();
    let end = result.last_cursor().cloned();

    let mut items = result.items;
    if !page.is_from_front() {
        items.reverse();
    }

    let mut edges = Vec::with_capacity(items.len());
    for item in items {
        let digest = item
            .payload
            .digest
            .as_deref()
            .context("ListTransactions item missing transaction digest")?
            .parse::<TransactionDigest>()
            .context("Failed to parse transaction digest from ListTransactions")?;

        // The item's cursor is already an encoded `CursorToken`; hand it back opaquely.
        let cursor = ByteCursor::new(item.cursor.to_vec()).encode_cursor();

        edges.push(Edge::new(
            cursor,
            Transaction::with_digest(scope.clone(), digest),
        ));
    }

    let (has_previous_page, has_next_page) = if page.is_from_front() {
        (page.after().is_some(), more)
    } else {
        // A descending (`last`) scan walks high -> low, so "more" means earlier items remain
        // before the page.
        (more, page.before().is_some())
    };

    // Presented ascending: a forward scan keeps scan order, a descending scan swaps.
    let (start_bytes, end_bytes) = if page.is_from_front() {
        (start, end)
    } else {
        (end, start)
    };

    Ok(TransactionConnection {
        edges,
        page_info: PageInfo {
            has_previous_page,
            has_next_page,
            start_cursor: start_bytes.map(|b| ByteCursor::new(b.to_vec()).encode_cursor()),
            end_cursor: end_bytes.map(|b| ByteCursor::new(b.to_vec()).encode_cursor()),
        },
    })
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
    query_limits::rich::debit(ctx)?;
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
    query_limits::rich::debit(ctx)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pagination::PageLimits;
    use bytes::Bytes;
    use sui_indexer_alt_reader::alpha_ledger_grpc_reader::PageItem;

    /// 32-byte zero digest, base58-encoded. Round-trips through `TransactionDigest::parse` so
    /// `build_bitmap_connection` can convert items back into edges in tests.
    fn zero_digest_b58() -> String {
        Base58::encode(TransactionDigest::default().inner())
    }

    /// Build a synthetic `PageItem` whose payload digest is the zero digest and whose resume
    /// cursor is the provided bytes.
    fn tx_item(cursor: &[u8]) -> PageItem<v2::ExecutedTransaction> {
        let mut payload = v2::ExecutedTransaction::default();
        payload.digest = Some(zero_digest_b58());
        PageItem {
            payload,
            cursor: Bytes::copy_from_slice(cursor),
        }
    }

    /// Build a `Page<CTransaction>` going forwards (`first: N`, no `after`/`before`).
    fn forward_page(limit: u64) -> Page<CTransaction> {
        let limits = PageLimits {
            default: limit as u32,
            max: limit as u32,
        };
        Page::from_params(&limits, Some(limit), None, None, None)
            .expect("constructing forward Page<CTransaction>")
    }

    /// Build a `Page<CTransaction>` going backwards (`last: N`, no `after`/`before`).
    fn backward_page(limit: u64) -> Page<CTransaction> {
        let limits = PageLimits {
            default: limit as u32,
            max: limit as u32,
        };
        Page::from_params(&limits, None, None, Some(limit), None)
            .expect("constructing backward Page<CTransaction>")
    }

    /// Forward page opened from an `after` cursor (`first: N, after: <cursor>`).
    fn forward_page_after(limit: u64, after: &[u8]) -> Page<CTransaction> {
        let limits = PageLimits {
            default: limit as u32,
            max: limit as u32,
        };
        Page::from_params(
            &limits,
            Some(limit),
            Some(ByteCursor::new(after.to_vec())),
            None,
            None,
        )
        .expect("constructing forward Page with after")
    }

    /// Backward page opened from a `before` cursor (`last: N, before: <cursor>`).
    fn backward_page_before(limit: u64, before: &[u8]) -> Page<CTransaction> {
        let limits = PageLimits {
            default: limit as u32,
            max: limit as u32,
        };
        Page::from_params(
            &limits,
            None,
            None,
            Some(limit),
            Some(ByteCursor::new(before.to_vec())),
        )
        .expect("constructing backward Page with before")
    }

    /// Empty connection surfaces cursors if provided by the streamed page.
    #[test]
    fn build_bitmap_connection_empty_page_surfaces_boundary_cursors() {
        let scope = Scope::for_tests();
        let page = forward_page(10);
        let result = StreamPage::<v2::ExecutedTransaction>::for_test(
            Vec::new(),
            Some(Bytes::copy_from_slice(b"first-watermark")),
            Some(Bytes::copy_from_slice(b"last-watermark")),
            None,
        );

        let conn = build_bitmap_connection(scope, &page, result).expect("connection built");
        assert!(conn.edges.is_empty());
        assert!(!conn.page_info.has_previous_page);
        assert!(conn.page_info.has_next_page);

        // Both start and end cursors should be set on the connection
        let start = conn.page_info.start_cursor.expect("start cursor set");
        let end = conn.page_info.end_cursor.expect("end cursor set");
        assert_ne!(start, end, "start and end cursors should be different");
    }

    /// Order of cursors on connection should be swapped from streamed page.
    #[test]
    fn build_bitmap_connection_empty_page_backward_correct_cursors() {
        let scope = Scope::for_tests();
        let page = backward_page(10);
        let result = StreamPage::<v2::ExecutedTransaction>::for_test(
            Vec::new(),
            Some(Bytes::copy_from_slice(b"last-watermark")),
            Some(Bytes::copy_from_slice(b"first-watermark")),
            None,
        );

        let conn = build_bitmap_connection(scope, &page, result).expect("connection built");
        assert!(conn.edges.is_empty());
        assert!(conn.page_info.has_previous_page);
        assert!(!conn.page_info.has_next_page);

        let start = conn.page_info.start_cursor.expect("start cursor set");
        let end = conn.page_info.end_cursor.expect("end cursor set");
        assert_eq!(
            start,
            ByteCursor::new(b"first-watermark".to_vec()).encode_cursor()
        );
        assert_eq!(
            end,
            ByteCursor::new(b"last-watermark".to_vec()).encode_cursor()
        );
    }

    #[test]
    fn build_bitmap_connection_non_empty_page_uses_edge_cursors() {
        let scope = Scope::for_tests();
        let page = forward_page(10);
        let result = StreamPage::<v2::ExecutedTransaction>::for_test(
            vec![tx_item(b"edge-1"), tx_item(b"edge-2"), tx_item(b"edge-3")],
            None,
            None,
            Some(v2alpha::QueryEndReason::CheckpointBound),
        );

        let conn = build_bitmap_connection(scope, &page, result).expect("connection built");
        assert_eq!(conn.edges.len(), 3);
        // `CheckpointBound` means the range was exhausted — no forward continuation.
        assert!(!conn.page_info.has_next_page);

        let start = conn.page_info.start_cursor.expect("start set");
        let end = conn.page_info.end_cursor.expect("end set");
        assert_eq!(
            start, conn.edges[0].cursor,
            "non-empty page should anchor start_cursor on first edge, not stream watermark"
        );
        assert_eq!(
            end, conn.edges[2].cursor,
            "non-empty page should anchor end_cursor on last edge, not stream watermark"
        );
    }

    #[test]
    fn build_bitmap_connection_full_page_at_item_limit_signals_more() {
        let scope = Scope::for_tests();
        let page = forward_page(3);
        let result = StreamPage::<v2::ExecutedTransaction>::for_test(
            vec![tx_item(b"e1"), tx_item(b"e2"), tx_item(b"e3")],
            None,
            None,
            Some(v2alpha::QueryEndReason::ItemLimit),
        );

        let conn = build_bitmap_connection(scope, &page, result).expect("connection built");
        assert_eq!(conn.edges.len(), 3);
        assert!(
            conn.page_info.has_next_page,
            "full page + ItemLimit must report hasNextPage: true (has_more() is true)"
        );
    }

    /// If watermark cursors and non-empty, expect watermark cursors on the connection.
    #[test]
    fn build_bitmap_connection_non_empty_page_and_wm() {
        let scope = Scope::for_tests();
        let page = forward_page(3);
        let result = StreamPage::<v2::ExecutedTransaction>::for_test(
            vec![tx_item(b"e1"), tx_item(b"e2"), tx_item(b"e3")],
            Some(Bytes::copy_from_slice(b"first-watermark")),
            Some(Bytes::copy_from_slice(b"last-watermark")),
            None,
        );

        let conn = build_bitmap_connection(scope, &page, result).expect("connection built");
        assert_eq!(conn.edges.len(), 3);
        assert!(conn.page_info.has_next_page,);
        let start = conn.page_info.start_cursor.expect("start cursor set");
        let end = conn.page_info.end_cursor.expect("end cursor set");
        assert_eq!(
            start,
            ByteCursor::new(b"first-watermark".to_vec()).encode_cursor()
        );
        assert_eq!(
            end,
            ByteCursor::new(b"last-watermark".to_vec()).encode_cursor()
        );
    }

    #[test]
    fn build_bitmap_connection_descending_page_reverses_to_ascending_edges() {
        let scope = Scope::for_tests();
        let page = backward_page(10);
        // Descending stream order: c3, c2, c1 (highest position first).
        let result = StreamPage::<v2::ExecutedTransaction>::for_test(
            vec![tx_item(b"c3"), tx_item(b"c2"), tx_item(b"c1")],
            None,
            None,
            Some(v2alpha::QueryEndReason::CheckpointBound),
        );

        let conn = build_bitmap_connection(scope, &page, result).expect("connection built");
        assert_eq!(conn.edges.len(), 3);
        // After reversal, the *first* edge corresponds to the *lowest* position from the
        // stream — i.e. the last item the stream emitted (`c1`).
        let start = conn.page_info.start_cursor.expect("start set");
        let end = conn.page_info.end_cursor.expect("end set");
        assert_eq!(
            start, conn.edges[0].cursor,
            "descending page's start_cursor anchors on the first ascending edge after reversal"
        );
        assert_eq!(start, ByteCursor::new(b"c1".to_vec()).encode_cursor());
        assert_eq!(
            end, conn.edges[2].cursor,
            "descending page's end_cursor anchors on the last ascending edge after reversal"
        );
        assert_eq!(end, ByteCursor::new(b"c3".to_vec()).encode_cursor());
    }

    /// A forward page opened from an `after` cursor reports `hasPreviousPage: true`
    /// (`page.after().is_some()`). `CheckpointBound` makes `has_more()` false, so the only source
    /// of a `true` flag is the input cursor — not the stream.
    #[test]
    fn build_bitmap_connection_forward_after_signals_previous_page() {
        let scope = Scope::for_tests();
        let page = forward_page_after(10, b"after-cursor");
        let result = StreamPage::<v2::ExecutedTransaction>::for_test(
            vec![tx_item(b"e1"), tx_item(b"e2")],
            None,
            None,
            Some(v2alpha::QueryEndReason::CheckpointBound),
        );

        let conn = build_bitmap_connection(scope, &page, result).expect("connection built");
        assert!(
            conn.page_info.has_previous_page,
            "after cursor set → hasPreviousPage"
        );
        assert!(
            !conn.page_info.has_next_page,
            "CheckpointBound → no hasNextPage"
        );
    }

    /// A backward page opened from a `before` cursor reports `hasNextPage: true`
    /// (`page.before().is_some()`). `CheckpointBound` makes `has_more()` false, so the only source
    /// of a `true` flag is the input cursor — not the stream.
    #[test]
    fn build_bitmap_connection_backward_before_signals_next_page() {
        let scope = Scope::for_tests();
        let page = backward_page_before(10, b"before-cursor");
        let result = StreamPage::<v2::ExecutedTransaction>::for_test(
            vec![tx_item(b"c2"), tx_item(b"c1")],
            None,
            None,
            Some(v2alpha::QueryEndReason::CheckpointBound),
        );

        let conn = build_bitmap_connection(scope, &page, result).expect("connection built");
        assert!(
            conn.page_info.has_next_page,
            "before cursor set → hasNextPage"
        );
        assert!(
            !conn.page_info.has_previous_page,
            "CheckpointBound → no hasPreviousPage"
        );
    }
}
