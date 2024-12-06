// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    address::Address,
    base64::Base64,
    cursor::{Page, Target},
    digest::Digest,
    epoch::Epoch,
    gas::GasInput,
    sui_address::SuiAddress,
    transaction_block_effects::{TransactionBlockEffects, TransactionBlockEffectsKind},
    transaction_block_kind::TransactionBlockKind,
};
use crate::{
    config::ServiceConfig,
    connection::ScanConnection,
    data::{self, DataLoader, Db, DbConnection, QueryExecutor},
    error::Error,
    server::watermark_task::Watermark,
};
use async_graphql::{connection::CursorType, dataloader::Loader, *};
use connection::Edge;
use cursor::TxLookup;
use diesel::{ExpressionMethods, JoinOnDsl, QueryDsl, SelectableHelper};
use diesel_async::scoped_futures::ScopedFutureExt;
use fastcrypto::encoding::{Base58, Encoding};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use sui_indexer::{
    models::transactions::StoredTransaction,
    schema::{transactions, tx_digests},
};
use sui_types::{
    base_types::SuiAddress as NativeSuiAddress,
    effects::TransactionEffects as NativeTransactionEffects,
    event::Event as NativeEvent,
    message_envelope::Message,
    transaction::{
        SenderSignedData as NativeSenderSignedData, TransactionData as NativeTransactionData,
        TransactionDataAPI, TransactionExpiration,
    },
};

mod cursor;
mod filter;
mod tx_lookups;

pub(crate) use cursor::Cursor;
pub(crate) use filter::TransactionBlockFilter;
pub(crate) use tx_lookups::{subqueries, TxBounds};

/// Wraps the actual transaction block data with the checkpoint sequence number at which the data
/// was viewed, for consistent results on paginating through and resolving nested types.
#[derive(Clone, Debug)]
pub(crate) struct TransactionBlock {
    pub inner: TransactionBlockInner,
    /// The checkpoint sequence number this was viewed at.
    pub checkpoint_viewed_at: u64,
}

#[derive(Clone, Debug)]
pub(crate) enum TransactionBlockInner {
    /// A transaction block that has been indexed and stored in the database,
    /// containing all information that the other two variants have, and more.
    Stored {
        stored_tx: StoredTransaction,
        native: NativeSenderSignedData,
    },
    /// A transaction block that has been executed via executeTransactionBlock
    /// but not yet indexed.
    Executed {
        tx_data: NativeSenderSignedData,
        effects: NativeTransactionEffects,
        events: Vec<NativeEvent>,
    },
    /// A transaction block that has been executed via dryRunTransactionBlock.
    /// This variant also does not return signatures or digest since only `NativeTransactionData` is present.
    DryRun {
        tx_data: NativeTransactionData,
        effects: NativeTransactionEffects,
        events: Vec<NativeEvent>,
    },
}

/// An input filter selecting for either system or programmable transactions.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum TransactionBlockKindInput {
    /// A system transaction can be one of several types of transactions.
    /// See [unions/transaction-block-kind] for more details.
    SystemTx = 0,
    /// A user submitted transaction block.
    ProgrammableTx = 1,
}

/// Filter for a point query of a TransactionBlock.
pub(crate) enum TransactionBlockLookup {
    ByDigest {
        digest: Digest,
        checkpoint_viewed_at: u64,
    },
    BySeq {
        tx_sequence_number: u64,
        checkpoint_viewed_at: u64,
    },
}

type Query<ST, GB> = data::Query<ST, transactions::table, GB>;

/// The cursor returned for each `TransactionBlock` in a connection's page of results. The
/// `checkpoint_viewed_at` will set the consistent upper bound for subsequent queries made on this
/// cursor.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub(crate) struct TransactionBlockCursor {
    /// The checkpoint sequence number this was viewed at.
    #[serde(rename = "c")]
    pub checkpoint_viewed_at: u64,
    #[serde(rename = "t")]
    pub tx_sequence_number: u64,
    /// The checkpoint sequence number when the transaction was finalized.
    #[serde(rename = "tc")]
    pub tx_checkpoint_number: u64,
}

/// `DataLoader` key for fetching a `TransactionBlock` by its digest, constrained by a consistency
/// cursor.
#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug)]
struct DigestKey {
    pub digest: Digest,
    pub checkpoint_viewed_at: u64,
}

