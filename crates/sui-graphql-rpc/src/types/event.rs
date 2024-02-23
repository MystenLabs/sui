// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use super::checkpoint::Checkpoint;
use super::cursor::{self, Page, Paginated, Target};
use super::digest::Digest;
use super::type_filter::{ModuleFilter, TypeFilter};
use super::{
    address::Address, base64::Base64, date_time::DateTime, move_module::MoveModule,
    move_value::MoveValue, sui_address::SuiAddress,
};
use crate::consistency::Checkpointed;
use crate::data::{self, QueryExecutor};
use crate::{data::Db, error::Error};
use async_graphql::connection::{Connection, CursorType, Edge};
use async_graphql::*;
use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl};
use serde::{Deserialize, Serialize};
use sui_indexer::models::{events::StoredEvent, transactions::StoredTransaction};
use sui_indexer::schema::{events, transactions, tx_senders};
use sui_types::base_types::ObjectID;
use sui_types::Identifier;
use sui_types::{
    base_types::SuiAddress as NativeSuiAddress, event::Event as NativeEvent, parse_sui_struct_tag,
};

/// A Sui node emits one of the following events:
/// Move event
/// Publish event
/// Transfer object event
/// Delete object event
/// New object event
/// Epoch change event
#[derive(Clone, Debug)]
pub(crate) struct Event {
    pub stored: Option<StoredEvent>,
    pub native: NativeEvent,
    /// The checkpoint sequence number this was viewed at.
    pub checkpoint_viewed_at: u64,
}

/// Contents of an Event's cursor.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub(crate) struct EventKey {
    /// Transaction Sequence Number
    tx: u64,

    /// Event Sequence Number
    e: u64,

    /// The checkpoint sequence number this was viewed at.
    #[serde(rename = "c")]
    checkpoint_viewed_at: u64,
}

pub(crate) type Cursor = cursor::JsonCursor<EventKey>;
type Query<ST, GB> = data::Query<ST, events::table, GB>;

#[derive(InputObject, Clone, Default)]
pub(crate) struct EventFilter {
    pub sender: Option<SuiAddress>,
    pub transaction_digest: Option<Digest>,
    // Enhancement (post-MVP)
    // after_checkpoint
    // before_checkpoint
    /// Events emitted by a particular module. An event is emitted by a
    /// particular module if some function in the module is called by a
    /// PTB and emits an event.
    ///
    /// Modules can be filtered by their package, or package::module.
    pub emitting_module: Option<ModuleFilter>,

    /// This field is used to specify the type of event emitted.
    ///
    /// Events can be filtered by their type's package, package::module,
    /// or their fully qualified type name.
    ///
    /// Generic types can be queried by either the generic type name, e.g.
    /// `0x2::coin::Coin`, or by the full type name, such as
    /// `0x2::coin::Coin<0x2::sui::SUI>`.
    pub event_type: Option<TypeFilter>,
    // Enhancement (post-MVP)
    // pub start_time
    // pub end_time

    // Enhancement (post-MVP)
    // pub any
    // pub all
    // pub not
}

#[Object]
impl Event {
    /// The Move module containing some function that when called by
    /// a programmable transaction block (PTB) emitted this event.
    /// For example, if a PTB invokes A::m1::foo, which internally
    /// calls A::m2::emit_event to emit an event,
    /// the sending module would be A::m1.
    async fn sending_module(&self, ctx: &Context<'_>) -> Result<Option<MoveModule>> {
        MoveModule::query(
            ctx.data_unchecked(),
            self.native.package_id.into(),
            &self.native.transaction_module.to_string(),
            self.checkpoint_viewed_at,
        )
        .await
        .extend()
    }

    /// Address of the sender of the event
    async fn sender(&self) -> Result<Option<Address>> {
        if self.native.sender == NativeSuiAddress::ZERO {
            return Ok(None);
        }

        Ok(Some(Address {
            address: self.native.sender.into(),
            checkpoint_viewed_at: Some(self.checkpoint_viewed_at),
        }))
    }

