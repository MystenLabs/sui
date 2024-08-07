// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{Cursor, TransactionBlockFilter};
use crate::{
    data::{pg::bytea_literal, Conn, DbConnection},
    filter, inner_join, query,
    raw_query::RawQuery,
    types::{
        cursor::Page,
        digest::Digest,
        sui_address::SuiAddress,
        transaction_block::TransactionBlockKindInput,
        type_filter::{FqNameFilter, ModuleFilter},
    },
};
use diesel::{
    query_dsl::positional_order_dsl::PositionalOrderDsl, CombineDsl, ExpressionMethods,
    NullableExpressionMethods, QueryDsl,
};
use std::fmt::Write;
use sui_indexer::schema::checkpoints;

#[derive(Clone, Debug, Copy)]
pub(crate) struct TxBounds {
    /// The inclusive lower bound tx_sequence_number corresponding to the first tx_sequence_number
    /// of the lower checkpoint bound before applying `after` cursor and `scan_limit`.
    pub lo: u64,
    /// The inclusive upper bound tx_sequence_number corresponding to the last tx_sequence_number of
    /// the upper checkpoint bound before applying `before` cursor and `scan_limit`.
    pub hi: u64,
    /// Exclusive starting cursor - the lower bound will snap to this value if it is larger than
    /// `lo`.
    pub after: Option<u64>,
    /// Exclusive ending cursor - the upper bound will snap to this value if it is smaller than
    /// `hi`.
    pub before: Option<u64>,
    pub scan_limit: Option<u64>,
    pub is_from_front: bool,
}

impl TxBounds {
    fn new(
        lo: u64,
        hi: u64,
        after: Option<u64>,
        before: Option<u64>,
        scan_limit: Option<u64>,
        is_from_front: bool,
    ) -> Self {
        Self {
            lo,
            hi,
            after,
            before,
            scan_limit,
            is_from_front,
        }
    }

    /// Determines the `tx_sequence_number` range from the checkpoint bounds for a transaction block
    /// query. If no checkpoint range is specified, the default is between 0 and the
    /// `checkpoint_viewed_at`. The corresponding `tx_sequence_number` range is fetched from db, and
    /// further adjusted by cursors and scan limit. If there are any inconsistencies or invalid
    /// combinations, i.e. `after` cursor is greater than the upper bound, return None.
    pub(crate) fn query(
        conn: &mut Conn,
        after_cp: Option<u64>,
        at_cp: Option<u64>,
        before_cp: Option<u64>,
        checkpoint_viewed_at: u64,
        scan_limit: Option<u64>,
        page: &Page<Cursor>,
    ) -> Result<Option<Self>, diesel::result::Error> {
        // If `after_cp` is given, increment it by 1 so we can uniformly select the lower bound
        // checkpoint's `min_tx_sequence_number`. The range is inclusive of this value.
        let lo_cp = max_option([after_cp.map(|x| x.saturating_add(1)), at_cp]).unwrap_or(0);
        // Assumes that `before_cp` is greater than 0. In the `TransactionBlock::paginate` flow, we
        // check if `before_cp` is 0, and if so, short-circuit and produce no results. Similarly, if
        // `before_cp` is given, decrement by 1 so we can select the upper bound checkpoint's
        // `max_tx_sequence_number` uniformly. The range is inclusive of this value.
        let hi_cp = min_option([
            before_cp.map(|x| x.saturating_sub(1)),
            at_cp,
            Some(checkpoint_viewed_at),
        ])
        .unwrap(); // SAFETY: we can unwrap because of the `Some(checkpoint_viewed_at)`

        use checkpoints::dsl;

        let from_db: Vec<(Option<i64>, Option<i64>)> = conn.results(move || {
            // Construct a UNION ALL query ordered on `sequence_number` to get the tx ranges for the
            // checkpoint range.
            dsl::checkpoints
                .select((
                    dsl::sequence_number.nullable(),
                    dsl::network_total_transactions.nullable(),
                ))
                .filter(dsl::sequence_number.eq(lo_cp.saturating_sub(1) as i64))
                .union(
                    dsl::checkpoints
                        .select((
                            dsl::sequence_number.nullable(),
                            dsl::network_total_transactions.nullable() - 1,
                        ))
                        .filter(dsl::sequence_number.eq(hi_cp as i64)),
                )
                .positional_order_by(1) // order by checkpoint's sequence number, which is the first column
        })?;

        // Expect exactly two rows, returning early if not.
        let [(Some(db_lo_cp), Some(lo)), (Some(db_hi_cp), Some(hi))] = from_db.as_slice() else {
            return Ok(None);
        };

        if *db_lo_cp as u64 != lo_cp.saturating_sub(1) || *db_hi_cp as u64 != hi_cp {
            return Ok(None);
        }

        let lo = if lo_cp == 0 { 0 } else { *lo as u64 };
        let hi = *hi as u64;

        if page.after().is_some_and(|x| x.tx_sequence_number >= hi)
            || page.before().is_some_and(|x| x.tx_sequence_number <= lo)
        {
            return Ok(None);
        }

        Ok(Some(Self::new(
            lo,
            hi,
            page.after().map(|x| x.tx_sequence_number),
            page.before().map(|x| x.tx_sequence_number),
            scan_limit,
            page.is_from_front(),
        )))
    }

