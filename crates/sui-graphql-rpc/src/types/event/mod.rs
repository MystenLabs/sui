// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use super::cursor::{Page, Target};
use super::{
    address::Address, base64::Base64, date_time::DateTime, move_module::MoveModule,
    move_value::MoveValue, transaction_block::TransactionBlock,
};
use crate::data::{self, DbConnection, QueryExecutor};
use crate::query;
use crate::{data::Db, error::Error};
use async_graphql::connection::{Connection, CursorType, Edge};
use async_graphql::*;
use cursor::EvLookup;
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::scoped_futures::ScopedFutureExt;
use lookups::{add_bounds, select_emit_module, select_event_type, select_sender};
use sui_indexer::models::{events::StoredEvent, transactions::StoredTransaction};
use sui_indexer::schema::{checkpoints, events};
use sui_types::base_types::ObjectID;
use sui_types::Identifier;
use sui_types::{
    base_types::SuiAddress as NativeSuiAddress, event::Event as NativeEvent, parse_sui_struct_tag,
};

mod cursor;
mod filter;
mod lookups;
pub(crate) use cursor::Cursor;
pub(crate) use filter::EventFilter;

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

type Query<ST, GB> = data::Query<ST, events::table, GB>;

#[Object]
impl Event {
    /// The transaction block that emitted this event. This information is only available for
    /// events from indexed transactions, and not from transactions that have just been executed or
    /// dry-run.
    async fn transaction_block(&self, ctx: &Context<'_>) -> Result<Option<TransactionBlock>> {
        let Some(stored) = &self.stored else {
            return Ok(None);
        };

        TransactionBlock::query(
            ctx,
            TransactionBlock::by_seq(stored.tx_sequence_number as u64, self.checkpoint_viewed_at),
        )
        .await
        .extend()
    }

