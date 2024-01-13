use std::collections::{BTreeMap, BTreeSet};

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use async_graphql::{
    connection::{Connection, CursorType, Edge},
    *,
};
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl};
use fastcrypto::encoding::{Base58, Encoding};
use sui_indexer::{
    models_v2::transactions::StoredTransaction,
    schema_v2::{
        transactions, tx_calls, tx_changed_objects, tx_input_objects, tx_recipients, tx_senders,
    },
};
use sui_types::{
    base_types::SuiAddress as NativeSuiAddress,
    transaction::{
        SenderSignedData as NativeSenderSignedData, TransactionDataAPI, TransactionExpiration,
    },
};

use crate::{
    data::{self, Db, DbConnection, QueryExecutor},
    error::Error,
};

use super::{
    address::Address,
    base64::Base64,
    cursor::{self, Page, Target},
    digest::Digest,
    epoch::Epoch,
    event::Event,
    gas::GasInput,
    sui_address::SuiAddress,
    transaction_block_effects::TransactionBlockEffects,
    transaction_block_kind::TransactionBlockKind,
    type_filter::FqNameFilter,
};

#[derive(Clone)]
pub(crate) struct TransactionBlock {
    /// Representation of transaction data in the Indexer's Store. The indexer stores the
    /// transaction data and its effects together, in one table.
    pub stored: StoredTransaction,

    /// Deserialized representation of `stored.raw_transaction`.
    pub native: NativeSenderSignedData,
}

/// An input filter selecting for either system or programmable transactions.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum TransactionBlockKindInput {
    /// A system transaction can be one of several types of transactions.
    /// See [unions/transaction-block-kind] for more details.
    SystemTx = 0,
    /// A user submitted transaction block
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

pub(crate) type Cursor = cursor::Cursor<u64>;
type CEvent = cursor::Cursor<usize>;
type Query<ST, GB> = data::Query<ST, transactions::table, GB>;

#[Object]
impl TransactionBlock {
    /// A 32-byte hash that uniquely identifies the transaction block contents, encoded in Base58.
    /// This serves as a unique id for the block on chain.
    async fn digest(&self) -> String {
        Base58::encode(&self.stored.transaction_digest)
    }

    /// The address corresponding to the public key that signed this transaction. System
    /// transactions do not have senders.
    async fn sender(&self) -> Option<Address> {
        let sender = self.native.transaction_data().sender();
        (sender != NativeSuiAddress::ZERO).then(|| Address {
            address: SuiAddress::from(sender),
        })
    }

    /// The gas input field provides information on what objects were used as gas as well as the
    /// owner of the gas object(s) and information on the gas price and budget.
    ///
    /// If the owner of the gas object(s) is not the same as the sender, the transaction block is a
    /// sponsored transaction block.
    async fn gas_input(&self) -> Option<GasInput> {
        Some(GasInput::from(self.native.transaction_data().gas_data()))
    }

    /// The type of this transaction as well as the commands and/or parameters comprising the
    /// transaction of this kind.
    async fn kind(&self) -> Option<TransactionBlockKind> {
        Some(TransactionBlockKind::from(
            self.native.transaction_data().kind().clone(),
        ))
    }

    /// A list of all signatures, Base64-encoded, from senders, and potentially the gas owner if
    /// this is a sponsored transaction.
    async fn signatures(&self) -> Option<Vec<Base64>> {
        Some(
            self.native
                .tx_signatures()
                .iter()
                .map(|s| Base64::from(s.as_ref()))
                .collect(),
        )
    }

    /// The effects field captures the results to the chain of executing this transaction.
    async fn effects(&self) -> Result<Option<TransactionBlockEffects>> {
        Ok(Some(
            TransactionBlockEffects::try_from(self.stored.clone()).extend()?,
        ))
    }

    /// Events emitted by this transaction block.
    async fn events(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CEvent>,
        last: Option<u64>,
        before: Option<CEvent>,
    ) -> Result<Connection<String, Event>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        let mut connection = Connection::new(false, false);
        let Some((prev, next, cs)) = page.paginate_indices(self.stored.events.len()) else {
            return Ok(connection);
        };

        connection.has_previous_page = prev;
        connection.has_next_page = next;

        for c in cs {
            let event = Event::try_from_stored_transaction(&self.stored, *c).extend()?;
            connection.edges.push(Edge::new(c.encode_cursor(), event));
        }