/// `DataLoader` key for fetching a `TransactionBlock` by its sequence number, constrained by a
/// consistency cursor.
#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug)]
struct SeqKey {
    pub tx_sequence_number: u64,
    pub checkpoint_viewed_at: u64,
}

#[Object]
impl TransactionBlock {
    /// A 32-byte hash that uniquely identifies the transaction block contents, encoded in Base58.
    /// This serves as a unique id for the block on chain.
    async fn digest(&self) -> Option<String> {
        self.native_signed_data()
            .map(|s| Base58::encode(s.digest()))
    }

    /// The address corresponding to the public key that signed this transaction. System
    /// transactions do not have senders.
    async fn sender(&self) -> Option<Address> {
        let sender = self.native().sender();

        (sender != NativeSuiAddress::ZERO).then(|| Address {
            address: SuiAddress::from(sender),
            checkpoint_viewed_at: self.checkpoint_viewed_at,
        })
    }

    /// The gas input field provides information on what objects were used as gas as well as the
    /// owner of the gas object(s) and information on the gas price and budget.
    ///
    /// If the owner of the gas object(s) is not the same as the sender, the transaction block is a
    /// sponsored transaction block.
    async fn gas_input(&self, ctx: &Context<'_>) -> Option<GasInput> {
        let checkpoint_viewed_at = if matches!(self.inner, TransactionBlockInner::Stored { .. }) {
            self.checkpoint_viewed_at
        } else {
            // Non-stored transactions have a sentinel checkpoint_viewed_at value that generally
            // prevents access to further queries, but inputs should generally be available so try
            // to access them at the high watermark.
            let Watermark { hi_cp, .. } = *ctx.data_unchecked();
            hi_cp
        };

        Some(GasInput::from(
            self.native().gas_data(),
            checkpoint_viewed_at,
        ))
    }

    /// The type of this transaction as well as the commands and/or parameters comprising the
    /// transaction of this kind.
    async fn kind(&self) -> Option<TransactionBlockKind> {
        Some(TransactionBlockKind::from(
            self.native().kind().clone(),
            self.checkpoint_viewed_at,
        ))
    }

    /// A list of all signatures, Base64-encoded, from senders, and potentially the gas owner if
    /// this is a sponsored transaction.
    async fn signatures(&self) -> Option<Vec<Base64>> {
        self.native_signed_data().map(|s| {
            s.tx_signatures()
                .iter()
                .map(|sig| Base64::from(sig.as_ref()))
                .collect()
        })
    }

    /// The effects field captures the results to the chain of executing this transaction.
    async fn effects(&self) -> Result<Option<TransactionBlockEffects>> {
        Ok(Some(self.clone().try_into().extend()?))
    }

    /// This field is set by senders of a transaction block. It is an epoch reference that sets a
    /// deadline after which validators will no longer consider the transaction valid. By default,
    /// there is no deadline for when a transaction must execute.
    async fn expiration(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        let TransactionExpiration::Epoch(id) = self.native().expiration() else {
            return Ok(None);
        };

        Epoch::query(ctx, Some(*id), self.checkpoint_viewed_at)
            .await
            .extend()
    }

    /// Serialized form of this transaction's `TransactionData`, BCS serialized and Base64 encoded.
    async fn bcs(&self) -> Option<Base64> {
        match &self.inner {
            TransactionBlockInner::Stored { native, .. } => Some(Base64::from(
                &bcs::to_bytes(native.transaction_data()).unwrap(),
            )),
            TransactionBlockInner::Executed { tx_data, .. } => Some(Base64::from(
                &bcs::to_bytes(tx_data.transaction_data()).unwrap(),
            )),
            TransactionBlockInner::DryRun { tx_data, .. } => {
                Some(Base64::from(&bcs::to_bytes(tx_data).unwrap()))
            }
        }
    }
}

impl TransactionBlock {
    fn native(&self) -> &NativeTransactionData {
        match &self.inner {
            TransactionBlockInner::Stored { native, .. } => native.transaction_data(),
            TransactionBlockInner::Executed { tx_data, .. } => tx_data.transaction_data(),
            TransactionBlockInner::DryRun { tx_data, .. } => tx_data,
        }
    }

