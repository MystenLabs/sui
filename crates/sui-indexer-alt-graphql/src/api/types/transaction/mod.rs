// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::RangeInclusive;
use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::Context;
use async_graphql::Object;
use async_graphql::connection::Connection;
use async_graphql::connection::CursorType;
use async_graphql::connection::Edge;
use fastcrypto::encoding::Base58;
use fastcrypto::encoding::Encoding;
use sui_indexer_alt_reader::kv_loader::KvLoader;
use sui_indexer_alt_reader::kv_loader::TransactionContents as NativeTransactionContents;
use sui_indexer_alt_reader::ledger_grpc_reader::CheckpointedTransaction;
use sui_indexer_alt_reader::ledger_grpc_reader::LedgerGrpcReader;
use sui_rpc::proto::sui::rpc::v2;
use sui_types::base_types::SuiAddress as NativeSuiAddress;
use sui_types::digests::TransactionDigest;
use sui_types::transaction::TransactionDataAPI;
use sui_types::transaction::TransactionExpiration;

use crate::api::scalars::base64::Base64;
use crate::api::scalars::cursor::OpaqueCursor;
use crate::api::scalars::digest::Digest;
use crate::api::scalars::id::Id;
use crate::api::scalars::json::Json;
use crate::api::types::address::Address;
use crate::api::types::checkpoint::filter::checkpoint_bounds;
use crate::api::types::epoch::Epoch;
use crate::api::types::gas_input::GasInput;
use crate::api::types::ledger_cursor::LedgerToken;
use crate::api::types::ledger_cursor::TransactionKey;
use crate::api::types::ledger_cursor::scan::LedgerPage;
use crate::api::types::ledger_cursor::scan::scan;
use crate::api::types::lookups::CheckpointBounds;
use crate::api::types::transaction::filter::TransactionFilter;
use crate::api::types::transaction_effects::EffectsContents;
use crate::api::types::transaction_effects::TransactionEffects;
use crate::api::types::transaction_kind::TransactionKind;
use crate::api::types::user_signature::UserSignature;
use crate::error::RpcError;
use crate::extensions::query_limits;
use crate::pagination::Page;
use crate::pagination::StreamConnection;
use crate::scope::Scope;
use crate::task::streaming::ProcessedTransaction;

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

/// A decoded transaction cursor.
pub type TransactionToken = LedgerToken<TransactionKey>;

