use std::collections::{BTreeMap, BTreeSet};

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use async_graphql::{
    connection::{Connection, CursorType, Edge},
    *,
};
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl};
use fastcrypto::encoding::{Base58, Encoding};
use serde::{Deserialize, Serialize};
use sui_indexer::{
    models::transactions::StoredTransaction,
    schema::{
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
    consistency::Checkpointed,
    data::{self, Db, DbConnection, QueryExecutor},
    error::Error,
    types::intersect,
};

use super::{
    address::Address,
    base64::Base64,
    checkpoint::Checkpoint,
    cursor::{self, Page, Paginated, Target},
    digest::Digest,
    epoch::Epoch,
    gas::GasInput,
    sui_address::SuiAddress,
    transaction_block_effects::{TransactionBlockEffects, TransactionBlockEffectsKind},
    transaction_block_kind::TransactionBlockKind,
    type_filter::FqNameFilter,
};

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
    /// The checkpoint sequence number when the transaction was finalized.
    #[serde(rename = "tc")]
    pub tx_checkpoint_number: u64,
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
            checkpoint_viewed_at: Some(self.checkpoint_viewed_at),
        })
    }

    /// The gas input field provides information on what objects were used as gas as well as the
    /// owner of the gas object(s) and information on the gas price and budget.
    ///
    /// If the owner of the gas object(s) is not the same as the sender, the transaction block is a
    /// sponsored transaction block.
    async fn gas_input(&self) -> Option<GasInput> {
        let checkpoint_sequence_number = match &self.inner {
            TransactionBlockInner::Stored { stored_tx, .. } => {
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

        Epoch::query(
            ctx.data_unchecked(),
            Some(*id),
            Some(self.checkpoint_viewed_at),
        )
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

    /// Look up a `TransactionBlock` in the database, by its transaction digest. If
    /// `checkpoint_viewed_at` is provided, the transaction block will inherit the value. Otherwise,
    /// it will be set to the upper bound of the available range at the time of the query.
    pub(crate) async fn query(
        db: &Db,
        digest: Digest,
        checkpoint_viewed_at: Option<u64>,
    ) -> Result<Option<Self>, Error> {
        use transactions::dsl;

        let (stored, checkpoint_viewed_at): (Option<StoredTransaction>, u64) = db
            .execute_repeatable(move |conn| {
                let checkpoint_viewed_at = match checkpoint_viewed_at {
                    Some(value) => Ok(value),
                    None => Checkpoint::available_range(conn).map(|(_, rhs)| rhs),
                }?;

                let stored = conn
                    .result(move || {
                        dsl::transactions.filter(dsl::transaction_digest.eq(digest.to_vec()))
                    })
                    .optional()?;

                Ok::<_, diesel::result::Error>((stored, checkpoint_viewed_at))
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch transaction: {e}")))?;

        let Some(stored) = stored else {
            return Ok(None);
        };

        let inner = TransactionBlockInner::try_from(stored)?;
        Ok(Some(TransactionBlock {
            inner,
            checkpoint_viewed_at,
        }))
    }

    /// Look up multiple `TransactionBlock`s by their digests. Returns a map from those digests to
    /// their resulting transaction blocks, for the blocks that could be found. We return a map
    /// because the order of results from the DB is not otherwise guaranteed to match the order that
    /// digests were passed into `multi_query`.
    pub(crate) async fn multi_query(
        db: &Db,
        digests: Vec<Digest>,
        checkpoint_viewed_at: u64,
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

            let inner = TransactionBlockInner::try_from(tx)?;
            let transaction = TransactionBlock {
                inner,
                checkpoint_viewed_at,
            };
            transactions.insert(digest, transaction);
        }

        Ok(transactions)
    }

    /// Query the database for a `page` of TransactionBlocks. The page uses `tx_sequence_number` and
    /// `checkpoint_viewed_at` as the cursor, and can optionally be further `filter`-ed.
    ///
    /// The `checkpoint_viewed_at` parameter is an Option<u64> representing the
    /// checkpoint_sequence_number at which this page was queried for, or `None` if the data was
    /// requested at the latest checkpoint. Each entity returned in the connection will inherit this
    /// checkpoint, so that when viewing that entity's state, it will be from the reference of this
    /// checkpoint_viewed_at parameter.
    ///
    /// If the `Page<Cursor>` is set, then this function will defer to the `checkpoint_viewed_at` in
    /// the cursor if they are consistent.
    pub(crate) async fn paginate(
        db: &Db,
        page: Page<Cursor>,
        filter: TransactionBlockFilter,
        checkpoint_viewed_at: Option<u64>,
    ) -> Result<Connection<String, TransactionBlock>, Error> {
        use transactions as tx;

        let cursor_viewed_at = page.validate_cursor_consistency()?;
        let checkpoint_viewed_at: Option<u64> = cursor_viewed_at.or(checkpoint_viewed_at);

        let response = db
            .execute_repeatable(move |conn| {
                let checkpoint_viewed_at = match checkpoint_viewed_at {
                    Some(value) => Ok(value),
                    None => Checkpoint::latest_checkpoint_sequence_number(conn),
                }?;

                let result = page.paginate_query::<StoredTransaction, _, _, _>(
                    conn,
                    checkpoint_viewed_at,
                    move || {
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

                        let before_checkpoint = filter
                            .before_checkpoint
                            .map_or(checkpoint_viewed_at + 1, |c| {
                                c.min(checkpoint_viewed_at + 1)
                            });
                        query = query.filter(
                            tx::dsl::checkpoint_sequence_number.lt(before_checkpoint as i64),
                        );

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
                    },
                )?;

                Ok::<_, diesel::result::Error>((result, checkpoint_viewed_at))
            })
            .await?;

        let ((prev, next, results), checkpoint_viewed_at) = response;

        let mut conn = Connection::new(prev, next);

        // Defer to the provided checkpoint_viewed_at, but if it is not provided, use the
        // current available range. This sets a consistent upper bound for the nested queries.
        for stored in results {
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
}

impl Paginated<Cursor> for StoredTransaction {
    type Source = transactions::table;

    fn filter_ge<ST, GB>(cursor: &Cursor, query: Query<ST, GB>) -> Query<ST, GB> {
        query
            .filter(transactions::dsl::tx_sequence_number.ge(cursor.tx_sequence_number as i64))
            .filter(
                transactions::dsl::checkpoint_sequence_number
                    .ge(cursor.tx_checkpoint_number as i64),
            )
    }

    fn filter_le<ST, GB>(cursor: &Cursor, query: Query<ST, GB>) -> Query<ST, GB> {
        query
            .filter(transactions::dsl::tx_sequence_number.le(cursor.tx_sequence_number as i64))
            .filter(
                transactions::dsl::checkpoint_sequence_number
                    .le(cursor.tx_checkpoint_number as i64),
            )
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
            tx_checkpoint_number: self.checkpoint_sequence_number as u64,
            checkpoint_viewed_at,
        })
    }
}

impl Checkpointed for Cursor {
    fn checkpoint_viewed_at(&self) -> u64 {
        self.checkpoint_viewed_at
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