    /// Returns the max of the current tx lower bound or the `after` tx cursor.
    fn cursor_lo(&self) -> u64 {
        max_option([self.after, Some(self.lo)]).unwrap()
    }

    /// Determines the lower bound for the scanning range. `exclude_cursor` determines whether to
    /// include `after` cursor in this calculation. When paginating backwards, the lower bound is
    /// the larger of either the lower bound or the `tx_sequence_number` some `scan_limit` distance
    /// less than the upper bound. The latter is additionally added by 1 if `before` cursor is None
    /// to avoid over-counting.
    fn calculate_lo(&self, exclude_cursor: bool) -> u64 {
        let cursor_lo = self.cursor_lo().saturating_add(exclude_cursor as u64);

        if self.is_from_front {
            cursor_lo
        } else if let Some(scan_limit) = self.scan_limit {
            cursor_lo.max(
                self.cursor_hi()
                    .saturating_sub(scan_limit)
                    // We encounter an off-by-one error when the `before` cursor is not provided, so
                    // we add 1 to counteract this
                    .saturating_add(self.before.is_none() as u64),
            )
        } else {
            cursor_lo
        }
    }

    /// The lower bound `tx_sequence_number` of the range to scan within. This is inclusive of the
    /// `after` cursor, which is needed for the db call, but should be excluded when determining the
    /// first scanned transaction. When paginating forwards, this is the larger value between the
    /// lower checkpoint's `tx_sequence_number` and the `after` cursor. If scanning backwards, then
    /// the lower bound is the larger between the former and the `tx_sequence_number` some
    /// `scan_limit` distance less than the upper bound.
    pub(crate) fn scan_lo(&self) -> u64 {
        self.calculate_lo(/* exclude_cursor */ false)
    }

    /// The lower bound `tx_sequence_number` of the scanned transactions, exclusive of the `after`
    /// cursor.
    pub(crate) fn inclusive_scan_lo(&self) -> u64 {
        self.calculate_lo(self.after.is_some())
    }

    /// Returns the min of the current tx upper bound or the `before` tx cursor.
    fn cursor_hi(&self) -> u64 {
        min_option([self.before, Some(self.hi)]).unwrap()
    }

    /// Determines the upper bound for the scanning range. `exclude_cursor` determines whether to
    /// include `before` cursor in this calculation. When paginating forwards, the upper bound is
    /// the smaller of either the upper bound or the `tx_sequence_number` some `scan_limit` distance
    /// more than the lower bound. The latter is additionally subtracted by 1 if `after` cursor is
    /// None to avoid over-counting.
    fn calculate_hi(&self, exclude_cursor: bool) -> u64 {
        let cursor_hi = self.cursor_hi().saturating_sub(exclude_cursor as u64);

        if !self.is_from_front {
            cursor_hi
        } else if let Some(scan_limit) = self.scan_limit {
            // If the `after` cursor is not provided, we will overcount when adding scan_limit
            // directly to the `lo` unless we subtract 1
            cursor_hi.min(
                self.cursor_lo()
                    .saturating_add(scan_limit)
                    .saturating_sub(self.after.is_none() as u64),
            )
        } else {
            cursor_hi
        }
    }

