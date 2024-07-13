// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{connection::CursorType, dataloader::Loader, *};
use connection::Edge;
use diesel::{ExpressionMethods, JoinOnDsl, QueryDsl, SelectableHelper};
use fastcrypto::encoding::{Base58, Encoding};
pub(crate) use filter::TransactionBlockFilter;
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
pub(crate) use tx_cursor::Cursor;
use tx_cursor::TxLookup;
pub(crate) use tx_lookups::{subqueries, TxBounds};

use crate::{
    config::ServiceConfig,
    connection::ScanConnection,
    data::{self, DataLoader, Db, DbConnection, QueryExecutor},
    error::Error,
    server::watermark_task::Watermark,
};

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

mod filter;
mod tx_cursor;
mod tx_lookups;

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

type Query<ST, GB> = data::Query<ST, transactions::table, GB>;

/// DataLoader key for fetching a `TransactionBlock` by its digest, optionally constrained by a
/// consistency cursor.
#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug)]
struct DigestKey {
    pub digest: Digest,
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
            let Watermark { checkpoint, .. } = *ctx.data_unchecked();
            checkpoint
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

    /// Serialized form of this transaction's `SenderSignedData`, BCS serialized and Base64 encoded.
    async fn bcs(&self) -> Option<Base64> {
        match &self.inner {
            TransactionBlockInner::Stored { stored_tx, .. } => {
                Some(Base64::from(&stored_tx.raw_transaction))
            }
            TransactionBlockInner::Executed { tx_data, .. } => {
                bcs::to_bytes(&tx_data).ok().map(Base64::from)
            }
            // Dry run transaction does not have signatures so no sender signed data.
            TransactionBlockInner::DryRun { .. } => None,
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

    /// Look up a `TransactionBlock` in the database, by its transaction digest. Treats it as if it
    /// is being viewed at the `checkpoint_viewed_at` (e.g. the state of all relevant addresses will
    /// be at that checkpoint).
    pub(crate) async fn query(
        ctx: &Context<'_>,
        digest: Digest,
        checkpoint_viewed_at: u64,
    ) -> Result<Option<Self>, Error> {
        let DataLoader(loader) = ctx.data_unchecked();
        loader
            .load_one(DigestKey {
                digest,
                checkpoint_viewed_at,
            })
            .await
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
    /// Filters that involve a combination of `recvAddress`, `inputObject`, `changedObject`, and
    /// `function` should provide a value for `scan_limit`. This indicates how many transactions to
    /// scan through per the filter conditions.
    pub(crate) async fn paginate(
        ctx: &Context<'_>,
        page: Page<Cursor>,
        filter: TransactionBlockFilter,
        checkpoint_viewed_at: u64,
        scan_limit: Option<u64>,
    ) -> Result<ScanConnection<String, TransactionBlock>, Error> {
        if filter.is_empty() {
            return Ok(ScanConnection::new(false, false));
        }

        let limits = ctx.data_unchecked::<ServiceConfig>().limits;

        // If there is more than one `complex_filter` specified, then the caller has provided some
        // arbitrary combination of `function`, `kind`, `recvAddress`, `inputObject`, or
        // `changedObject`. Consequently, we require setting a `scanLimit`, or else we will return
        // an error.
        if filter.requires_scan_limit() {
            if let Some(scan_limit) = scan_limit {
                if scan_limit >= limits.max_scan_limit as u64 {
                    return Err(Error::Client(format!(
                        "Scan limit exceeds max limit of '{}'",
                        limits.max_scan_limit
                    )));
                }
            } else {
                return Err(Error::Client(
                    "A scan limit must be specified for the given filter combination".to_string(),
                ));
            }
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
        let db: &Db = ctx.data_unchecked();

        use transactions::dsl as tx;

        let (prev, next, before, after, transactions, tx_bounds): (
            bool,
            bool,
            Option<u64>,
            Option<u64>,
            Vec<StoredTransaction>,
            Option<TxBounds>,
        ) = db
            .execute_repeatable(move |conn| {
                let Some(tx_bounds) = TxBounds::query(
                    conn,
                    filter.after_checkpoint,
                    filter.at_checkpoint,
                    filter.before_checkpoint,
                    checkpoint_viewed_at,
                    scan_limit,
                    &page,
                )?
                else {
                    return Ok::<_, diesel::result::Error>((
                        false,
                        false,
                        None,
                        None,
                        Vec::new(),
                        None,
                    ));
                };

                // If no filters are selected, or if the filter is composed of only checkpoint
                // filters, we can directly query the main `transactions` table. Otherwise, we first
                // fetch the set of `tx_sequence_number` from a join over relevant lookup tables,
                // and then issue a query against the `transactions` table to fetch the remaining
                // contents.
                let (prev, next, before, after, transactions) = if !filter.has_filters() {
                    let (prev, next, iter) = page.paginate_query::<StoredTransaction, _, _, _>(
                        conn,
                        checkpoint_viewed_at,
                        move || {
                            tx::transactions
                                .filter(tx::tx_sequence_number.ge(tx_bounds.scan_lo() as i64))
                                .filter(tx::tx_sequence_number.le(tx_bounds.scan_hi() as i64))
                                .into_boxed()
                        },
                    )?;

                    (prev, next, None, None, iter.collect())
                } else {
                    let subquery = subqueries(&filter, tx_bounds).unwrap();
                    let (prev, next, before, after, results) = page
                        .paginate_raw_query_with_scan_limit::<TxLookup>(
                            conn,
                            checkpoint_viewed_at,
                            tx_bounds,
                            subquery,
                        )?;

                    let tx_sequence_numbers = results
                        .into_iter()
                        .map(|x| x.tx_sequence_number)
                        .collect::<Vec<i64>>();

                    let transactions = conn.results(move || {
                        tx::transactions
                            .filter(tx::tx_sequence_number.eq_any(tx_sequence_numbers.clone()))
                    })?;

                    (prev, next, before, after, transactions)
                };

                Ok::<_, diesel::result::Error>((
                    prev,
                    next,
                    before,
                    after,
                    transactions,
                    Some(tx_bounds),
                ))
            })
            .await?;

        let mut conn = ScanConnection::new(prev, next);

        if tx_bounds.is_none() {
            return Ok(conn);
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

        // If there is a derived before and/ or after, it means the page is empty. This occurs when
        // `scan_limit` is specified. Even though there are no matches on the page of candidates, we
        // can continue to paginate forwards or backwards with the derived cursor.
        if let Some(before) = before {
            conn.has_previous_page = true;
            conn.start_cursor = Some(
                Cursor::new(tx_cursor::TransactionBlockCursor {
                    checkpoint_viewed_at,
                    tx_sequence_number: before,
                })
                .encode_cursor(),
            );
        }
        if let Some(after) = after {
            conn.has_next_page = true;
            conn.end_cursor = Some(
                Cursor::new(tx_cursor::TransactionBlockCursor {
                    checkpoint_viewed_at,
                    tx_sequence_number: after,
                })
                .encode_cursor(),
            );
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
                conn.results(move || {
                    let join = ds::tx_sequence_number.eq(tx::tx_sequence_number);

                    tx::transactions
                        .inner_join(ds::tx_digests.on(join))
                        .select(StoredTransaction::as_select())
                        .filter(ds::tx_digest.eq_any(digests.clone()))
                })
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
