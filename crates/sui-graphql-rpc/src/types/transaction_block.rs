use std::collections::{BTreeMap, BTreeSet};

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use async_graphql::{
    connection::{Connection, CursorType, Edge},
    *,
};
use diesel::{alias, ExpressionMethods, NullableExpressionMethods, OptionalExtension, QueryDsl};
use fastcrypto::encoding::{Base58, Encoding};
use sui_indexer::{
    models_v2::transactions::StoredTransaction,
    schema_v2::{
        transactions, tx_calls, tx_changed_objects, tx_input_objects, tx_recipients, tx_senders,
    },
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
    data::{self, Db, DbConnection, QueryExecutor},
    error::Error,
    types::intersect,
};

use super::{
    address::Address,
    base64::Base64,
    cursor::{self, Page, Paginated, Target},
    digest::Digest,
    epoch::Epoch,
    gas::GasInput,
    sui_address::SuiAddress,
    transaction_block_effects::TransactionBlockEffects,
    transaction_block_kind::TransactionBlockKind,
    type_filter::FqNameFilter,
};

#[derive(Clone, Debug)]
pub(crate) enum TransactionBlock {
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

pub(crate) type Cursor = cursor::JsonCursor<u64>;
type Query<ST, GB> = data::Query<ST, transactions::table, GB>;

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

        let checkpoint_sequence_number = match self {
            TransactionBlock::Stored { stored_tx, .. } => {
                Some(stored_tx.checkpoint_sequence_number as u64)
            }
            _ => None,
        };

        (sender != NativeSuiAddress::ZERO).then(|| Address {
            address: SuiAddress::from(sender),
            checkpoint_sequence_number,
        })
    }

    /// The gas input field provides information on what objects were used as gas as well as the
    /// owner of the gas object(s) and information on the gas price and budget.
    ///
    /// If the owner of the gas object(s) is not the same as the sender, the transaction block is a
    /// sponsored transaction block.
    async fn gas_input(&self) -> Option<GasInput> {
        let checkpoint_sequence_number = match self {
            TransactionBlock::Stored { stored_tx, .. } => {
                Some(stored_tx.checkpoint_sequence_number as u64)
            }
            _ => None,
        };

        Some(GasInput::from(
            self.native().gas_data(),
            checkpoint_sequence_number,
        ))
    }

    /// The type of this transaction as well as the commands and/or parameters comprising the
    /// transaction of this kind.
    async fn kind(&self) -> Option<TransactionBlockKind> {
        Some(TransactionBlockKind::from(self.native().kind().clone()))
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

        Epoch::query(ctx.data_unchecked(), Some(*id)).await.extend()
    }

    /// Serialized form of this transaction's `SenderSignedData`, BCS serialized and Base64 encoded.
    async fn bcs(&self) -> Option<Base64> {
        match self {
            TransactionBlock::Stored { stored_tx, .. } => {
                Some(Base64::from(&stored_tx.raw_transaction))
            }
            TransactionBlock::Executed { tx_data, .. } => {
                bcs::to_bytes(&tx_data).ok().map(Base64::from)
            }
            // Dry run transaction does not have signatures so no sender signed data.
            TransactionBlock::DryRun { .. } => None,
        }
    }
}

impl TransactionBlock {
    fn native(&self) -> &NativeTransactionData {
        match self {
            TransactionBlock::Stored { native, .. } => native.transaction_data(),
            TransactionBlock::Executed { tx_data, .. } => tx_data.transaction_data(),
            TransactionBlock::DryRun { tx_data, .. } => tx_data,
        }
    }

    fn native_signed_data(&self) -> Option<&NativeSenderSignedData> {
        match self {
            TransactionBlock::Stored { native, .. } => Some(native),
            TransactionBlock::Executed { tx_data, .. } => Some(tx_data),
            TransactionBlock::DryRun { .. } => None,
        }
    }

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
        page: Page<Cursor>,
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
                        // Translate bound on checkpoint number into a bound on transaction sequence
                        // number to make better use of indices. Experimentally, postgres struggles
                        // to use the index on checkpoint sequence number to handle inequality
                        // constraints -- it still uses the index on transaction sequence number --
                        // but it's fine to use that index on an equality query.
                        //
                        // Diesel also does not like the same table appearing multiple times in a
                        // single query, so we create an alias of the `transactions` table to query
                        // for the transaction sequence number bound.
                        let tx_ = alias!(tx as tx_after);
                        let sub_query = tx_
                            .select(tx_.field(tx::dsl::tx_sequence_number))
                            .filter(tx_.field(tx::dsl::checkpoint_sequence_number).ge(*c as i64))
                            .order(tx_.field(tx::dsl::tx_sequence_number).desc())
                            .limit(1);

                        query = query.filter(
                            tx::dsl::tx_sequence_number
                                .nullable()
                                .gt(sub_query.single_value()),
                        );
                    }

                    if let Some(c) = &filter.at_checkpoint {
                        query = query.filter(tx::dsl::checkpoint_sequence_number.eq(*c as i64));
                    }

                    if let Some(c) = &filter.before_checkpoint {
                        // See comment on handling `after_checkpoint` filter (above) for context.
                        let tx_ = alias!(tx as tx_before);
                        let sub_query = tx_
                            .select(tx_.field(tx::dsl::tx_sequence_number))
                            .filter(tx_.field(tx::dsl::checkpoint_sequence_number).le(*c as i64))
                            .order(tx_.field(tx::dsl::tx_sequence_number).asc())
                            .limit(1);

                        query = query.filter(
                            tx::dsl::tx_sequence_number
                                .nullable()
                                .lt(sub_query.single_value()),
                        );
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
            let cursor = stored.cursor().encode_cursor();
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
}

impl Paginated<Cursor> for StoredTransaction {
    type Source = transactions::table;

    fn filter_ge<ST, GB>(cursor: &Cursor, query: Query<ST, GB>) -> Query<ST, GB> {
        query.filter(transactions::dsl::tx_sequence_number.ge(**cursor as i64))
    }

    fn filter_le<ST, GB>(cursor: &Cursor, query: Query<ST, GB>) -> Query<ST, GB> {
        query.filter(transactions::dsl::tx_sequence_number.le(**cursor as i64))
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
    fn cursor(&self) -> Cursor {
        Cursor::new(self.tx_sequence_number as u64)
    }
}

impl TryFrom<StoredTransaction> for TransactionBlock {
    type Error = Error;

    fn try_from(stored_tx: StoredTransaction) -> Result<Self, Error> {
        let native = bcs::from_bytes(&stored_tx.raw_transaction)
            .map_err(|e| Error::Internal(format!("Error deserializing transaction block: {e}")))?;

        Ok(TransactionBlock::Stored { stored_tx, native })
    }
}

impl TryFrom<TransactionBlockEffects> for TransactionBlock {
    type Error = Error;

    fn try_from(effects: TransactionBlockEffects) -> Result<Self, Error> {
        match effects {
            TransactionBlockEffects::Stored { stored_tx, .. } => {
                TransactionBlock::try_from(stored_tx.clone())
            }
            TransactionBlockEffects::Executed {
                tx_data,
                native,
                events,
            } => Ok(TransactionBlock::Executed {
                tx_data: tx_data.clone(),
                effects: native.clone(),
                events: events.clone(),
            }),
            TransactionBlockEffects::DryRun {
                tx_data,
                native,
                events,
            } => Ok(TransactionBlock::DryRun {
                tx_data: tx_data.clone(),
                effects: native.clone(),
                events: events.clone(),
            }),
        }
    }
}