    /// The upper bound `tx_sequence_number` of the range to scan within. This is inclusive of the
    /// `before` cursor, which is needed for the db call, but should be excluded when determining
    /// the last scanned transaction. When paginating backwards, this is the smaller value between
    /// the upper checkpoint's `tx_sequence_number` and the `before` cursor. If scanning forwards,
    /// then the upper bound is the smaller between the former and the `tx_sequence_number` some
    /// `scan_limit` distance more than the lower bound.
    pub(crate) fn scan_hi(&self) -> u64 {
        self.calculate_hi(/* exclude_cursor */ false)
    }

    /// The upper bound `tx_sequence_number` of the scanned transactions, exclusive of the `before`
    /// cursor.
    pub(crate) fn inclusive_scan_hi(&self) -> u64 {
        self.calculate_hi(self.before.is_some())
    }

    /// Whether there are more transactions to scan to the left of this page.
    pub(crate) fn scan_has_prev_page(&self) -> bool {
        self.lo < self.inclusive_scan_lo()
    }

    /// Whether there are more transactions to scan to the right of this page.
    pub(crate) fn scan_has_next_page(&self) -> bool {
        self.inclusive_scan_hi() < self.hi
    }
}

/// Determines the maximum value in an arbitrary number of Option<impl Ord>.
fn max_option<T: Ord>(xs: impl IntoIterator<Item = Option<T>>) -> Option<T> {
    xs.into_iter().flatten().max()
}

/// Determines the minimum value in an arbitrary number of Option<impl Ord>.
fn min_option<T: Ord>(xs: impl IntoIterator<Item = Option<T>>) -> Option<T> {
    xs.into_iter().flatten().min()
}

/// Constructs a `RawQuery` as a join over all relevant side tables, filtered on their own filter
/// condition, plus optionally a sender, plus optionally tx/cp bounds.
pub(crate) fn subqueries(filter: &TransactionBlockFilter, tx_bounds: TxBounds) -> Option<RawQuery> {
    let sender = filter.sign_address;

    let mut subqueries = vec![];

    if let Some(f) = &filter.function {
        subqueries.push(match f {
            FqNameFilter::ByModule(filter) => match filter {
                ModuleFilter::ByPackage(p) => ("tx_calls_pkg", select_pkg(p, sender, tx_bounds)),
                ModuleFilter::ByModule(p, m) => {
                    ("tx_calls_mod", select_mod(p, m.clone(), sender, tx_bounds))
                }
            },
            FqNameFilter::ByFqName(p, m, n) => (
                "tx_calls_fun",
                select_fun(p, m.clone(), n.clone(), sender, tx_bounds),
            ),
        });
    }
    if let Some(kind) = &filter.kind {
        subqueries.push(("tx_kinds", select_kind(*kind, sender, tx_bounds)));
    }
    if let Some(recv) = &filter.recv_address {
        subqueries.push(("tx_recipients", select_recipient(recv, sender, tx_bounds)));
    }
    if let Some(input) = &filter.input_object {
        subqueries.push(("tx_input_objects", select_input(input, sender, tx_bounds)));
    }
    if let Some(changed) = &filter.changed_object {
        subqueries.push((
            "tx_changed_objects",
            select_changed(changed, sender, tx_bounds),
        ));
    }
    if let Some(sender) = &filter.explicit_sender() {
        subqueries.push(("tx_senders", select_sender(sender, tx_bounds)));
    }
    if let Some(txs) = &filter.transaction_ids {
        subqueries.push(("tx_digests", select_ids(txs, tx_bounds)));
    }

    let Some((_, mut subquery)) = subqueries.pop() else {
        return None;
    };

    if !subqueries.is_empty() {
        subquery = query!("SELECT tx_sequence_number FROM ({}) AS initial", subquery);
        while let Some((alias, subselect)) = subqueries.pop() {
            subquery = inner_join!(subquery, alias => subselect, using: ["tx_sequence_number"]);
        }
    }

    Some(subquery)
}

