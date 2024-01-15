// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::cursor::{self, Page, Target};
use super::digest::Digest;
use super::type_filter::{ModuleFilter, TypeFilter};
use super::{
    address::Address, base64::Base64, date_time::DateTime, move_module::MoveModule,
    move_value::MoveValue, sui_address::SuiAddress,
};
use crate::data::{self, QueryExecutor};
use crate::{data::Db, error::Error};
use async_graphql::connection::{Connection, CursorType, Edge};
use async_graphql::*;
use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl};
use serde::{Deserialize, Serialize};
use sui_indexer::models_v2::{events::StoredEvent, transactions::StoredTransaction};
use sui_indexer::schema_v2::{events, transactions, tx_senders};
use sui_types::{
    base_types::SuiAddress as NativeSuiAddress, event::Event as NativeEvent, parse_sui_struct_tag,
    TypeTag,
};

pub(crate) struct Event {
    pub stored: StoredEvent,
}

/// Contents of an Event's cursor.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub(crate) struct EventKey {
    /// Transaction Sequence Number
    tx: u64,

    /// Event Sequence Number
    e: u64,
}

pub(crate) type Cursor = cursor::Cursor<EventKey>;
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
        let sending_package = SuiAddress::from_bytes(&self.stored.package)
            .map_err(|e| Error::Internal(e.to_string()))
            .extend()?;
        MoveModule::query(ctx.data_unchecked(), sending_package, &self.stored.module)
            .await
            .extend()
    }

    /// Address of the sender of the event
    async fn sender(&self) -> Result<Option<Address>> {
        let Some(Some(sender)) = self.stored.senders.first() else {
            return Ok(None);
        };

        let address = SuiAddress::from_bytes(sender)
            .map_err(|e| Error::Internal(format!("Failed to deserialize address: {e}")))
            .extend()?;

        if address.as_slice() == NativeSuiAddress::ZERO.as_ref() {
            return Ok(None);
        }

        Ok(Some(Address { address }))
    }

    /// UTC timestamp in milliseconds since epoch (1/1/1970)
    async fn timestamp(&self) -> Result<Option<DateTime>, Error> {
        Ok(DateTime::from_ms(self.stored.timestamp_ms).ok())
    }

    #[graphql(flatten)]
    async fn move_value(&self) -> Result<MoveValue> {
        let type_ = TypeTag::from(
            parse_sui_struct_tag(&self.stored.event_type)
                .map_err(|e| Error::Internal(e.to_string()))
                .extend()?,
        );
        Ok(MoveValue::new(type_, Base64::from(self.stored.bcs.clone())))
    }
}

impl Event {
    /// Query the database for a `page` of events. The Page uses a combination of transaction and
    /// event sequence numbers as the cursor, and can optionally be further `filter`-ed by the
    /// `EventFilter`.
    pub(crate) async fn paginate(
        db: &Db,
        page: Page<EventKey>,
        filter: EventFilter,
    ) -> Result<Connection<String, Event>, Error> {
        let (prev, next, results) = db
            .execute(move |conn| {
                page.paginate_query::<StoredEvent, _, _, _>(conn, move || {
                    let mut query = events::dsl::events.into_boxed();

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
                                        transactions::dsl::transaction_digest.eq(digest.to_vec()),
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
                })
            })
            .await?;

        let mut conn = Connection::new(prev, next);

        for stored in results {
            let cursor = Cursor::new(stored.cursor()).encode_cursor();
            conn.edges.push(Edge::new(cursor, Event { stored }));
        }

        Ok(conn)
    }

    pub(crate) fn try_from_stored_transaction(
        stored_tx: &StoredTransaction,
        idx: usize,
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
            bcs: native_event.contents,
            timestamp_ms: stored_tx.timestamp_ms,
        };

        Ok(Self {
            stored: stored_event,
        })
    }
}

impl Target<EventKey> for StoredEvent {
    type Source = events::table;

    fn filter_ge<ST, GB>(cursor: &EventKey, query: Query<ST, GB>) -> Query<ST, GB> {
        use events::dsl::{event_sequence_number as event, tx_sequence_number as tx};
        query.filter(
            tx.gt(cursor.tx as i64)
                .or(tx.eq(cursor.tx as i64).and(event.ge(cursor.e as i64))),
        )
    }

    fn filter_le<ST, GB>(cursor: &EventKey, query: Query<ST, GB>) -> Query<ST, GB> {
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

    fn cursor(&self) -> EventKey {
        EventKey {
            tx: self.tx_sequence_number as u64,
            e: self.event_sequence_number as u64,
        }
    }
}