pub type CTransaction = OpaqueCursor<TransactionToken>;

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
                TransactionExpiration::ValidDuring { max_epoch, .. }
                | TransactionExpiration::Validity { max_epoch, .. } => {
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

            // `merge_transaction_data` (`sui-types/src/rpc_proto_conversions.rs`) derives every
            // `Transaction` proto field from the decoded `TransactionData` alone, so local
            // conversion is equivalent to the json produced from the proto transaction.
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

    /// Paginate over the transactions of a checkpoint held in memory (e.g. from streaming),
    /// applying the filter in memory. `transactions` holds every transaction in checkpoint
    /// `checkpoint`, in order, so a transaction's index in it is its index in the checkpoint. The
    /// cursors minted here continue the same pagination through the query API.
    pub(crate) fn paginate_preloaded_transactions(
        scope: Scope,
        checkpoint: u64,
        transactions: &[ProcessedTransaction],
        page: &Page<CTransaction>,
        filter: TransactionFilter,
    ) -> Result<StreamConnection<Transaction>, RpcError> {
        let after = page.after().map(|cursor| cursor.point());
        let before = page.before().map(|cursor| cursor.point());

        // Matches excluded by `after` precede the page, and matches excluded by `before` follow
        // it.
        let mut matches_before = false;
        let mut matches_after = false;
        let mut window = vec![];
        for (index, tx) in transactions.iter().enumerate() {
            if !filter.matches(&tx.contents) {
                continue;
            }

            let key = TransactionKey {
                checkpoint,
                transaction_index: index as u64,
            };

            if after.is_some_and(|a| !a.includes_after(&key)) {
                matches_before = true;
            } else if before.is_some_and(|b| !b.includes_before(&key)) {
                matches_after = true;
            } else {
                window.push((key, tx));
            }
        }

        let limit = page.limit();
        let overflows = window.len() > limit;
        let (has_previous_page, has_next_page) = if page.is_from_front() {
            window.truncate(limit);
            (matches_before, overflows || matches_after)
        } else {
            window.drain(..window.len().saturating_sub(limit));
            (overflows || matches_before, matches_after)
        };

        let mut conn = Connection::new(has_previous_page, has_next_page);
        for (key, tx) in window {
            conn.edges.push(Edge::new(
                TransactionToken::at(key).encode_cursor(),
                Transaction::with_contents(scope.clone(), tx.contents.clone())?,
            ));
        }

        Ok(conn.into())
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

    /// Cursor based pagination through transactions with filters applied, served by the ledger
    /// gRPC List APIs. Pages may be partially filled, with valid cursors if there are more pages
    /// to paginate through.
    ///
    /// Returns empty results when no checkpoint is set in scope (e.g. execution scope).
    pub(crate) async fn paginate(
        ctx: &Context<'_>,
        scope: Scope,
        page: Page<CTransaction>,
        filter: TransactionFilter,
    ) -> Result<StreamConnection<Transaction>, RpcError> {
        query_limits::rich::debit(ctx)?;

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

        let reader: &LedgerGrpcReader = ctx.data()?;
        let result = Self::scan_grpc(
            reader,
            cp_bounds,
            &page,
            page.limit_with_lookahead(),
            &filter,
        )
        .await?;

        build_grpc_connection(scope, &page, result)
    }

    /// Scan up to `limit` transactions from `page`'s cursors over the checkpoint range `cp_bounds`
    /// via the streaming gRPC List API. Computing checkpoint bounds is the caller's responsibility.
    /// Items are returned in scan order (descending when paginating from the back).
    pub(crate) async fn scan_grpc(
        reader: &LedgerGrpcReader,
        cp_bounds: RangeInclusive<u64>,
        page: &Page<CTransaction>,
        limit: usize,
        filter: &TransactionFilter,
    ) -> Result<LedgerPage<v2::ExecutedTransaction, CTransaction>, RpcError> {
        let filter = filter.to_grpc_filter();
        scan(page, cp_bounds, limit, transaction_key, |scan| async move {
            // Items are placed by their checkpoint and their index within it.
            let mut read_mask = CheckpointedTransaction::read_mask();
            read_mask.paths.push("transaction_index".to_owned());

            let mut request = v2::ListTransactionsRequest::default();
            request.read_mask = Some(read_mask);
            request.start_checkpoint = Some(scan.start.checkpoint);
            request.start_transaction_index = scan.start.transaction_index;
            request.end_checkpoint = Some(scan.end.checkpoint);
            request.end_transaction_index = scan.end.transaction_index;
            request.filter = filter;
            request.options = Some(scan.options);

            reader
                .list_transactions(request)
                .await
                .context("Failed to list transactions")
        })
        .await
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

        // Execution context is not backed by the index yet.
        if self.scope.is_executed() {
            return Ok(self.clone());
        }

        // A just-streamed transaction runs ahead of the KV backend, so serve it from the in-memory
        // streamed store (live streamed path only) until the backend catches up.
        if let Some(streaming_transactions) = self.scope.streamed_transaction_store()
            && let Some(contents) = streaming_transactions.get(&digest)
        {
            return Ok(Self {
                scope: self.scope.clone(),
                contents: Some(contents),
            });
        }

        let kv_loader: &KvLoader = ctx.data()?;
        let Some(transaction) = kv_loader
            .load_one_transaction(digest)
            .await
            .context("Failed to fetch transaction contents")?
        else {
            return Ok(self.clone());
        };

        // Enforce the consistency cutoff only when viewing as of a specific checkpoint. A
        // subscription backfill has no `checkpoint_viewed_at` and takes the indexed contents as-is.
        if let Some(checkpoint_viewed_at) = self.scope.checkpoint_viewed_at() {
            let cp_num = transaction
                .cp_sequence_number()
                .context("Any transaction fetched from the DB should have a checkpoint set")?;
            if cp_num > checkpoint_viewed_at {
                return Ok(self.clone());
            }
        }

        Ok(Self {
            scope: self.scope.clone(),
            contents: Some(Arc::new(transaction)),
        })
    }
}

/// Where a `ListTransactions` item sits: its checkpoint and its index within it.
fn transaction_key(payload: &v2::ExecutedTransaction) -> anyhow::Result<TransactionKey> {
    Ok(TransactionKey {
        checkpoint: payload
            .checkpoint
            .context("ListTransactions item missing checkpoint")?,
        transaction_index: payload
            .transaction_index
            .context("ListTransactions item missing transaction_index")?,
    })
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

/// Hydrate a `Transaction` node from a `ListTransactions` stream item. The item carries the
/// transaction's checkpointed contents, so fields resolve without a KV lookup.
pub(crate) fn transaction_from_stream_item(
    scope: Scope,
    payload: &v2::ExecutedTransaction,
) -> Result<Transaction, RpcError> {
    let contents = CheckpointedTransaction::try_from(payload)
        .context("Failed to convert ListTransactions item")?;
    Transaction::with_contents(
        scope,
        Arc::new(NativeTransactionContents::LedgerGrpc(contents)),
    )
}

/// Build a `StreamConnection<Transaction>` from a page of a `ListTransactions` stream.
///
/// Edges are returned in ascending order.
pub(crate) fn build_grpc_connection(
    scope: Scope,
    page: &Page<CTransaction>,
    result: LedgerPage<v2::ExecutedTransaction, CTransaction>,
) -> Result<StreamConnection<Transaction>, RpcError> {
    page.paginate_stream_results(result, |payload| {
        transaction_from_stream_item(scope.clone(), payload)
    })
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use fastcrypto::encoding::Base64;
    use sui_indexer_alt_reader::kv_loader::ExecutedTransactionData;
    use sui_indexer_alt_reader::ledger_grpc_reader::PageBoundary;
    use sui_indexer_alt_reader::ledger_grpc_reader::PageItem;
    use sui_indexer_alt_reader::ledger_grpc_reader::StreamPage;
    use sui_rpc::proto::sui::rpc::v2::QueryEndReason as End;
    use sui_types::base_types::random_object_ref;
    use sui_types::effects::TransactionEffects as NativeTransactionEffects;
    use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
    use sui_types::transaction::TransactionData;

    use crate::api::types::ledger_cursor::LedgerKey;
    use crate::api::types::ledger_cursor::Point;
    use crate::api::types::ledger_cursor::scan::PositionFields;
    use crate::api::types::ledger_cursor::scan::ScanPlan;
    use crate::api::types::ledger_cursor::scan::ScanRequest;
    use crate::pagination::PageLimits;

    use super::*;

    /// The checkpoint the preloaded transactions in these tests were streamed from.
    const STREAMED_CP: u64 = 42;

    fn key(checkpoint: u64, transaction_index: u64) -> TransactionKey {
        TransactionKey {
            checkpoint,
            transaction_index,
        }
    }

    fn fields(
        checkpoint: u64,
        transaction_index: Option<u64>,
        event_index: Option<u32>,
    ) -> PositionFields {
        PositionFields {
            checkpoint,
            transaction_index,
            event_index,
        }
    }

    /// A `ListTransactions` item at `key` with a minimal-but-valid payload (empty programmable
    /// transaction, default effects), carrying `cursor` as its ledger cursor.
    fn tx_item(cursor: &'static [u8], key: TransactionKey) -> PageItem<v2::ExecutedTransaction> {
        let pt = ProgrammableTransactionBuilder::new().finish();
        let data = TransactionData::new_programmable(
            NativeSuiAddress::ZERO,
            vec![random_object_ref()],
            pt,
            1,
            1,
        );

        let mut transaction = v2::Transaction::default();
        transaction.bcs = Some(v2::Bcs::serialize(&data).expect("serialize transaction"));

        let mut effects = v2::TransactionEffects::default();
        effects.bcs = Some(
            v2::Bcs::serialize(&NativeTransactionEffects::default()).expect("serialize effects"),
        );

        let mut payload = v2::ExecutedTransaction::default();
        payload.transaction = Some(transaction);
        payload.effects = Some(effects);
        payload.checkpoint = Some(key.checkpoint);
        payload.transaction_index = Some(key.transaction_index);
        PageItem {
            payload,
            cursor: Bytes::from_static(cursor),
            position: position(key.successor()),
        }
    }

    /// The ledger position before `at`.
    fn position(at: TransactionKey) -> v2::LedgerPosition {
        v2::LedgerPosition::default()
            .with_checkpoint(at.checkpoint)
            .with_transaction_index(at.transaction_index)
    }

    /// A watermark frame without an item, carrying `cursor` and the position before `at`.
    fn boundary(cursor: &'static [u8], at: TransactionKey) -> PageBoundary {
        PageBoundary {
            cursor: Bytes::from_static(cursor),
            position: position(at),
        }
    }

    fn stream(
        items: Vec<PageItem<v2::ExecutedTransaction>>,
        first_watermark: Option<PageBoundary>,
        last_watermark: Option<PageBoundary>,
        end: Option<End>,
    ) -> StreamPage<v2::ExecutedTransaction> {
        StreamPage::for_test(items, first_watermark, last_watermark, end)
    }

    fn page(
        first: Option<u64>,
        after: Option<CTransaction>,
        last: Option<u64>,
        before: Option<CTransaction>,
    ) -> Page<CTransaction> {
        let limits = PageLimits {
            default: 10,
            max: 100,
        };
        Page::from_params(&limits, first, after, last, before).expect("valid page")
    }

    /// Mint GraphQL cursors for `stream`, the response to `page` over checkpoints 0 to 100.
    fn ledger_page(
        page: &Page<CTransaction>,
        stream: StreamPage<v2::ExecutedTransaction>,
    ) -> LedgerPage<v2::ExecutedTransaction, CTransaction> {
        ScanPlan::new(page, 0..=100)
            .page(stream, transaction_key)
            .expect("page minted")
    }

    fn connection(
        page: &Page<CTransaction>,
        stream: StreamPage<v2::ExecutedTransaction>,
    ) -> StreamConnection<Transaction> {
        build_grpc_connection(Scope::for_tests(), page, ledger_page(page, stream))
            .expect("connection built")
    }

    /// The request `page` sends over checkpoints 0 to 100.
    fn scan_request(page: &Page<CTransaction>) -> ScanRequest {
        ScanPlan::new(page, 0..=100)
            .request(page.limit_with_lookahead())
            .expect("non-empty range")
    }

    fn decode(cursor: &str) -> TransactionToken {
        (*CTransaction::decode_cursor(cursor).expect("valid cursor")).clone()
    }

    /// The ledger bytes a GraphQL cursor carries.
    fn ledger_bytes(cursor: &CTransaction) -> Option<Bytes> {
        cursor.ledger_bytes().cloned()
    }

    /// A ledger cursor carrying `bytes`, at the transaction at `at`.
    fn ledger_item(bytes: &'static [u8], at: TransactionKey) -> CTransaction {
        TransactionToken::from_ledger(Bytes::from_static(bytes), Point::At(at))
    }

    /// The place of each edge, read back from its cursor.
    fn edge_keys(conn: &StreamConnection<Transaction>) -> Vec<(u64, u64)> {
        conn.edges
            .iter()
            .map(|edge| match decode(&edge.cursor).point() {
                Point::At(key) => (key.checkpoint, key.transaction_index),
                point => panic!("expected an item cursor, got {point:?}"),
            })
            .collect()
    }

    fn start_cursor(conn: &StreamConnection<Transaction>) -> CTransaction {
        CTransaction::decode_cursor(
            conn.page_info
                .start_cursor
                .as_deref()
                .expect("start cursor"),
        )
        .expect("valid cursor")
    }

    fn end_cursor(conn: &StreamConnection<Transaction>) -> CTransaction {
        CTransaction::decode_cursor(conn.page_info.end_cursor.as_deref().expect("end cursor"))
            .expect("valid cursor")
    }

    /// A `ProcessedTransaction` with just enough content for `TransactionFilter::matches` (sender
    /// and effects) and digest extraction.
    fn preloaded_tx(sender: NativeSuiAddress) -> ProcessedTransaction {
        let pt = ProgrammableTransactionBuilder::new().finish();
        let data = TransactionData::new_programmable(sender, vec![random_object_ref()], pt, 1, 1);
        let contents = NativeTransactionContents::ExecutedTransaction(ExecutedTransactionData {
            effects: Box::new(NativeTransactionEffects::default()),
            events: vec![],
            transaction_data: Box::new(data),
            signatures: vec![],
            balance_changes: vec![],
            proto_effects: None,
            proto_transaction: None,
            timestamp_ms: Some(0),
            cp_sequence_number: Some(STREAMED_CP),
        });

        ProcessedTransaction {
            contents: Arc::new(contents),
        }
    }

    /// `count` transactions held in memory, all from the same sender.
    fn preloaded_txs(count: usize) -> Vec<ProcessedTransaction> {
        (0..count)
            .map(|_| preloaded_tx(NativeSuiAddress::ZERO))
            .collect()
    }

    fn preloaded(
        page: &Page<CTransaction>,
        transactions: &[ProcessedTransaction],
    ) -> Result<StreamConnection<Transaction>, RpcError> {
        Transaction::paginate_preloaded_transactions(
            Scope::for_tests(),
            STREAMED_CP,
            transactions,
            page,
            TransactionFilter::default(),
        )
    }

    /// Transactions held in memory get item cursors at their place in the checkpoint.
    #[test]
    fn paginate_preloaded_mints_cursors() {
        let txs = preloaded_txs(5);
        let conn = preloaded(&page(Some(3), None, None, None), &txs).unwrap();

        assert_eq!(edge_keys(&conn), [(42, 0), (42, 1), (42, 2)]);
        assert!(!conn.page_info.has_previous_page);
        assert!(conn.page_info.has_next_page);
        assert_eq!(
            conn.page_info.start_cursor.as_deref(),
            Some(conn.edges[0].cursor.as_str())
        );
        assert_eq!(
            conn.page_info.end_cursor.as_deref(),
            Some(conn.edges[2].cursor.as_str())
        );
    }

    /// In memory, a cursor is placed by its key.
    #[test]
    fn paginate_preloaded_resumes_after_cursor() {
        let txs = preloaded_txs(5);
        let after = TransactionToken::at(key(STREAMED_CP, 1));
        let conn = preloaded(&page(Some(2), Some(after), None, None), &txs).unwrap();
        assert_eq!(edge_keys(&conn), [(42, 2), (42, 3)]);
        assert!(conn.page_info.has_previous_page);
        assert!(conn.page_info.has_next_page);
    }

    /// A ledger page boundary splits transactions held in memory at the position it sits at.
    #[test]
    fn paginate_preloaded_after_a_ledger_boundary() {
        let txs = preloaded_txs(5);
        let after = TransactionToken::from_ledger(
            Bytes::from_static(b"w"),
            Point::Before(key(STREAMED_CP, 2)),
        );
        let conn = preloaded(&page(Some(2), Some(after), None, None), &txs).unwrap();
        assert_eq!(edge_keys(&conn), [(42, 2), (42, 3)]);
        assert!(conn.page_info.has_previous_page);
    }

    /// A cursor in an earlier checkpoint puts the whole checkpoint after it.
    #[test]
    fn paginate_preloaded_after_an_earlier_checkpoint() {
        let txs = preloaded_txs(3);
        let after = TransactionToken::at(key(STREAMED_CP - 1, 7));
        let conn = preloaded(&page(Some(10), Some(after), None, None), &txs).unwrap();
        assert_eq!(edge_keys(&conn), [(42, 0), (42, 1), (42, 2)]);
        assert!(!conn.page_info.has_previous_page);
        assert!(!conn.page_info.has_next_page);
    }

    /// `last: n` returns the tail of the matching set.
    #[test]
    fn paginate_preloaded_backward_page_returns_tail() {
        let txs = preloaded_txs(5);
        let conn = preloaded(&page(None, None, Some(2), None), &txs).unwrap();
        assert_eq!(edge_keys(&conn), [(42, 3), (42, 4)]);
        assert!(conn.page_info.has_previous_page);
        assert!(!conn.page_info.has_next_page);
    }

    /// `last: n, before: <cursor>` returns the transactions immediately preceding the cursor.
    #[test]
    fn paginate_preloaded_backward_page_before_cursor() {
        let txs = preloaded_txs(7);
        let before = TransactionToken::at(key(STREAMED_CP, 5));
        let conn = preloaded(&page(None, None, Some(2), Some(before)), &txs).unwrap();
        assert_eq!(edge_keys(&conn), [(42, 3), (42, 4)]);
        assert!(conn.page_info.has_previous_page);
        assert!(conn.page_info.has_next_page);
    }

    /// A `last: n` page over fewer matches than `n` returns everything.
    #[test]
    fn paginate_preloaded_backward_page_smaller_than_window() {
        let txs = preloaded_txs(2);
        let conn = preloaded(&page(None, None, Some(3), None), &txs).unwrap();
        assert_eq!(edge_keys(&conn), [(42, 0), (42, 1)]);
        assert!(!conn.page_info.has_previous_page);
        assert!(!conn.page_info.has_next_page);
    }

    /// A ledger cursor without a place in ledger order is not a valid cursor.
    #[test]
    fn ledger_cursor_without_a_place_is_invalid() {
        // Marker, `Transaction`, `Ledger`, the length-prefixed ledger bytes, then `None`.
        let unplaced = Base64::encode([0x00, 0x00, 0x01, 0x01, 0xaa, 0x00]);
        let error = CTransaction::decode_cursor(&unplaced).expect_err("rejected");
        assert!(
            error.to_string().contains("ledger cursor has no position"),
            "{error}"
        );
    }

    /// An empty page surfaces the stream's watermarks as its cursors, at the positions the ledger
    /// reports for them.
    #[test]
    fn empty_page_surfaces_boundary_cursors() {
        let page = page(Some(10), None, None, None);
        let conn = connection(
            &page,
            stream(
                vec![],
                Some(boundary(b"w1", key(1, 0))),
                Some(boundary(b"w2", key(3, 0))),
                None,
            ),
        );
        assert!(conn.edges.is_empty());
        assert!(!conn.page_info.has_previous_page);
        assert!(conn.page_info.has_next_page);

        let start = start_cursor(&conn);
        assert_eq!(ledger_bytes(&start).as_deref(), Some(&b"w1"[..]));
        assert_eq!(start.point(), Point::Before(key(1, 0)));

        let end = end_cursor(&conn);
        assert_eq!(ledger_bytes(&end).as_deref(), Some(&b"w2"[..]));
        assert_eq!(end.point(), Point::Before(key(3, 0)));
    }

    /// Paginating from the back, the stream's first watermark is the page's end and its last one
    /// the page's start.
    #[test]
    fn empty_backward_page_swaps_boundary_cursors() {
        let page = page(None, None, Some(10), None);
        let conn = connection(
            &page,
            stream(
                vec![],
                Some(boundary(b"high", key(9, 0))),
                Some(boundary(b"low", key(4, 0))),
                None,
            ),
        );
        assert!(conn.edges.is_empty());
        assert!(conn.page_info.has_previous_page);
        assert!(!conn.page_info.has_next_page);
        assert_eq!(
            ledger_bytes(&start_cursor(&conn)).as_deref(),
            Some(&b"low"[..])
        );
        assert_eq!(
            ledger_bytes(&end_cursor(&conn)).as_deref(),
            Some(&b"high"[..])
        );
    }

    /// A page without watermarks anchors its cursors on its first and last edges.
    #[test]
    fn non_empty_page_uses_edge_cursors() {
        let page = page(Some(10), None, None, None);
        let items = vec![
            tx_item(b"i0", key(1, 0)),
            tx_item(b"i1", key(1, 1)),
            tx_item(b"i2", key(1, 2)),
        ];
        let conn = connection(&page, stream(items, None, None, Some(End::CheckpointBound)));
        assert_eq!(edge_keys(&conn), [(1, 0), (1, 1), (1, 2)]);
        // `CheckpointBound` means the range was exhausted: no forward continuation.
        assert!(!conn.page_info.has_next_page);
        assert_eq!(
            conn.page_info.start_cursor.as_deref(),
            Some(conn.edges[0].cursor.as_str())
        );
        assert_eq!(
            conn.page_info.end_cursor.as_deref(),
            Some(conn.edges[2].cursor.as_str())
        );
        assert_eq!(
            ledger_bytes(&end_cursor(&conn)).as_deref(),
            Some(&b"i2"[..])
        );
    }

    /// A server that capped the requested limit at its own maximum ends a full page at
    /// `ItemLimit` without returning the lookahead item, so more data may still exist.
    #[test]
    fn capped_item_limit_signals_more() {
        let page = page(Some(3), None, None, None);
        let items = vec![
            tx_item(b"i0", key(1, 0)),
            tx_item(b"i1", key(1, 1)),
            tx_item(b"i2", key(1, 2)),
        ];
        let conn = connection(&page, stream(items, None, None, Some(End::ItemLimit)));
        assert_eq!(conn.edges.len(), 3);
        assert!(conn.page_info.has_next_page);
    }

    /// The lookahead item proves that a further page exists. It is dropped, and the page ends on
    /// the last kept item so that resuming from `endCursor` returns the dropped item next.
    #[test]
    fn forward_lookahead_item_dropped() {
        let page = page(Some(2), None, None, None);
        let items = vec![
            tx_item(b"i0", key(1, 0)),
            tx_item(b"i1", key(1, 1)),
            tx_item(b"i2", key(1, 2)),
        ];
        let conn = connection(&page, stream(items, None, None, Some(End::ItemLimit)));
        assert_eq!(edge_keys(&conn), [(1, 0), (1, 1)]);
        assert!(!conn.page_info.has_previous_page);
        assert!(conn.page_info.has_next_page);
        assert_eq!(
            ledger_bytes(&start_cursor(&conn)).as_deref(),
            Some(&b"i0"[..])
        );
        assert_eq!(
            ledger_bytes(&end_cursor(&conn)).as_deref(),
            Some(&b"i1"[..])
        );
    }

    /// Paginating from the back, the lookahead item is the lowest one (the last the descending
    /// stream emitted), and the page starts on the lowest kept item.
    #[test]
    fn backward_lookahead_item_dropped() {
        let page = page(None, None, Some(2), None);
        let items = vec![
            tx_item(b"i2", key(1, 2)),
            tx_item(b"i1", key(1, 1)),
            tx_item(b"i0", key(1, 0)),
        ];
        let conn = connection(&page, stream(items, None, None, Some(End::ItemLimit)));
        assert_eq!(edge_keys(&conn), [(1, 1), (1, 2)]);
        assert!(conn.page_info.has_previous_page);
        assert!(!conn.page_info.has_next_page);
        assert_eq!(
            ledger_bytes(&start_cursor(&conn)).as_deref(),
            Some(&b"i1"[..])
        );
        assert_eq!(
            ledger_bytes(&end_cursor(&conn)).as_deref(),
            Some(&b"i2"[..])
        );
    }

    /// A descending scan bounded by `after` closes with a boundary past it. The page starts on its
    /// lowest item, so paging backwards from `startCursor` returns the `after` item.
    #[test]
    fn backward_cursor_bound_starts_on_last_item() {
        let page = page(None, Some(ledger_item(b"i0", key(1, 0))), Some(2), None);
        let items = vec![tx_item(b"i2", key(1, 2)), tx_item(b"i1", key(1, 1))];
        let conn = connection(
            &page,
            stream(
                items,
                None,
                Some(boundary(b"b", key(1, 1))),
                Some(End::CursorBound),
            ),
        );
        assert_eq!(edge_keys(&conn), [(1, 1), (1, 2)]);
        assert_eq!(
            conn.page_info.start_cursor.as_deref(),
            Some(conn.edges[0].cursor.as_str())
        );
    }

    /// Watermarks around the items become the page's cursors.
    #[test]
    fn non_empty_page_and_watermarks() {
        let page = page(Some(3), None, None, None);
        let items = vec![tx_item(b"i0", key(1, 0)), tx_item(b"i1", key(1, 1))];
        let conn = connection(
            &page,
            stream(
                items,
                Some(boundary(b"w0", key(1, 0))),
                Some(boundary(b"w2", key(1, 2))),
                None,
            ),
        );
        assert_eq!(conn.edges.len(), 2);
        assert!(conn.page_info.has_next_page);
        assert_eq!(
            ledger_bytes(&start_cursor(&conn)).as_deref(),
            Some(&b"w0"[..])
        );
        assert_eq!(
            ledger_bytes(&end_cursor(&conn)).as_deref(),
            Some(&b"w2"[..])
        );
    }

    /// A page whose last frame has no item, partway through a checkpoint (a scan limit), ends
    /// exactly at the position the ledger reports for that frame, in either direction.
    #[test]
    fn page_ending_on_a_boundary_mid_checkpoint_ends_at_its_position() {
        let forward = page(Some(10), None, None, None);
        let items = vec![tx_item(b"i0", key(3, 4))];
        let result = ledger_page(
            &forward,
            stream(
                items,
                None,
                Some(boundary(b"w", key(3, 9))),
                Some(End::ScanLimit),
            ),
        );
        let end = result.last_cursor.expect("resume cursor");
        assert_eq!(ledger_bytes(&end).as_deref(), Some(&b"w"[..]));
        assert_eq!(end.point(), Point::Before(key(3, 9)));

        // The next page starts at that position, with the frame's bytes in `after`.
        let next = scan_request(&page(Some(10), Some(end), None, None));
        assert_eq!(next.options.after.as_deref(), Some(&b"w"[..]));
        assert_eq!(next.start, fields(3, Some(9), None));

        let backward = page(None, None, Some(10), None);
        let items = vec![tx_item(b"i0", key(7, 5))];
        let result = ledger_page(
            &backward,
            stream(
                items,
                None,
                Some(boundary(b"w", key(7, 2))),
                Some(End::ScanLimit),
            ),
        );
        let end = result.last_cursor.expect("resume cursor");
        assert_eq!(end.point(), Point::Before(key(7, 2)));

        // The previous page ends at that position, with the frame's bytes in `before`.
        let previous = scan_request(&page(None, None, Some(10), Some(end)));
        assert_eq!(previous.options.before.as_deref(), Some(&b"w"[..]));
        assert_eq!(previous.end, fields(8, Some(2), None));
    }

    /// A descending page is reversed into ascending edges.
    #[test]
    fn descending_page_reverses_to_ascending_edges() {
        let page = page(None, None, Some(10), None);
        let items = vec![
            tx_item(b"i2", key(1, 2)),
            tx_item(b"i1", key(1, 1)),
            tx_item(b"i0", key(1, 0)),
        ];
        let conn = connection(&page, stream(items, None, None, Some(End::CheckpointBound)));
        assert_eq!(edge_keys(&conn), [(1, 0), (1, 1), (1, 2)]);
        assert_eq!(
            ledger_bytes(&start_cursor(&conn)).as_deref(),
            Some(&b"i0"[..])
        );
        assert_eq!(
            ledger_bytes(&end_cursor(&conn)).as_deref(),
            Some(&b"i2"[..])
        );
    }

    /// A forward page opened from an `after` cursor reports `hasPreviousPage`, and a backward page
    /// opened from a `before` cursor reports `hasNextPage`, even when the stream reports no more.
    #[test]
    fn request_cursor_signals_the_page_behind_it() {
        let forward = page(Some(10), Some(TransactionToken::at(key(1, 0))), None, None);
        let items = vec![tx_item(b"i1", key(1, 1))];
        let conn = connection(
            &forward,
            stream(items, None, None, Some(End::CheckpointBound)),
        );
        assert!(conn.page_info.has_previous_page);
        assert!(!conn.page_info.has_next_page);

        let backward = page(None, None, Some(10), Some(TransactionToken::at(key(1, 3))));
        let items = vec![tx_item(b"i2", key(1, 2)), tx_item(b"i1", key(1, 1))];
        let conn = connection(
            &backward,
            stream(items, None, None, Some(End::CheckpointBound)),
        );
        assert!(conn.page_info.has_next_page);
        assert!(!conn.page_info.has_previous_page);
    }

    /// A cursor with no ledger bytes (minted from data GraphQL holds) starts the scan at its place:
    /// at its checkpoint, from the position after it. The page's resume cursor is its last item, so
    /// the next page sends that item's ledger bytes and starts after it.
    #[test]
    fn point_cursor_starts_the_scan_after_itself() {
        let first = page(Some(10), Some(TransactionToken::at(key(5, 1))), None, None);
        let request = scan_request(&first);
        assert_eq!(request.options.after, None);
        assert_eq!(request.start, fields(5, Some(2), None));
        assert_eq!(request.end, fields(101, None, None));

        let items = vec![tx_item(b"i2", key(5, 2))];
        let result = ledger_page(&first, stream(items, None, None, Some(End::ItemLimit)));
        let resume = result.last_cursor.expect("resume cursor");
        assert_eq!(ledger_bytes(&resume).as_deref(), Some(&b"i2"[..]));
        assert_eq!(resume.point(), Point::At(key(5, 2)));

        let second = scan_request(&page(Some(10), Some(resume), None, None));
        assert_eq!(second.options.after.as_deref(), Some(&b"i2"[..]));
        assert_eq!(second.start, fields(5, Some(3), None));
    }

    /// Paging forward with a `before` cursor, the scan ends at the cursor: within its checkpoint,
    /// at the position of the cursor itself. Reaching it ends the page on its last item, with more
    /// to come.
    #[test]
    fn point_cursor_before_ends_the_page() {
        let bounded = page(Some(10), None, None, Some(TransactionToken::at(key(5, 1))));
        let request = scan_request(&bounded);
        assert_eq!(request.end, fields(6, Some(1), None));

        let items = vec![tx_item(b"i0", key(5, 0))];
        let conn = connection(
            &bounded,
            stream(items, None, None, Some(End::CheckpointBound)),
        );
        assert_eq!(edge_keys(&conn), [(5, 0)]);
        assert!(conn.page_info.has_next_page);
        assert_eq!(
            conn.page_info.end_cursor.as_deref(),
            Some(conn.edges[0].cursor.as_str())
        );

        // A cursor at a checkpoint's start bounds the scan before that checkpoint.
        let at_start = page(
            Some(10),
            None,
            None,
            Some(TransactionToken::checkpoint_start(5)),
        );
        assert_eq!(scan_request(&at_start).end, fields(5, None, None));
    }

    /// A ledger server that stops at the far cursor's position, inside the last checkpoint of the
    /// page's range, has reached the far cursor, not the end of the data: the page ends on its last
    /// item, with more to come.
    #[test]
    fn stopping_at_the_far_position_reaches_the_far_cursor() {
        let bounded = page(Some(10), None, None, Some(TransactionToken::at(key(5, 1))));
        let plan = ScanPlan::new(&bounded, 0..=5);
        let request = plan.request(bounded.limit_with_lookahead()).unwrap();
        assert_eq!(request.end, fields(6, Some(1), None));

        let items = vec![tx_item(b"i0", key(5, 0))];
        let stream = stream(items, None, None, Some(End::CheckpointBound));
        let result = plan.page(stream, transaction_key).unwrap();
        let conn = build_grpc_connection(Scope::for_tests(), &bounded, result).unwrap();
        assert_eq!(edge_keys(&conn), [(5, 0)]);
        assert!(conn.page_info.has_next_page);
    }

    /// A cursor's position is sent when it lies within the page's checkpoint range. When a
    /// checkpoint filter already bounds the range past the cursor, the request starts or ends at
    /// the range's edge instead.
    #[test]
    fn positions_are_clamped_to_the_checkpoint_range() {
        let request = |page: &Page<CTransaction>, cp_bounds| {
            ScanPlan::new(page, cp_bounds)
                .request(page.limit_with_lookahead())
                .unwrap()
        };

        let after = page(Some(10), Some(TransactionToken::at(key(5, 1))), None, None);
        assert_eq!(request(&after, 5..=100).start, fields(5, Some(2), None));
        assert_eq!(request(&after, 7..=100).start, fields(7, None, None));

        let before = page(None, None, Some(10), Some(TransactionToken::at(key(5, 1))));
        assert_eq!(request(&before, 0..=5).end, fields(6, Some(1), None));
        assert_eq!(request(&before, 0..=3).end, fields(4, None, None));
    }

    /// Cursors whose places cross leave nothing between them, whether or not they carry ledger
    /// bytes. The page ends empty without a request, as an exhausted window: no further page
    /// either way.
    #[tokio::test]
    async fn crossed_cursors_end_the_page() {
        let no_request = |_| async { Err(anyhow::anyhow!("an empty range needs no request")) };
        let crossed_points = page(
            Some(10),
            Some(TransactionToken::at(key(2, 0))),
            None,
            Some(TransactionToken::at(key(1, 1))),
        );
        let crossed_ledger = page(
            Some(10),
            Some(ledger_item(b"i3", key(2, 0))),
            None,
            Some(ledger_item(b"i1", key(1, 1))),
        );

        for crossed in [crossed_points, crossed_ledger] {
            let result = scan(
                &crossed,
                0..=100,
                crossed.limit_with_lookahead(),
                transaction_key,
                no_request,
            )
            .await
            .unwrap();
            let conn = build_grpc_connection(Scope::for_tests(), &crossed, result).unwrap();
            assert!(conn.edges.is_empty());
            assert!(!conn.page_info.has_previous_page);
            assert!(!conn.page_info.has_next_page);
        }
    }

    /// A cursor whose place is past the page's range (as a subscription's cursor can be ahead of
    /// the checkpoint a query views) ends the page empty without a request: no further page, and a
    /// resume cursor at the cursor's own place, to poll from.
    #[tokio::test]
    async fn cursor_past_the_range_resumes_at_itself() {
        let no_request = |_| async { Err(anyhow::anyhow!("an empty range needs no request")) };
        let ahead = key(150, 2);
        let behind = key(10, 1);
        let cases = [
            (
                page(Some(10), Some(TransactionToken::at(ahead)), None, None),
                Point::At(ahead).gap_after(),
            ),
            (
                page(None, None, Some(10), Some(TransactionToken::at(behind))),
                Point::At(behind).gap_before(),
            ),
        ];
        for (page, place) in cases {
            let limit = page.limit_with_lookahead();
            let result = scan(&page, 50..=100, limit, transaction_key, no_request)
                .await
                .unwrap();
            let conn = build_grpc_connection(Scope::for_tests(), &page, result).unwrap();
            assert!(conn.edges.is_empty());
            let (more, resume) = if page.is_from_front() {
                (conn.page_info.has_next_page, end_cursor(&conn))
            } else {
                (conn.page_info.has_previous_page, start_cursor(&conn))
            };
            assert!(!more);
            assert_eq!(ledger_bytes(&resume), None);
            assert_eq!(resume.point(), place);
        }
    }

    /// Between two cursors, an empty page that stopped at the far cursor has read the whole window:
    /// it reports no further page and no cursors, in either direction. The far cursor is reached
    /// at its ledger bytes (`CursorBound`) or at its position (`CheckpointBound`).
    #[test]
    fn empty_page_between_cursors_ends() {
        let mut cases = vec![];
        for (first, last) in [(Some(2), None), (None, Some(2))] {
            for end in [End::CursorBound, End::CheckpointBound] {
                let window = page(
                    first,
                    Some(ledger_item(b"i2", key(1, 2))),
                    last,
                    Some(ledger_item(b"i5", key(1, 5))),
                );
                let stream = stream(
                    vec![],
                    Some(boundary(b"w", key(1, 3))),
                    Some(boundary(b"w", key(1, 3))),
                    Some(end),
                );
                cases.push((window, stream));
            }
        }

        // Between two places the ledger never saw, the page ends at the far position.
        let point_window = page(
            Some(10),
            Some(TransactionToken::at(key(1, 2))),
            None,
            Some(TransactionToken::at(key(1, 5))),
        );
        cases.push((
            point_window,
            stream(vec![], None, None, Some(End::CheckpointBound)),
        ));

        for (page, stream) in cases {
            let conn = connection(&page, stream);
            assert!(conn.edges.is_empty());
            assert!(!conn.page_info.has_previous_page);
            assert!(!conn.page_info.has_next_page);
            assert_eq!(conn.page_info.start_cursor, None);
            assert_eq!(conn.page_info.end_cursor, None);
        }
    }

    /// An empty page that stopped short of the far cursor, or that only one cursor bounds, still
    /// reports a further page and resumes from the server's boundary.
    #[test]
    fn empty_page_not_between_cursors_signals_more() {
        let cases = [
            (
                page(
                    Some(2),
                    Some(ledger_item(b"i0", key(1, 0))),
                    None,
                    Some(ledger_item(b"i9", key(1, 9))),
                ),
                End::ScanLimit,
            ),
            (
                page(None, Some(ledger_item(b"i2", key(1, 2))), Some(2), None),
                End::CursorBound,
            ),
        ];
        for (page, reason) in cases {
            let conn = connection(
                &page,
                stream(
                    vec![],
                    Some(boundary(b"w", key(1, 5))),
                    Some(boundary(b"w", key(1, 5))),
                    Some(reason),
                ),
            );
            let more = if page.is_from_front() {
                conn.page_info.has_next_page
            } else {
                conn.page_info.has_previous_page
            };
            assert!(more, "{reason:?}");
            assert_eq!(
                ledger_bytes(&end_cursor(&conn)),
                Some(Bytes::from_static(b"w"))
            );
        }
    }
}