    fn native_signed_data(&self) -> Option<&NativeSenderSignedData> {
        match &self.inner {
            TransactionBlockInner::Stored { native, .. } => Some(native),
            TransactionBlockInner::Executed { tx_data, .. } => Some(tx_data),
            TransactionBlockInner::DryRun { .. } => None,
        }
    }

    /// Look-up the transaction block by its transaction digest.
    pub(crate) fn by_digest(digest: Digest, checkpoint_viewed_at: u64) -> TransactionBlockLookup {
        TransactionBlockLookup::ByDigest {
            digest,
            checkpoint_viewed_at,
        }
    }

    /// Look-up the transaction block by its sequence number (this is not usually exposed through
    /// the GraphQL schema, but internally, othe entities in the DB will refer to transactions at
    /// their sequence number).
    pub(crate) fn by_seq(
        tx_sequence_number: u64,
        checkpoint_viewed_at: u64,
    ) -> TransactionBlockLookup {
        TransactionBlockLookup::BySeq {
            tx_sequence_number,
            checkpoint_viewed_at,
        }
    }

    /// Look up a `TransactionBlock` in the database, by its transaction digest. Treats it as if it
    /// is being viewed at the `checkpoint_viewed_at` (e.g. the state of all relevant addresses will
    /// be at that checkpoint).
    pub(crate) async fn query(
        ctx: &Context<'_>,
        lookup: TransactionBlockLookup,
    ) -> Result<Option<Self>, Error> {
        let DataLoader(loader) = ctx.data_unchecked();

        match lookup {
            TransactionBlockLookup::ByDigest {
                digest,
                checkpoint_viewed_at,
            } => {
                loader
                    .load_one(DigestKey {
                        digest,
                        checkpoint_viewed_at,
                    })
                    .await
            }
            TransactionBlockLookup::BySeq {
                tx_sequence_number,
                checkpoint_viewed_at,
            } => {
                loader
                    .load_one(SeqKey {
                        tx_sequence_number,
                        checkpoint_viewed_at,
                    })
                    .await
            }
        }
    }

    /// Look up multiple `TransactionBlock`s by their digests. Returns a map from those digests to
    /// their resulting transaction blocks, for the blocks that could be found. We return a map
    /// because the order of results from the DB is not otherwise guaranteed to match the order that
    /// digests were passed into `multi_query`.
    pub(crate) async fn multi_query(
        ctx: &Context<'_>,
        digests: Vec<Digest>,
        checkpoint_viewed_at: u64,
    ) -> Result<BTreeMap<Digest, Self>, Error> {
        let DataLoader(loader) = ctx.data_unchecked();
        let result = loader
            .load_many(digests.into_iter().map(|digest| DigestKey {
                digest,
                checkpoint_viewed_at,
            }))
            .await?;

        Ok(result.into_iter().map(|(k, v)| (k.digest, v)).collect())
    }

