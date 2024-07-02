// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    connection::{Connection, CursorType, Edge},
    dataloader::Loader,
    *,
};
use diesel::{
    deserialize::{Queryable, QueryableByName},
    ExpressionMethods, JoinOnDsl, QueryDsl, Selectable, SelectableHelper,
};
use fastcrypto::encoding::{Base58, Encoding};
use paginate::{subqueries, TxBounds};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
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

use crate::{
    consistency::Checkpointed,
    data::{self, DataLoader, Db, DbConnection, QueryExecutor},
    error::Error,
    filter, query,
    raw_query::RawQuery,
    server::watermark_task::Watermark,
    types::intersect,
};

use super::{
    address::Address,
    base64::Base64,
    cursor::{self, Page, Paginated, RawPaginated, Target},
    digest::Digest,
    epoch::Epoch,
    gas::GasInput,
    sui_address::SuiAddress,
    transaction_block_effects::{TransactionBlockEffects, TransactionBlockEffectsKind},
    transaction_block_kind::TransactionBlockKind,
    type_filter::FqNameFilter,
};

use tx_lookups::select_ids;

mod paginate;
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

#[derive(InputObject, Debug, Default, Clone)]
pub(crate) struct TransactionBlockFilter {
    pub function: Option<FqNameFilter>,

    /// An input filter selecting for either system or programmable transactions.
    pub kind: Option<TransactionBlockKindInput>,
    pub after_checkpoint: Option<u64>,
    pub at_checkpoint: Option<u64>,
    pub before_checkpoint: Option<u64>,

    pub sign_address: Option<SuiAddress>,
    pub recv_address: Option<SuiAddress>,

    pub input_object: Option<SuiAddress>,
    pub changed_object: Option<SuiAddress>,

    pub transaction_ids: Option<Vec<Digest>>,
}

pub(crate) type Cursor = cursor::JsonCursor<TransactionBlockCursor>;
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
}

/// DataLoader key for fetching a `TransactionBlock` by its digest, optionally constrained by a
/// consistency cursor.
#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug)]
struct DigestKey {
    pub digest: Digest,
    pub checkpoint_viewed_at: u64,
}