    /// The Move module containing some function that when called by
    /// a programmable transaction block (PTB) emitted this event.
    /// For example, if a PTB invokes A::m1::foo, which internally
    /// calls A::m2::emit_event to emit an event,
    /// the sending module would be A::m1.
    async fn sending_module(&self, ctx: &Context<'_>) -> Result<Option<MoveModule>> {
        MoveModule::query(
            ctx,
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
            checkpoint_viewed_at: self.checkpoint_viewed_at,
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

    /// The event's contents as a Move value.
    async fn contents(&self) -> Result<MoveValue> {
        Ok(MoveValue::new(
            self.native.type_.clone().into(),
            Base64::from(self.native.contents.clone()),
        ))
    }

    /// The Base64 encoded BCS serialized bytes of the event.
    async fn bcs(&self) -> Result<Base64> {
        Ok(Base64::from(
            bcs::to_bytes(&self.native).map_err(|e| Error::Internal(e.to_string()))?,
        ))
    }
}

impl Event {
    /// Query the database for a `page` of events. The Page uses the transaction, event, and
    /// checkpoint sequence numbers as the cursor to determine the correct page of results. The
    /// query can optionally be further `filter`-ed by the `EventFilter`.
    ///
    /// The `checkpoint_viewed_at` parameter represents the checkpoint sequence number at which
    /// this page was queried. Each entity returned in the connection inherits this checkpoint, so
    /// that when viewing that entity's state, it's as if it's being viewed at this checkpoint.
    ///
    /// The cursors in `page` might also include checkpoint viewed at fields. If these are set,
    /// they take precedence over the checkpoint that pagination is being conducted in.
    pub(crate) async fn paginate(
        db: &Db,
        page: Page<Cursor>,
        filter: EventFilter,
        checkpoint_viewed_at: u64,
    ) -> Result<Connection<String, Event>, Error> {
        let cursor_viewed_at = page.validate_cursor_consistency()?;
        let checkpoint_viewed_at = cursor_viewed_at.unwrap_or(checkpoint_viewed_at);

        // Construct tx and ev sequence number query with table-relevant filters, if they exist. The
        // resulting query will look something like `SELECT tx_sequence_number,
        // event_sequence_number FROM lookup_table WHERE ...`. If no filter is provided we don't
        // need to use any lookup tables and can just query `events` table, as can be seen in the
        // code below.
        let query_constraint = match (filter.sender, &filter.emitting_module, &filter.event_type) {
            (None, None, None) => None,
            (Some(sender), None, None) => Some(select_sender(sender)),
            (sender, None, Some(event_type)) => Some(select_event_type(event_type, sender)),
            (sender, Some(module), None) => Some(select_emit_module(module, sender)),
            (_, Some(_), Some(_)) => {
                return Err(Error::Client(
                    "Filtering by both emitting module and event type is not supported".to_string(),
                ))
            }
        };

        use checkpoints::dsl;
        let (prev, next, results) = db
            .execute(move |conn| async move {
                let tx_hi: i64 = conn.first(move || {
                    dsl::checkpoints.select(dsl::network_total_transactions)
                        .filter(dsl::sequence_number.eq(checkpoint_viewed_at as i64))
                }).await?;

                let (prev, next, mut events): (bool, bool, Vec<StoredEvent>) =
                    if let Some(filter_query) =  query_constraint {
                        let query = add_bounds(filter_query, &filter.transaction_digest, &page, tx_hi);

                        let (prev, next, results) =
                            page.paginate_raw_query::<EvLookup>(conn, checkpoint_viewed_at, query).await?;

                        let ev_lookups = results
                            .into_iter()
                            .map(|x| (x.tx, x.ev))
                            .collect::<Vec<(i64, i64)>>();

                        if ev_lookups.is_empty() {
                            return Ok::<_, diesel::result::Error>((prev, next, vec![]));
                        }

                        // Unlike a multi-get on a single column which can be serviced by a query `IN
                        // (...)`, because events have a composite primary key, the query planner tends
                        // to perform a sequential scan when given a list of tuples to lookup. A query
                        // using `UNION ALL` allows us to leverage the index on the composite key.
                        let events = conn.results(move || {
                            // Diesel's DSL does not current support chained `UNION ALL`, so we have to turn
                            // to `RawQuery` here.
                            let query_string = ev_lookups.iter()
                                .map(|&(tx, ev)| {
                                    format!("SELECT * FROM events WHERE tx_sequence_number = {} AND event_sequence_number = {}", tx, ev)
                                })
                                .collect::<Vec<String>>()
                                .join(" UNION ALL ");

                            query!(query_string).into_boxed()
                        }).await?;
                        (prev, next, events)
                    } else {
                        // No filter is provided so we add bounds to the basic `SELECT * FROM
                        // events` query and call it a day.
                        let query = add_bounds(query!("SELECT * FROM events"), &filter.transaction_digest, &page, tx_hi);
                        let (prev, next, events_iter) = page.paginate_raw_query::<StoredEvent>(conn, checkpoint_viewed_at, query).await?;
                        let events = events_iter.collect::<Vec<StoredEvent>>();
                        (prev, next, events)
                    };

                // UNION ALL does not guarantee order, so we need to sort the results. Whether
                // `first` or `last, the result set is always sorted in ascending order.
                events.sort_by(|a, b| {
                        a.tx_sequence_number.cmp(&b.tx_sequence_number)
                            .then_with(|| a.event_sequence_number.cmp(&b.event_sequence_number))
                });


                Ok::<_, diesel::result::Error>((prev, next, events))
            }.scope_boxed())
            .await?;

        let mut conn = Connection::new(prev, next);

        // The "checkpoint viewed at" sets a consistent upper bound for the nested queries.
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
        let Some(serialized_event) = &stored_tx.get_event_at_idx(idx) else {
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
            senders: vec![Some(native_event.sender.to_vec())],
            package: native_event.package_id.to_vec(),
            module: native_event.transaction_module.to_string(),
            event_type: native_event
                .type_
                .to_canonical_string(/* with_prefix */ true),
            bcs: native_event.contents.clone(),
            timestamp_ms: stored_tx.timestamp_ms,
            sender: Some(native_event.sender.to_vec()),
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
        let Some(Some(sender_bytes)) = ({ stored.senders.first() }) else {
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