        Ok(connection)
    }

    /// This field is set by senders of a transaction block. It is an epoch reference that sets a
    /// deadline after which validators will no longer consider the transaction valid. By default,
    /// there is no deadline for when a transaction must execute.
    async fn expiration(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        let TransactionExpiration::Epoch(id) = self.native.transaction_data().expiration() else {
            return Ok(None);
        };

        Epoch::query(ctx.data_unchecked(), Some(*id)).await.extend()
    }

    /// Serialized form of this transaction's `SenderSignedData`, BCS serialized and Base64 encoded.
    async fn bcs(&self) -> Option<Base64> {
        Some(Base64::from(&self.stored.raw_transaction))
    }
}

impl TransactionBlock {
    /// Look up a `TransactionBlock` in the database, by its transaction digest.
    pub(crate) async fn query(db: &Db, digest: Digest) -> Result<Option<Self>, Error> {
        use transactions::dsl;

        let stored: Option<StoredTransaction> = db
            .execute(move |conn| {
                conn.result(move || {
                    dsl::transactions.filter(dsl::transaction_digest.eq(digest.to_vec()))
                })
                .optional()
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch transaction: {e}")))?;

        stored.map(TransactionBlock::try_from).transpose()
    }

    /// Look up multiple `TransactionBlock`s by their digests. Returns a map from those digests to
    /// their resulting transaction blocks, for the blocks that could be found. We return a map
    /// because the order of results from the DB is not otherwise guaranteed to match the order that
    /// digests were passed into `multi_query`.
    pub(crate) async fn multi_query(
        db: &Db,
        digests: Vec<Digest>,
    ) -> Result<BTreeMap<Digest, Self>, Error> {
        use transactions::dsl;
        let digests: Vec<_> = digests.into_iter().map(|d| d.to_vec()).collect();

        let stored: Vec<StoredTransaction> = db
            .execute(move |conn| {
                conn.results(move || {
                    dsl::transactions.filter(dsl::transaction_digest.eq_any(digests.clone()))
                })
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch transactions: {e}")))?;

        let mut transactions = BTreeMap::new();
        for tx in stored {
            let digest = Digest::try_from(&tx.transaction_digest[..])
                .map_err(|e| Error::Internal(format!("Bad digest for transaction: {e}")))?;

            let transaction = TransactionBlock::try_from(tx)?;
            transactions.insert(digest, transaction);
        }

        Ok(transactions)
    }

    pub(crate) async fn paginate(
        db: &Db,
        page: Page<u64>,
        filter: TransactionBlockFilter,
    ) -> Result<Connection<String, TransactionBlock>, Error> {
        let (prev, next, results) = db
            .execute(move |conn| {
                page.paginate_query::<StoredTransaction, _, _, _>(conn, move || {
                    use transactions as tx;
                    let mut query = tx::dsl::transactions.into_boxed();

                    if let Some(f) = &filter.function {
                        let sub_query = tx_calls::dsl::tx_calls
                            .select(tx_calls::dsl::tx_sequence_number)
                            .into_boxed();

                        query = query.filter(tx::dsl::tx_sequence_number.eq_any(f.apply(
                            sub_query,
                            tx_calls::dsl::package,
                            tx_calls::dsl::module,
                            tx_calls::dsl::func,
                        )));
                    }

                    if let Some(k) = &filter.kind {
                        query = query.filter(tx::dsl::transaction_kind.eq(*k as i16))
                    }

                    if let Some(c) = &filter.after_checkpoint {
                        query = query.filter(tx::dsl::checkpoint_sequence_number.gt(*c as i64));
                    }

                    if let Some(c) = &filter.at_checkpoint {
                        query = query.filter(tx::dsl::checkpoint_sequence_number.eq(*c as i64));
                    }

                    if let Some(c) = &filter.before_checkpoint {
                        query = query.filter(tx::dsl::checkpoint_sequence_number.lt(*c as i64));
                    }

                    if let Some(a) = &filter.sign_address {
                        let sub_query = tx_senders::dsl::tx_senders
                            .select(tx_senders::dsl::tx_sequence_number)
                            .filter(tx_senders::dsl::sender.eq(a.into_vec()));
                        query = query.filter(tx::dsl::tx_sequence_number.eq_any(sub_query));
                    }

                    if let Some(a) = &filter.recv_address {
                        let sub_query = tx_recipients::dsl::tx_recipients
                            .select(tx_recipients::dsl::tx_sequence_number)
                            .filter(tx_recipients::dsl::recipient.eq(a.into_vec()));
                        query = query.filter(tx::dsl::tx_sequence_number.eq_any(sub_query));
                    }

                    if let Some(o) = &filter.input_object {
                        let sub_query = tx_input_objects::dsl::tx_input_objects
                            .select(tx_input_objects::dsl::tx_sequence_number)
                            .filter(tx_input_objects::dsl::object_id.eq(o.into_vec()));
                        query = query.filter(tx::dsl::tx_sequence_number.eq_any(sub_query));
                    }

                    if let Some(o) = &filter.changed_object {
                        let sub_query = tx_changed_objects::dsl::tx_changed_objects
                            .select(tx_changed_objects::dsl::tx_sequence_number)
                            .filter(tx_changed_objects::dsl::object_id.eq(o.into_vec()));
                        query = query.filter(tx::dsl::tx_sequence_number.eq_any(sub_query));
                    }

                    if let Some(txs) = &filter.transaction_ids {
                        let digests: Vec<_> = txs.iter().map(|d| d.to_vec()).collect();
                        query = query.filter(tx::dsl::transaction_digest.eq_any(digests));
                    }

                    query
                })
            })
            .await?;

        let mut conn = Connection::new(prev, next);

        for stored in results {
            let cursor = Cursor::new(stored.cursor()).encode_cursor();
            let transaction = TransactionBlock::try_from(stored)?;
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
        fn by_eq<T: Eq>(a: T, b: T) -> Option<T> {
            (a == b).then_some(a)
        }

        fn by_max<T: Ord>(a: T, b: T) -> Option<T> {
            Some(a.max(b))
        }

        fn by_min<T: Ord>(a: T, b: T) -> Option<T> {
            Some(a.min(b))
        }

        macro_rules! merge {
            ($field:ident, $body:expr) => {
                merge_filter(self.$field, other.$field, $body)
            };
        }

        Some(Self {
            function: merge!(function, by_eq)?,
            kind: merge!(kind, by_eq)?,

            after_checkpoint: merge!(after_checkpoint, by_max)?,
            at_checkpoint: merge!(at_checkpoint, by_eq)?,
            before_checkpoint: merge!(before_checkpoint, by_min)?,

            sign_address: merge!(sign_address, by_eq)?,
            recv_address: merge!(recv_address, by_eq)?,
            input_object: merge!(input_object, by_eq)?,
            changed_object: merge!(changed_object, by_eq)?,

            transaction_ids: merge!(transaction_ids, |a, b| {
                let a = BTreeSet::from_iter(a.into_iter());
                let b = BTreeSet::from_iter(b.into_iter());
                Some(a.intersection(&b).cloned().collect())
            })?,
        })
    }
}

impl Target<u64> for StoredTransaction {
    type Source = transactions::table;

    fn filter_ge<ST, GB>(cursor: &u64, query: Query<ST, GB>) -> Query<ST, GB> {
        query.filter(transactions::dsl::tx_sequence_number.ge(*cursor as i64))
    }

    fn filter_le<ST, GB>(cursor: &u64, query: Query<ST, GB>) -> Query<ST, GB> {
        query.filter(transactions::dsl::tx_sequence_number.le(*cursor as i64))
    }

    fn order<ST, GB>(asc: bool, query: Query<ST, GB>) -> Query<ST, GB> {
        use transactions::dsl;
        if asc {
            query.order_by(dsl::tx_sequence_number.asc())
        } else {
            query.order_by(dsl::tx_sequence_number.desc())
        }
    }

    fn cursor(&self) -> u64 {
        self.tx_sequence_number as u64
    }
}

impl TryFrom<StoredTransaction> for TransactionBlock {
    type Error = Error;

    fn try_from(stored: StoredTransaction) -> Result<Self, Error> {
        let native = bcs::from_bytes(&stored.raw_transaction)
            .map_err(|e| Error::Internal(format!("Error deserializing transaction block: {e}")))?;

        Ok(TransactionBlock { stored, native })
    }
}

/// Merges two optional filter values. If both values exist, `merge` is used to combine them, which
/// returns some combined value if there is some consistent combination, and `None` otherwise. The
/// overall function returns `Some(None)`, if the filters combined to no filter, `Some(Some(f))` if
/// the filters combined to `f`, and `None` if the filters couldn't be combined.
fn merge_filter<T>(
    this: Option<T>,
    that: Option<T>,
    merge: impl FnOnce(T, T) -> Option<T>,
) -> Option<Option<T>> {
    match (this, that) {
        (None, None) => Some(None),
        (Some(this), None) => Some(Some(this)),
        (None, Some(that)) => Some(Some(that)),
        (Some(this), Some(that)) => merge(this, that).map(Some),
    }
}