    /// UTC timestamp in milliseconds since epoch (1/1/1970)
    async fn timestamp(&self) -> Result<Option<DateTime>, Error> {
        if let Some(stored) = &self.stored {
            Ok(Some(DateTime::from_ms(stored.timestamp_ms)?))
        } else {
            Ok(None)
        }
    }

    #[graphql(flatten)]
    async fn move_value(&self) -> Result<MoveValue> {
        Ok(MoveValue::new(
            self.native.type_.clone().into(),
            Base64::from(self.native.contents.clone()),
        ))
    }
}

impl Event {
    /// Query the database for a `page` of events. The Page uses the transaction, event, and
    /// checkpoint sequence numbers as the cursor to determine the correct page of results. The
    /// query can optionally be further `filter`-ed by the `EventFilter`.
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
        filter: EventFilter,
        checkpoint_viewed_at: Option<u64>,
    ) -> Result<Connection<String, Event>, Error> {
        let cursor_viewed_at = page.validate_cursor_consistency()?;
        let checkpoint_viewed_at: Option<u64> = cursor_viewed_at.or(checkpoint_viewed_at);

        let ((prev, next, results), checkpoint_viewed_at) = db
            .execute_repeatable(move |conn| {
                let checkpoint_viewed_at = match checkpoint_viewed_at {
                    Some(value) => Ok(value),
                    None => Checkpoint::available_range(conn).map(|(_, rhs)| rhs),
                }?;

                let result = page.paginate_query::<StoredEvent, _, _, _>(
                    conn,
                    checkpoint_viewed_at,
                    move || {
                        let mut query = events::dsl::events.into_boxed();

                        // Bound events by the provided `checkpoint_viewed_at`. From EXPLAIN
                        // ANALYZE, using the checkpoint sequence number directly instead of
                        // translating into a transaction sequence number bound is more efficient.
                        query = query.filter(
                            events::dsl::checkpoint_sequence_number.le(checkpoint_viewed_at as i64),
                        );

                        // The transactions table doesn't have an index on the senders column, so use
                        // `tx_senders`.
                        if let Some(sender) = &filter.sender {
                            query = query.filter(
                                events::dsl::tx_sequence_number.eq_any(
                                    tx_senders::dsl::tx_senders
                                        .select(tx_senders::dsl::tx_sequence_number)
                                        .filter(tx_senders::dsl::sender.eq(sender.into_vec())),
                                ),
                            )
                        }

                        if let Some(digest) = &filter.transaction_digest {
                            query = query.filter(
                                events::dsl::tx_sequence_number.eq_any(
                                    transactions::dsl::transactions
                                        .select(transactions::dsl::tx_sequence_number)
                                        .filter(
                                            transactions::dsl::transaction_digest
                                                .eq(digest.to_vec()),
                                        ),
                                ),
                            )
                        }

                        if let Some(module) = &filter.emitting_module {
                            query = module.apply(query, events::dsl::package, events::dsl::module);
                        }

                        if let Some(type_) = &filter.event_type {
                            query = type_.apply(query, events::dsl::event_type);
                        }

                        query
                    },
                )?;

                Ok::<_, diesel::result::Error>((result, checkpoint_viewed_at))
            })
            .await?;

        let mut conn = Connection::new(prev, next);

        // Defer to the provided checkpoint_viewed_at, but if it is not provided, use the
        // current available range. This sets a consistent upper bound for the nested queries.
        for stored in results {
            let cursor = stored.cursor(checkpoint_viewed_at).encode_cursor();
            conn.edges.push(Edge::new(
                cursor,
                Event::try_from_stored_event(stored, checkpoint_viewed_at)?,
            ));
        }

        Ok(conn)
    }

    pub(crate) fn try_from_stored_transaction(
        stored_tx: &StoredTransaction,
        idx: usize,
        checkpoint_viewed_at: u64,
    ) -> Result<Self, Error> {
        let Some(Some(serialized_event)) = stored_tx.events.get(idx) else {
            return Err(Error::Internal(format!(
                "Could not find event with event_sequence_number {} at transaction {}",
                idx, stored_tx.tx_sequence_number
            )));
        };

        let native_event: NativeEvent = bcs::from_bytes(serialized_event).map_err(|_| {
            Error::Internal(format!(
                "Failed to deserialize event with {} at transaction {}",
                idx, stored_tx.tx_sequence_number
            ))
        })?;

        let stored_event = StoredEvent {
            tx_sequence_number: stored_tx.tx_sequence_number,
            event_sequence_number: idx as i64,
            transaction_digest: stored_tx.transaction_digest.clone(),
            checkpoint_sequence_number: stored_tx.checkpoint_sequence_number,
            senders: vec![Some(native_event.sender.to_vec())],
            package: native_event.package_id.to_vec(),
            module: native_event.transaction_module.to_string(),
            event_type: native_event
                .type_
                .to_canonical_string(/* with_prefix */ true),
            bcs: native_event.contents.clone(),
            timestamp_ms: stored_tx.timestamp_ms,
        };

        Ok(Self {
            stored: Some(stored_event),
            native: native_event,
            checkpoint_viewed_at,
        })
    }

    fn try_from_stored_event(
        stored: StoredEvent,
        checkpoint_viewed_at: u64,
    ) -> Result<Self, Error> {
        let Some(Some(sender_bytes)) = stored.senders.first() else {
            return Err(Error::Internal("No senders found for event".to_string()));
        };
        let sender = NativeSuiAddress::from_bytes(sender_bytes)
            .map_err(|e| Error::Internal(e.to_string()))?;

        let package_id =
            ObjectID::from_bytes(&stored.package).map_err(|e| Error::Internal(e.to_string()))?;
        let type_ =
            parse_sui_struct_tag(&stored.event_type).map_err(|e| Error::Internal(e.to_string()))?;
        let transaction_module =
            Identifier::from_str(&stored.module).map_err(|e| Error::Internal(e.to_string()))?;
        let contents = stored.bcs.clone();
        Ok(Event {
            stored: Some(stored),
            native: NativeEvent {
                sender,
                package_id,
                transaction_module,
                type_,
                contents,
            },
            checkpoint_viewed_at,
        })
    }
}