#[derive(Clone, Debug, Queryable, QueryableByName, Selectable)]
#[diesel(table_name = transactions)]
pub struct TxLookup {
    pub tx_sequence_number: i64,
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
    ) -> Result<Connection<String, TransactionBlock>, Error> {
        filter.is_consistent()?;
        let cursor_viewed_at = page.validate_cursor_consistency()?;
        let checkpoint_viewed_at = cursor_viewed_at.unwrap_or(checkpoint_viewed_at);

        let db: &Db = ctx.data_unchecked();

        use transactions::dsl as tx;

        let (prev, next, transactions): (bool, bool, Vec<StoredTransaction>) = db
            .execute_repeatable(move |conn| {
                let tx_bounds = TxBounds::query(
                    conn,
                    filter.after_checkpoint,
                    filter.at_checkpoint,
                    filter.before_checkpoint,
                    checkpoint_viewed_at,
                )?;
                // TODO (wlmyng): adjust per cursor
                // TODO (wlmyng): handle scan_limit and its interaction with has_next_page and has_prev_page
                // if let Some(scan_limit) = scan_limit {
                // if page.is_from_front() {
                // hi_tx = std::cmp::min(hi_tx, lo_tx.saturating_add(scan_limit));
                // } else {
                // lo_tx = std::cmp::max(lo_tx, hi_tx.saturating_sub(scan_limit));
                // }
                // }

                if !filter.has_filters() {
                    let (prev, next, iter) = page.paginate_query::<StoredTransaction, _, _, _>(
                        conn,
                        checkpoint_viewed_at,
                        move || {
                            tx::transactions
                                .filter(tx::tx_sequence_number.ge(tx_bounds.lo))
                                .filter(tx::tx_sequence_number.le(tx_bounds.hi))
                                .into_boxed()
                        },
                    )?;

                    return Ok::<_, diesel::result::Error>((prev, next, iter.collect()));
                };

                let subquery = subqueries(&filter, tx_bounds);

                if let Some(txs) = &filter.transaction_ids {
                    let transaction_ids: Vec<TxLookup> =
                        conn.results(move || select_ids(txs, tx_bounds).into_boxed())?;
                    // TODO (wlmyng) we can adjust has_prev_page and has_next_page based on scan_limit
                    if transaction_ids.is_empty() {
                        return Ok::<_, diesel::result::Error>((false, false, vec![]));
                    }
                    let digest_txs = transaction_ids
                        .into_iter()
                        .map(|x| (x.tx_sequence_number as u64).to_string())
                        .collect::<Vec<String>>()
                        .join(", ");

                    if let Some(subquery) = subquery {
                        let (prev, next, iter) = page.paginate_raw_query::<StoredTransaction>(
                            conn,
                            checkpoint_viewed_at,
                            query!(
                                "{} AND tx_sequence_number IN ({})",
                                filter!(
                                    query!("SELECT * FROM TRANSACTIONS"),
                                    format!("tx_sequence_number IN ({})", digest_txs)
                                ),
                                subquery
                            ),
                        )?;

                        let transactions = iter.collect();
                        return Ok::<_, diesel::result::Error>((prev, next, transactions));
                    }
                } else {
                    // If `transactionIds` were not specified, then there must be at least one
                    // subquery, and thus it should be safe to unwrap. Issue the query to fetch the
                    // set of `tx_sequence_number` that will then be used to fetch remaining
                    // contents from the `transactions` table.
                    let (prev, next, results) = page.paginate_raw_query::<TxLookup>(
                        conn,
                        checkpoint_viewed_at,
                        subquery.unwrap(),
                    )?;

                    let tx_sequence_numbers = results
                        .into_iter()
                        .map(|x| x.tx_sequence_number)
                        .collect::<Vec<i64>>();

                    // then just do a multi-get
                    let transactions = conn.results(move || {
                        tx::transactions
                            .filter(tx::tx_sequence_number.eq_any(tx_sequence_numbers.clone()))
                    })?;

                    return Ok::<_, diesel::result::Error>((prev, next, transactions));
                }

                Ok::<_, diesel::result::Error>((false, false, vec![]))
            })
            .await?;

        let mut conn = Connection::new(prev, next);

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

impl TransactionBlockFilter {
    /// Try to create a filter whose results are the intersection of transaction blocks in `self`'s
    /// results and transaction blocks in `other`'s results. This may not be possible if the
    /// resulting filter is inconsistent in some way (e.g. a filter that requires one field to be
    /// two different values simultaneously).
    pub(crate) fn intersect(self, other: Self) -> Option<Self> {
        macro_rules! intersect {
            ($field:ident, $body:expr) => {
                intersect::field(self.$field, other.$field, $body)
            };
        }

        Some(Self {
            function: intersect!(function, FqNameFilter::intersect)?,
            kind: intersect!(kind, intersect::by_eq)?,

            after_checkpoint: intersect!(after_checkpoint, intersect::by_max)?,
            at_checkpoint: intersect!(at_checkpoint, intersect::by_eq)?,
            before_checkpoint: intersect!(before_checkpoint, intersect::by_min)?,

            sign_address: intersect!(sign_address, intersect::by_eq)?,
            recv_address: intersect!(recv_address, intersect::by_eq)?,
            input_object: intersect!(input_object, intersect::by_eq)?,
            changed_object: intersect!(changed_object, intersect::by_eq)?,

            transaction_ids: intersect!(transaction_ids, |a, b| {
                let a = BTreeSet::from_iter(a.into_iter());
                let b = BTreeSet::from_iter(b.into_iter());
                Some(a.intersection(&b).cloned().collect())
            })?,
        })
    }

    /// A TransactionBlockFilter has complex filters if it has at least one of `function`, `kind`,
    /// `recv_address`, `input_object`, and `changed_object`.
    pub(crate) fn has_complex_filters(&self) -> bool {
        [
            self.function.is_some(),
            self.kind.is_some(),
            self.recv_address.is_some(),
            self.input_object.is_some(),
            self.changed_object.is_some(),
        ]
        .iter()
        .filter(|&is_set| *is_set)
        .count()
            > 0
    }

    /// A TransactionBlockFilter is considered not to have any filters if no filters are specified,
    /// or if the only filters are on `checkpoint`.
    pub(crate) fn has_filters(&self) -> bool {
        self.function.is_some()
            || self.kind.is_some()
            || self.sign_address.is_some()
            || self.recv_address.is_some()
            || self.input_object.is_some()
            || self.changed_object.is_some()
            || self.transaction_ids.is_some()
    }

    pub(crate) fn is_consistent(&self) -> Result<(), Error> {
        if let Some(before) = self.before_checkpoint {
            if before == 0 {
                return Err(Error::Client(
                    "`beforeCheckpoint` must be greater than 0".to_string(),
                ));
            }
        }

        if let (Some(after), Some(before)) = (self.after_checkpoint, self.before_checkpoint) {
            // Because `after` and `before` are both exclusive, they must be at least one apart if
            // both are provided.
            if after + 1 >= before {
                return Err(Error::Client(
                    "`afterCheckpoint` must be less than `beforeCheckpoint`".to_string(),
                ));
            }
        }

        if let (Some(after), Some(at)) = (self.after_checkpoint, self.at_checkpoint) {
            if after >= at {
                return Err(Error::Client(
                    "`afterCheckpoint` must be less than `atCheckpoint`".to_string(),
                ));
            }
        }

        if let (Some(at), Some(before)) = (self.at_checkpoint, self.before_checkpoint) {
            if at >= before {
                return Err(Error::Client(
                    "`atCheckpoint` must be less than `beforeCheckpoint`".to_string(),
                ));
            }
        }

        if let (Some(TransactionBlockKindInput::SystemTx), Some(signer)) =
            (self.kind, self.sign_address)
        {
            if signer != SuiAddress::from(NativeSuiAddress::ZERO) {
                return Err(Error::Client(
                    "System transactions cannot have a sender".to_string(),
                ));
            }
        }

        Ok(())
    }
}

impl Paginated<Cursor> for StoredTransaction {
    type Source = transactions::table;

    fn filter_ge<ST, GB>(cursor: &Cursor, query: Query<ST, GB>) -> Query<ST, GB> {
        query.filter(transactions::dsl::tx_sequence_number.ge(cursor.tx_sequence_number as i64))
    }

    fn filter_le<ST, GB>(cursor: &Cursor, query: Query<ST, GB>) -> Query<ST, GB> {
        query.filter(transactions::dsl::tx_sequence_number.le(cursor.tx_sequence_number as i64))
    }

    fn order<ST, GB>(asc: bool, query: Query<ST, GB>) -> Query<ST, GB> {
        use transactions::dsl;
        if asc {
            query.order_by(dsl::tx_sequence_number.asc())
        } else {
            query.order_by(dsl::tx_sequence_number.desc())
        }
    }
}

impl Target<Cursor> for StoredTransaction {
    fn cursor(&self, checkpoint_viewed_at: u64) -> Cursor {
        Cursor::new(TransactionBlockCursor {
            tx_sequence_number: self.tx_sequence_number as u64,
            checkpoint_viewed_at,
        })
    }
}

impl Checkpointed for Cursor {
    fn checkpoint_viewed_at(&self) -> u64 {
        self.checkpoint_viewed_at
    }
}

impl Target<Cursor> for TxLookup {
    fn cursor(&self, checkpoint_viewed_at: u64) -> Cursor {
        Cursor::new(TransactionBlockCursor {
            tx_sequence_number: self.tx_sequence_number as u64,
            checkpoint_viewed_at,
        })
    }
}

impl RawPaginated<Cursor> for TxLookup {
    fn filter_ge(cursor: &Cursor, query: RawQuery) -> RawQuery {
        filter!(
            query,
            format!("tx_sequence_number >= {}", cursor.tx_sequence_number)
        )
    }

    fn filter_le(cursor: &Cursor, query: RawQuery) -> RawQuery {
        filter!(
            query,
            format!("tx_sequence_number <= {}", cursor.tx_sequence_number)
        )
    }

    fn order(asc: bool, query: RawQuery) -> RawQuery {
        if asc {
            query.order_by("tx_sequence_number ASC")
        } else {
            query.order_by("tx_sequence_number DESC")
        }
    }
}

impl RawPaginated<Cursor> for StoredTransaction {
    fn filter_ge(cursor: &Cursor, query: RawQuery) -> RawQuery {
        filter!(
            query,
            format!("tx_sequence_number >= {}", cursor.tx_sequence_number)
        )
    }

    fn filter_le(cursor: &Cursor, query: RawQuery) -> RawQuery {
        filter!(
            query,
            format!("tx_sequence_number <= {}", cursor.tx_sequence_number)
        )
    }

    fn order(asc: bool, query: RawQuery) -> RawQuery {
        if asc {
            query.order_by("tx_sequence_number ASC")
        } else {
            query.order_by("tx_sequence_number DESC")
        }
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