fn select_tx(sender: Option<SuiAddress>, bound: TxBounds, from: &str) -> RawQuery {
    let mut query = filter!(
        query!(format!("SELECT tx_sequence_number FROM {from}")),
        format!(
            "{} <= tx_sequence_number AND tx_sequence_number <= {}",
            bound.scan_lo(),
            bound.scan_hi()
        )
    );

    if let Some(sender) = sender {
        query = filter!(
            query,
            format!("sender = {}", bytea_literal(sender.as_slice()))
        );
    }

    query
}

fn select_pkg(pkg: &SuiAddress, sender: Option<SuiAddress>, bound: TxBounds) -> RawQuery {
    filter!(
        select_tx(sender, bound, "tx_calls_pkg"),
        format!("package = {}", bytea_literal(pkg.as_slice()))
    )
}

fn select_mod(
    pkg: &SuiAddress,
    mod_: String,
    sender: Option<SuiAddress>,
    bound: TxBounds,
) -> RawQuery {
    filter!(
        select_tx(sender, bound, "tx_calls_mod"),
        format!(
            "package = {} and module = {{}}",
            bytea_literal(pkg.as_slice())
        ),
        mod_
    )
}

fn select_fun(
    pkg: &SuiAddress,
    mod_: String,
    fun: String,
    sender: Option<SuiAddress>,
    bound: TxBounds,
) -> RawQuery {
    filter!(
        select_tx(sender, bound, "tx_calls_fun"),
        format!(
            "package = {} AND module = {{}} AND func = {{}}",
            bytea_literal(pkg.as_slice()),
        ),
        mod_,
        fun
    )
}

/// Returns a RawQuery that selects transactions of a specific kind. If SystemTX is specified, we
/// ignore the `sender`. If ProgrammableTX is specified, we filter against the `tx_kinds` table if
/// no `sender` is provided; otherwise, we just query the `tx_senders` table. Other combinations, in
/// particular when kind is SystemTx and sender is specified and not 0x0, are inconsistent and will
/// not produce any results. These inconsistent cases are expected to be checked for before this is
/// called.
fn select_kind(
    kind: TransactionBlockKindInput,
    sender: Option<SuiAddress>,
    bound: TxBounds,
) -> RawQuery {
    match (kind, sender) {
        // We can simplify the query to just the `tx_senders` table if ProgrammableTX and sender is
        // specified.
        (TransactionBlockKindInput::ProgrammableTx, Some(sender)) => select_sender(&sender, bound),
        // Otherwise, we can ignore the sender always, and just query the `tx_kinds` table.
        _ => filter!(
            select_tx(None, bound, "tx_kinds"),
            format!("tx_kind = {}", kind as i16)
        ),
    }
}

fn select_sender(sender: &SuiAddress, bound: TxBounds) -> RawQuery {
    select_tx(Some(*sender), bound, "tx_senders")
}

fn select_recipient(recv: &SuiAddress, sender: Option<SuiAddress>, bound: TxBounds) -> RawQuery {
    filter!(
        select_tx(sender, bound, "tx_recipients"),
        format!("recipient = {}", bytea_literal(recv.as_slice()))
    )
}

fn select_input(input: &SuiAddress, sender: Option<SuiAddress>, bound: TxBounds) -> RawQuery {
    filter!(
        select_tx(sender, bound, "tx_input_objects"),
        format!("object_id = {}", bytea_literal(input.as_slice()))
    )
}

fn select_changed(changed: &SuiAddress, sender: Option<SuiAddress>, bound: TxBounds) -> RawQuery {
    filter!(
        select_tx(sender, bound, "tx_changed_objects"),
        format!("object_id = {}", bytea_literal(changed.as_slice()))
    )
}

fn select_ids(ids: &Vec<Digest>, bound: TxBounds) -> RawQuery {
    let query = select_tx(None, bound, "tx_digests");
    if ids.is_empty() {
        filter!(query, "1=0")
    } else {
        let mut inner = String::new();
        let mut prefix = "tx_digest IN (";
        for id in ids {
            write!(&mut inner, "{prefix}{}", bytea_literal(id.as_slice())).unwrap();
            prefix = ", ";
        }
        inner.push(')');
        filter!(query, inner)
    }
}