    /// Query the database for a `page` of TransactionBlocks. The page uses `tx_sequence_number` and
    /// `checkpoint_viewed_at` as the cursor, and can optionally be further `filter`-ed.
    ///
    /// The `checkpoint_viewed_at` parameter represents the checkpoint sequence number at which this
    /// page was queried for. Each entity returned in the connection will inherit this checkpoint,
    /// so that when viewing that entity's state, it will be from the reference of this
    /// checkpoint_viewed_at parameter.
    ///
    /// If the `Page<Cursor>` is set, then this function will defer to the `checkpoint_viewed_at` in
    /// the cursor if they are consistent.
    ///
    /// Filters that involve a combination of `affectedAddress`, `inputObject`, `changedObject`,
    /// and `function` should provide a value for `scan_limit`. This modifies querying behavior by
    /// limiting how many transactions to scan through before applying filters, and also affects
    /// pagination behavior.
    ///
    /// Queries for data that have been pruned will return an empty connection; we treat pruned data
    /// as simply non-existent and thus no error is returned.
    pub(crate) async fn paginate(
        ctx: &Context<'_>,
        page: Page<Cursor>,
        filter: TransactionBlockFilter,
        checkpoint_viewed_at: u64,
        scan_limit: Option<u64>,
    ) -> Result<ScanConnection<String, TransactionBlock>, Error> {
        let limits = &ctx.data_unchecked::<ServiceConfig>().limits;
        let db: &Db = ctx.data_unchecked();
        // If we've entered this function, we already fetched `checkpoint_viewed_at` from the
        // `Watermark`, and so we must be able to retrieve `lo_cp` as well.
        let Watermark { lo_cp, .. } = *ctx.data_unchecked();

        // If the caller has provided some arbitrary combination of `function`, `kind`,
        // `recvAddress`, `inputObject`, or `changedObject`, we require setting a `scanLimit`.
        if let Some(scan_limit) = scan_limit {
            if scan_limit > limits.max_scan_limit as u64 {
                return Err(Error::Client(format!(
                    "Scan limit exceeds max limit of '{}'",
                    limits.max_scan_limit
                )));
            }
        } else if filter.requires_scan_limit() {
            return Err(Error::Client(
                "A scan limit must be specified for the given filter combination".to_string(),
            ));
        }

        if let Some(tx_ids) = &filter.transaction_ids {
            if tx_ids.len() > limits.max_transaction_ids as usize {
                return Err(Error::Client(format!(
                    "Transaction IDs exceed max limit of '{}'",
                    limits.max_transaction_ids
                )));
            }
        }

        let cursor_viewed_at = page.validate_cursor_consistency()?;
        let checkpoint_viewed_at = cursor_viewed_at.unwrap_or(checkpoint_viewed_at);
        let is_from_front = page.is_from_front();
        let cp_after = filter.after_checkpoint.map(u64::from);
        let cp_at = filter.at_checkpoint.map(u64::from);
        let cp_before = filter.before_checkpoint.map(u64::from);

        // If page size or scan limit is 0, we want to standardize behavior by returning an empty
        // connection
        if filter.is_empty() || page.limit() == 0 || scan_limit.is_some_and(|v| v == 0) {
            return Ok(ScanConnection::new(false, false));
        }

        use transactions::dsl as tx;
        let (prev, next, transactions, tx_bounds): (
            bool,
            bool,
            Vec<StoredTransaction>,
            Option<TxBounds>,
        ) = db
            .execute_repeatable(move |conn| {
                async move {
                    let Some(tx_bounds) = TxBounds::query(
                        conn,
                        cp_after,
                        cp_at,
                        cp_before,
                        lo_cp,
                        checkpoint_viewed_at,
                        scan_limit,
                        &page,
                    )
                    .await?
                    else {
                        return Ok::<_, diesel::result::Error>((false, false, Vec::new(), None));
                    };

                    // If no filters are selected, or if the filter is composed of only checkpoint
                    // filters, we can directly query the main `transactions` table. Otherwise, we first
                    // fetch the set of `tx_sequence_number` from a join over relevant lookup tables,
                    // and then issue a query against the `transactions` table to fetch the remaining
                    // contents.
                    let (prev, next, transactions) = if !filter.has_filters() {
                        let (prev, next, iter) = page
                            .paginate_query::<StoredTransaction, _, _, _>(
                                conn,
                                checkpoint_viewed_at,
                                move || {
                                    tx::transactions
                                        .filter(
                                            tx::tx_sequence_number.ge(tx_bounds.scan_lo() as i64),
                                        )
                                        .filter(
                                            tx::tx_sequence_number.lt(tx_bounds.scan_hi() as i64),
                                        )
                                        .into_boxed()
                                },
                            )
                            .await?;

                        (prev, next, iter.collect())
                    } else {
                        let subquery = subqueries(&filter, tx_bounds).unwrap();
                        let (prev, next, results) = page
                            .paginate_raw_query::<TxLookup>(conn, checkpoint_viewed_at, subquery)
                            .await?;

                        let tx_sequence_numbers = results
                            .into_iter()
                            .map(|x| x.tx_sequence_number)
                            .collect::<Vec<i64>>();

                        let transactions = conn
                            .results(move || {
                                tx::transactions.filter(
                                    tx::tx_sequence_number.eq_any(tx_sequence_numbers.clone()),
                                )
                            })
                            .await?;

                        (prev, next, transactions)
                    };

                    Ok::<_, diesel::result::Error>((prev, next, transactions, Some(tx_bounds)))
                }
                .scope_boxed()
            })
            .await?;

        let mut conn = ScanConnection::new(prev, next);

        let Some(tx_bounds) = tx_bounds else {
            return Ok(conn);
        };

        if scan_limit.is_some() {
            apply_scan_limited_pagination(
                &mut conn,
                tx_bounds,
                checkpoint_viewed_at,
                is_from_front,
            );
        }

        for stored in transactions {
            let cursor = stored.cursor(checkpoint_viewed_at).encode_cursor();
            let inner = TransactionBlockInner::try_from(stored)?;
            let transaction = TransactionBlock {
                inner,
                checkpoint_viewed_at,
            };
            conn.edges.push(Edge::new(cursor, transaction));
        }

        Ok(conn)
    }
}

