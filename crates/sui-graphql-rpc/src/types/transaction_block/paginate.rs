// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::tx_lookups::{
    select_changed, select_fun, select_input, select_kind, select_mod, select_pkg,
    select_recipient, select_sender,
};
use crate::{
    data::{Conn, DbConnection},
    inner_join, max_option, min_option, query,
    raw_query::RawQuery,
    types::type_filter::{FqNameFilter, ModuleFilter},
};
use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, QueryableByName},
    row::NamedRow,
};

use super::TransactionBlockFilter;

/// The `tx_sequence_number` range of the transactions to be queried.
#[derive(Clone, Debug, Copy)]
pub(crate) struct TxBounds {
    pub lo: i64,
    pub hi: i64,
}

impl TxBounds {
    /// Anchor the lower and upper checkpoint bounds if provided from the filter, otherwise default
    /// the lower bound to 0 and the upper bound to `checkpoint_viewed_at`. Increment `after` by 1
    /// so that we can uniformly select the `min_tx_sequence_number` for the lower bound. Similarly,
    /// decrement `before` by 1 so that we can uniformly select the `max_tx_sequence_number`. In
    /// other words, the range consists of all transactions from the smallest tx_sequence_number in
    /// lo_cp to the max tx_sequence_number in hi_cp.
    pub(crate) fn query(
        conn: &mut Conn,
        after_cp: Option<u64>,
        at_cp: Option<u64>,
        before_cp: Option<u64>,
        checkpoint_viewed_at: u64,
    ) -> Result<Self, diesel::result::Error> {
        let lo_cp = max_option!(after_cp.map(|x| x.saturating_add(1)), at_cp).unwrap_or(0);
        let hi_cp = min_option!(before_cp.map(|x| x.saturating_sub(1)), at_cp)
            .unwrap_or(checkpoint_viewed_at);
        conn.result(move || tx_bounds_query(lo_cp, hi_cp).into_boxed())
    }
}

/// `sql_query` raw queries require `QueryableByName`. The default implementation looks for a table
/// based on the struct name, and it also expects the struct's fields to reflect the table's
/// columns. We can override this behavior by implementing `QueryableByName` for our struct. For
/// `TxBounds`, its fields are derived from `checkpoints`, so we can't leverage the default
/// implementation directly.
impl<DB> QueryableByName<DB> for TxBounds
where
    DB: Backend,
    i64: FromSql<diesel::sql_types::BigInt, DB>,
{
    fn build<'a>(row: &impl NamedRow<'a, DB>) -> deserialize::Result<Self> {
        let lo = NamedRow::get::<diesel::sql_types::BigInt, _>(row, "lo")?;
        let hi = NamedRow::get::<diesel::sql_types::BigInt, _>(row, "hi")?;

        Ok(Self { lo, hi })
    }
}

/// Constructs a query that selects the first tx_sequence_number of lo_cp and the last
/// tx_sequence_number of hi_cp. The first tx_sequence_number of lo_cp is the
/// `network_total_transactions` of lo_cp - 1, and the last tx_sequence_number is the
/// `network_total_transactions` - 1 of `hi_cp`.
pub(crate) fn tx_bounds_query(lo_cp: u64, hi_cp: u64) -> RawQuery {
    let lo = match lo_cp {
        0 => query!("SELECT 0"),
        _ => query!(format!(
            r#"SELECT network_total_transactions
            FROM checkpoints
            WHERE sequence_number = {}"#,
            lo_cp.saturating_sub(1)
        )),
    };

    let hi = query!(format!(
        r#"SELECT network_total_transactions - 1
        FROM checkpoints
        WHERE sequence_number = {}"#,
        hi_cp
    ));

    query!(
        "SELECT CAST(({}) AS BIGINT) AS lo, CAST(({}) AS BIGINT) AS hi",
        lo,
        hi
    )
}

/// Determines the maximum value in an arbitrary number of Option<u64>.
#[macro_export]
macro_rules! max_option {
    ($($x:expr),+ $(,)?) => {{
        [$($x),*].iter()
            .filter_map(|&x| x)
            .max()
    }};
}

/// Determines the minimum value in an arbitrary number of Option<u64>.
#[macro_export]
macro_rules! min_option {
    ($($x:expr),+ $(,)?) => {{
        [$($x),*].iter()
            .filter_map(|&x| x)
            .min()
    }};
}

/// Constructs a `RawQuery` as a join over all relevant side tables, filtered on their own filter
/// condition, plus optionally a sender, plus optionally tx/cp bounds.
pub(crate) fn subqueries(filter: &TransactionBlockFilter, tx_bounds: TxBounds) -> Option<RawQuery> {
    let sender = filter.sign_address;

    let mut subqueries = vec![];

    if let Some(f) = &filter.function {
        subqueries.push(match f {
            FqNameFilter::ByModule(filter) => match filter {
                ModuleFilter::ByPackage(p) => (select_pkg(p, sender, tx_bounds), "tx_calls_pkg"),
                ModuleFilter::ByModule(p, m) => {
                    (select_mod(p, m.clone(), sender, tx_bounds), "tx_calls_mod")
                }
            },
            FqNameFilter::ByFqName(p, m, n) => (
                select_fun(p, m.clone(), n.clone(), sender, tx_bounds),
                "tx_calls_fun",
            ),
        });
    }
    if let Some(kind) = &filter.kind {
        subqueries.push((select_kind(*kind, tx_bounds), "tx_kinds"));
    }
    if let Some(recv) = &filter.recv_address {
        subqueries.push((select_recipient(recv, sender, tx_bounds), "tx_recipients"));
    }
    if let Some(input) = &filter.input_object {
        subqueries.push((select_input(input, sender, tx_bounds), "tx_input_objects"));
    }
    if let Some(changed) = &filter.changed_object {
        subqueries.push((
            select_changed(changed, sender, tx_bounds),
            "tx_changed_objects",
        ));
    }
    if let Some(sender) = &sender {
        if !filter.has_complex_filters() || filter.kind.is_some() {
            subqueries.push((select_sender(sender, tx_bounds), "tx_senders"));
        }
    }

    if subqueries.is_empty() {
        return None;
    }

    let mut subquery = subqueries.pop().unwrap().0;

    if !subqueries.is_empty() {
        subquery = query!("SELECT tx_sequence_number FROM ({}) AS initial", subquery);
        while let Some((subselect, alias)) = subqueries.pop() {
            subquery =
                inner_join!(subquery, rhs => (subselect, alias), using: ["tx_sequence_number"]);
        }
    }

    Some(subquery)
}
