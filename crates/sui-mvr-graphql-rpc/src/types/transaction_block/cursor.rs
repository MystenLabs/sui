// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    consistency::Checkpointed,
    filter,
    raw_query::RawQuery,
    types::cursor::{self, Paginated, RawPaginated, ScanLimited, Target},
};
use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, QueryableByName},
    row::NamedRow,
    ExpressionMethods, QueryDsl,
};
use serde::{Deserialize, Serialize};
use sui_indexer::{models::transactions::StoredTransaction, schema::transactions};

use super::Query;

pub(crate) type Cursor = cursor::JsonCursor<TransactionBlockCursor>;

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
    /// Whether the cursor was derived from a `scan_limit`. Only applicable to the `startCursor` and
    /// `endCursor` returned from a Connection's `PageInfo`, and indicates that the cursor may not
    /// have a corresponding node in the result set.
    #[serde(rename = "i")]
    pub is_scan_limited: bool,
}

/// Results from raw queries in Diesel can only be deserialized into structs that implements
/// `QueryableByName`. This struct is used to represent a row of `tx_sequence_number` returned from
/// subqueries against tx lookup tables.
#[derive(Clone, Debug)]
pub struct TxLookup {
    pub tx_sequence_number: i64,
}

impl Checkpointed for Cursor {
    fn checkpoint_viewed_at(&self) -> u64 {
        self.checkpoint_viewed_at
    }
}

impl ScanLimited for Cursor {
    fn is_scan_limited(&self) -> bool {
        self.is_scan_limited
    }

    fn unlimited(&self) -> Self {
        Cursor::new(TransactionBlockCursor {
            is_scan_limited: false,
            tx_sequence_number: self.tx_sequence_number,
            checkpoint_viewed_at: self.checkpoint_viewed_at,
        })
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
            is_scan_limited: false,
        })
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

impl Target<Cursor> for TxLookup {
    fn cursor(&self, checkpoint_viewed_at: u64) -> Cursor {
        Cursor::new(TransactionBlockCursor {
            tx_sequence_number: self.tx_sequence_number as u64,
            checkpoint_viewed_at,
            is_scan_limited: false,
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

/// `sql_query` raw queries require `QueryableByName`. The default implementation looks for a table
/// based on the struct name, and it also expects the struct's fields to reflect the table's
/// columns. We can override this behavior by implementing `QueryableByName` for our struct. For
/// `TxBounds`, its fields are derived from `checkpoints`, so we can't leverage the default
/// implementation directly.
impl<DB> QueryableByName<DB> for TxLookup
where
    DB: Backend,
    i64: FromSql<diesel::sql_types::BigInt, DB>,
{
    fn build<'a>(row: &impl NamedRow<'a, DB>) -> deserialize::Result<Self> {
        let tx_sequence_number =
            NamedRow::get::<diesel::sql_types::BigInt, _>(row, "tx_sequence_number")?;

        Ok(Self { tx_sequence_number })
    }
}