impl Paginated<Cursor> for StoredEvent {
    type Source = events::table;

    fn filter_ge<ST, GB>(cursor: &Cursor, query: Query<ST, GB>) -> Query<ST, GB> {
        use events::dsl::{event_sequence_number as event, tx_sequence_number as tx};
        query.filter(
            tx.gt(cursor.tx as i64)
                .or(tx.eq(cursor.tx as i64).and(event.ge(cursor.e as i64))),
        )
    }

    fn filter_le<ST, GB>(cursor: &Cursor, query: Query<ST, GB>) -> Query<ST, GB> {
        use events::dsl::{event_sequence_number as event, tx_sequence_number as tx};
        query.filter(
            tx.lt(cursor.tx as i64)
                .or(tx.eq(cursor.tx as i64).and(event.le(cursor.e as i64))),
        )
    }

    fn order<ST, GB>(asc: bool, query: Query<ST, GB>) -> Query<ST, GB> {
        use events::dsl;
        if asc {
            query
                .order_by(dsl::tx_sequence_number.asc())
                .then_order_by(dsl::event_sequence_number.asc())
        } else {
            query
                .order_by(dsl::tx_sequence_number.desc())
                .then_order_by(dsl::event_sequence_number.desc())
        }
    }
}

impl Target<Cursor> for StoredEvent {
    fn cursor(&self, checkpoint_viewed_at: u64) -> Cursor {
        Cursor::new(EventKey {
            tx: self.tx_sequence_number as u64,
            e: self.event_sequence_number as u64,
            checkpoint_viewed_at,
        })
    }
}

impl Checkpointed for Cursor {
    fn checkpoint_viewed_at(&self) -> u64 {
        self.checkpoint_viewed_at
    }
}