#[async_trait::async_trait]
impl Loader<DigestKey> for Db {
    type Value = TransactionBlock;
    type Error = Error;

    async fn load(
        &self,
        keys: &[DigestKey],
    ) -> Result<HashMap<DigestKey, TransactionBlock>, Error> {
        use transactions::dsl as tx;
        use tx_digests::dsl as ds;

        let digests: Vec<_> = keys.iter().map(|k| k.digest.to_vec()).collect();

        let transactions: Vec<StoredTransaction> = self
            .execute(move |conn| {
                async move {
                    conn.results(move || {
                        let join = ds::tx_sequence_number.eq(tx::tx_sequence_number);

                        tx::transactions
                            .inner_join(ds::tx_digests.on(join))
                            .select(StoredTransaction::as_select())
                            .filter(ds::tx_digest.eq_any(digests.clone()))
                    })
                    .await
                }
                .scope_boxed()
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch transactions: {e}")))?;

        let transaction_digest_to_stored: BTreeMap<_, _> = transactions
            .into_iter()
            .map(|tx| (tx.transaction_digest.clone(), tx))
            .collect();

        let mut results = HashMap::new();
        for key in keys {
            let Some(stored) = transaction_digest_to_stored
                .get(key.digest.as_slice())
                .cloned()
            else {
                continue;
            };

            // Filter by key's checkpoint viewed at here. Doing this in memory because it should be
            // quite rare that this query actually filters something, but encoding it in SQL is
            // complicated.
            if key.checkpoint_viewed_at < stored.checkpoint_sequence_number as u64 {
                continue;
            }

            let inner = TransactionBlockInner::try_from(stored)?;
            results.insert(
                *key,
                TransactionBlock {
                    inner,
                    checkpoint_viewed_at: key.checkpoint_viewed_at,
                },
            );
        }

        Ok(results)
    }
}

#[async_trait::async_trait]
impl Loader<SeqKey> for Db {
    type Value = TransactionBlock;
    type Error = Error;

    async fn load(&self, keys: &[SeqKey]) -> Result<HashMap<SeqKey, TransactionBlock>, Error> {
        use transactions::dsl as tx;

        let seqs: Vec<_> = keys.iter().map(|k| k.tx_sequence_number as i64).collect();

        let transactions: Vec<StoredTransaction> = self
            .execute(move |conn| {
                async move {
                    conn.results(move || {
                        tx::transactions
                            .select(StoredTransaction::as_select())
                            .filter(tx::tx_sequence_number.eq_any(seqs.clone()))
                    })
                    .await
                }
                .scope_boxed()
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch transactions: {e}")))?;

        let seq_to_stored: BTreeMap<_, _> = transactions
            .into_iter()
            .map(|tx| (tx.tx_sequence_number as u64, tx))
            .collect();

        let mut results = HashMap::new();
        for key in keys {
            let Some(stored) = seq_to_stored.get(&key.tx_sequence_number).cloned() else {
                continue;
            };

            // Filter by key's checkpoint viewed at here. Doing this in memory because it should be
            // quite rare that this query actually filters something, but encoding it in SQL is
            // complicated.
            if key.checkpoint_viewed_at < stored.checkpoint_sequence_number as u64 {
                continue;
            }

            let inner = TransactionBlockInner::try_from(stored)?;
            results.insert(
                *key,
                TransactionBlock {
                    inner,
                    checkpoint_viewed_at: key.checkpoint_viewed_at,
                },
            );
        }

        Ok(results)
    }
}

impl TryFrom<StoredTransaction> for TransactionBlockInner {
    type Error = Error;

    fn try_from(stored_tx: StoredTransaction) -> Result<Self, Error> {
        let native = bcs::from_bytes(&stored_tx.raw_transaction)
            .map_err(|e| Error::Internal(format!("Error deserializing transaction block: {e}")))?;

        Ok(TransactionBlockInner::Stored { stored_tx, native })
    }
}

impl TryFrom<TransactionBlockEffects> for TransactionBlock {
    type Error = Error;

    fn try_from(effects: TransactionBlockEffects) -> Result<Self, Error> {
        let checkpoint_viewed_at = effects.checkpoint_viewed_at;
        let inner = match effects.kind {
            TransactionBlockEffectsKind::Stored { stored_tx, .. } => {
                TransactionBlockInner::try_from(stored_tx.clone())
            }
            TransactionBlockEffectsKind::Executed {
                tx_data,
                native,
                events,
            } => Ok(TransactionBlockInner::Executed {
                tx_data: tx_data.clone(),
                effects: native.clone(),
                events: events.clone(),
            }),
            TransactionBlockEffectsKind::DryRun {
                tx_data,
                native,
                events,
            } => Ok(TransactionBlockInner::DryRun {
                tx_data: tx_data.clone(),
                effects: native.clone(),
                events: events.clone(),
            }),
        }?;

        Ok(TransactionBlock {
            inner,
            checkpoint_viewed_at,
        })
    }
}

fn apply_scan_limited_pagination(
    conn: &mut ScanConnection<String, TransactionBlock>,
    tx_bounds: TxBounds,
    checkpoint_viewed_at: u64,
    is_from_front: bool,
) {
    if is_from_front {
        apply_forward_scan_limited_pagination(conn, tx_bounds, checkpoint_viewed_at);
    } else {
        apply_backward_scan_limited_pagination(conn, tx_bounds, checkpoint_viewed_at);
    }
}

/// When paginating forwards on a scan-limited query, the starting cursor and previous page flag
/// will be the first tx scanned in the current window, and whether this window is within the
/// scanning range. The ending cursor and next page flag wraps the last element of the result set if
/// there are more matches in the scanned window that are truncated - if the page size is smaller
/// than the scan limit - but otherwise is expanded out to the last tx scanned.
fn apply_forward_scan_limited_pagination(
    conn: &mut ScanConnection<String, TransactionBlock>,
    tx_bounds: TxBounds,
    checkpoint_viewed_at: u64,
) {
    conn.has_previous_page = tx_bounds.scan_has_prev_page();
    conn.start_cursor = Some(
        Cursor::new(cursor::TransactionBlockCursor {
            checkpoint_viewed_at,
            tx_sequence_number: tx_bounds.scan_start_cursor(),
            is_scan_limited: true,
        })
        .encode_cursor(),
    );

    // There may be more results within the scanned range that got truncated, which occurs when page
    // size is less than `scan_limit`, so only overwrite the end when the base pagination reports no
    // next page.
    if !conn.has_next_page {
        conn.has_next_page = tx_bounds.scan_has_next_page();
        conn.end_cursor = Some(
            Cursor::new(cursor::TransactionBlockCursor {
                checkpoint_viewed_at,
                tx_sequence_number: tx_bounds.scan_end_cursor(),
                is_scan_limited: true,
            })
            .encode_cursor(),
        );
    }
}

/// When paginating backwards on a scan-limited query, the ending cursor and next page flag will be
/// the last tx scanned in the current window, and whether this window is within the scanning range.
/// The starting cursor and previous page flag wraps the first element of the result set if there
/// are more matches in the scanned window that are truncated - if the page size is smaller than the
/// scan limit - but otherwise is expanded out to the first tx scanned.
fn apply_backward_scan_limited_pagination(
    conn: &mut ScanConnection<String, TransactionBlock>,
    tx_bounds: TxBounds,
    checkpoint_viewed_at: u64,
) {
    conn.has_next_page = tx_bounds.scan_has_next_page();
    conn.end_cursor = Some(
        Cursor::new(cursor::TransactionBlockCursor {
            checkpoint_viewed_at,
            tx_sequence_number: tx_bounds.scan_end_cursor(),
            is_scan_limited: true,
        })
        .encode_cursor(),
    );

    // There may be more results within the scanned range that are truncated, especially if page
    // size is less than `scan_limit`, so only overwrite the end when the base pagination reports no
    // next page.
    if !conn.has_previous_page {
        conn.has_previous_page = tx_bounds.scan_has_prev_page();
        conn.start_cursor = Some(
            Cursor::new(cursor::TransactionBlockCursor {
                checkpoint_viewed_at,
                tx_sequence_number: tx_bounds.scan_start_cursor(),
                is_scan_limited: true,
            })
            .encode_cursor(),
        );
    }
}
