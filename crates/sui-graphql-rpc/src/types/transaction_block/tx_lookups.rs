// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! # Transaction Filter Lookup Tables
//!
//! ## Schemas
//!
//! Tables backing Transaction filters in GraphQL all follow the same rough shape:
//!
//! 1. They each get their own table, mapping the filter value to the transaction sequence number.
//!
//! 2. They also include a `sender` column, and a secondary index over the sender, filter values
//!    and the transaction sequence number.
//!
//! 3. They also include a secondary index over the transaction sequence number.
//!
//! This pattern allows us to offer a simple rule for users: If you are filtering on a single
//! value, you can do so without worrying. If you want to additionally filter by the sender, that
//! is also possible, but if you want to combine any other set of filters, you need to use a "scan
//! limit".
//!
//! ## Query construction
//!
//! Queries that filter transactions work in two phases: Identify the transaction sequence numbers
//! to fetch, and then fetch their contents. Filtering all happens in the first phase:
//!
//! - Firstly filters are broken down into individual queries targeting the appropriate lookup
//!   table. Each constituent query is expected to return a sorted run of transaction sequence
//!   numbers.
//!
//! - If a `sender` filter is included, then it is incorporated into each constituent query,
//!   leveraging their secondary indices (2), otherwise each constituent query filters only based on
//!   its filter value using the primary index (1).
//!
//! - The fact that both the primary and secondary indices contain the transaction sequence number
//!   help to ensure that the output from an index scan is already sorted, which avoids a
//!   potentially expensive materialize and sort operation.
//!
//! - If there are multiple constituent queries, they are intersected using inner joins. Postgres
//!   can occasionally pick a poor query plan for this merge, so we require that filters resulting in
//!   such merges also use a "scan limit" (see below).
//!
//! ## Scan limits
//!
//! The scan limit restricts the number of transactions considered as candidates for the results.
//! It is analogous to the page size limit, which restricts the number of results returned to the
//! user, but it operates at the top of the funnel rather than the top.
//!
//! When postgres picks a poor query plan, it can end up performing a sequential scan over all
//! candidate transactions. By limiting the size of the candidate set, we bound the work done in
//! the worse case (whereas otherwise, the worst case would grow with the history of the chain).

use super::{Cursor, TransactionBlockFilter};
use crate::{
    data::{pg::bytea_literal, Conn, DbConnection},
    filter, inner_join, query,
    raw_query::RawQuery,
    types::{
        cursor::{End, Page},
        digest::Digest,
        sui_address::SuiAddress,
        transaction_block::TransactionBlockKindInput,
        type_filter::{FqNameFilter, ModuleFilter},
    },
};
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl};
use std::fmt::Write;
use sui_indexer::schema::checkpoints;

/// Bounds on transaction sequence number, imposed by filters, cursors, and the scan limit. The
/// outermost bounds are determined by the checkpoint filters. These get translated into bounds in
/// terms of transaction sequence numbers:
///
/// ```ignore
///     tx_lo                                                             tx_hi
///     [-----------------------------------------------------------------)
/// ```
///
/// If cursors are provided, they further restrict the range of transactions to scan. Cursors are
/// exclusive, but when issuing database queries, we treat them inclusively so that we can detect
/// previous and next pages based on the existence of cursors in the results:
///
/// ```ignore
///             cursor_lo                                  cursor_hi_inclusive
///             [------------------------------------------]
/// ```
///
/// Finally, the scan limit restricts the number of transactions to scan. The scan limit can be
/// applied to either the front (forward pagination) or the back (backward pagination):
///
/// ```ignore
///             [-----scan-limit-----)---------------------|  end = Front
///             |---------------------[-----scan-limit------) end = Back
/// ```
///
/// This data structure can be used to compute the interval of transactions to look in for
/// candidates to include in a page of results. It can also determine whether the scanning has been
/// cut short on either side, implying that there is a previous or next page of values to scan.
///
/// NOTE: for consistency, assume that lowerbounds are inclusive and upperbounds are exclusive.
/// Bounds that do not follow this convention will be annotated explicitly (e.g. `lo_exclusive` or
/// `hi_inclusive`).
#[derive(Clone, Debug, Copy)]
pub(crate) struct TxBounds {
    /// The inclusive lower bound tx_sequence_number derived from checkpoint bounds. If checkpoint
    /// bounds are not provided, this will default to `0`.
    tx_lo: u64,

    /// The exclusive upper bound tx_sequence_number derived from checkpoint bounds. If checkpoint
    /// bounds are not provided, this will default to the total transaction count at the checkpoint
    /// viewed.
    tx_hi: u64,

    /// The starting cursor (aka `after`).
    cursor_lo_exclusive: Option<u64>,

    // The ending cursor (aka `before`).
    cursor_hi: Option<u64>,

    /// The number of transactions to treat as candidates, defaults to all the transactions in the
    /// range defined by the bounds above.
    scan_limit: Option<u64>,

    /// Which end of the range candidates will be scanned from.
    end: End,
}

impl TxBounds {
    /// Determines the `tx_sequence_number` range from the checkpoint bounds for a transaction block
    /// query. If no checkpoint range is specified, the default is between 0 and the
    /// `checkpoint_viewed_at`. The corresponding `tx_sequence_number` range is fetched from db, and
    /// further adjusted by cursors and scan limit. If there are any inconsistencies or invalid
    /// combinations, i.e. `after` cursor is greater than the upper bound, return None.
    pub(crate) async fn query(
        conn: &mut Conn<'_>,
        cp_after: Option<u64>,
        cp_at: Option<u64>,
        cp_before: Option<u64>,
        checkpoint_viewed_at: u64,
        scan_limit: Option<u64>,
        page: &Page<Cursor>,
    ) -> Result<Option<Self>, diesel::result::Error> {
        // Lowerbound in terms of checkpoint sequence number. We want to get the total transaction
        // count of the checkpoint before this one, or 0 if there is no previous checkpoint.
        let cp_lo = max_option([cp_after.map(|x| x.saturating_add(1)), cp_at]).unwrap_or(0);

        let cp_before_inclusive = match cp_before {
            // There are no results strictly before checkpoint 0.
            Some(0) => return Ok(None),
            Some(x) => Some(x - 1),
            None => None,
        };

        // Upperbound in terms of checkpoint sequence number. We want to get the total transaction
        // count at the end of this checkpoint. If no upperbound is given, use
        // `checkpoint_viewed_at`.
        //
        // SAFETY: we can unwrap because of the `Some(checkpoint_viewed_at)
        let cp_hi = min_option([cp_before_inclusive, cp_at, Some(checkpoint_viewed_at)]).unwrap();

        use checkpoints::dsl;
        let (tx_lo, tx_hi) = if let Some(cp_prev) = cp_lo.checked_sub(1) {
            let res: Vec<i64> = conn
                .results(move || {
                    dsl::checkpoints
                        .select(dsl::network_total_transactions)
                        .filter(dsl::sequence_number.eq_any([cp_prev as i64, cp_hi as i64]))
                        .order_by(dsl::network_total_transactions.asc())
                })
                .await?;

            // If there are not two distinct results, it means that the transaction bounds are
            // empty (lo and hi are the same), or it means that the one or other of the checkpoints
            // doesn't exist, so we can return early.
            let &[lo, hi] = res.as_slice() else {
                return Ok(None);
            };

            (lo as u64, hi as u64)
        } else {
            let res: Option<i64> = conn
                .first(move || {
                    dsl::checkpoints
                        .select(dsl::network_total_transactions)
                        .filter(dsl::sequence_number.eq(cp_hi as i64))
                })
                .await
                .optional()?;

            // If there is no result, it means that the checkpoint doesn't exist, so we can return
            // early.
            let Some(hi) = res else {
                return Ok(None);
            };

            (0, hi as u64)
        };

        // If the cursors point outside checkpoint bounds, we can return early.
        if matches!(page.after(), Some(a) if tx_hi <= a.tx_sequence_number.saturating_add(1)) {
            return Ok(None);
        }

        if matches!(page.before(), Some(b) if b.tx_sequence_number <= tx_lo) {
            return Ok(None);
        }

        Ok(Some(Self {
            tx_lo,
            tx_hi,
            cursor_lo_exclusive: page.after().map(|a| a.tx_sequence_number),
            cursor_hi: page.before().map(|b| b.tx_sequence_number),
            scan_limit,
            end: page.end(),
        }))
    }

    /// Inclusive lowerbound for range of transactions to scan, accounting for the bounds from
    /// filters and the cursor, but not scan limits. For the purposes of scanning records in the
    /// DB, cursors are treated inclusively, even though they are exclusive bounds.
    fn db_lo(&self) -> u64 {
        max_option([self.cursor_lo_exclusive, Some(self.tx_lo)]).unwrap()
    }

    /// Exclusive upperbound for range of transactions to scan, accounting for the bounds from
    /// filters and the cursor, but not scan limits. For the purposes of scanning records in the
    /// DB, cursors are treated inclusively, even though they are exclusive bounds.
    fn db_hi(&self) -> u64 {
        min_option([
            self.cursor_hi.map(|h| h.saturating_add(1)),
            Some(self.tx_hi),
        ])
        .unwrap()
    }

    /// Whether the cursor lowerbound restricts the transaction range.
    fn has_cursor_prev_page(&self) -> bool {
        self.cursor_lo_exclusive.is_some_and(|lo| self.tx_lo <= lo)
    }

    /// Whether the cursor upperbound restricts the transaction range.
    fn has_cursor_next_page(&self) -> bool {
        self.cursor_hi.is_some_and(|hi| hi < self.tx_hi)
    }

    /// Inclusive lowerbound of range of transactions to scan.
    pub(crate) fn scan_lo(&self) -> u64 {
        match (self.end, self.scan_limit) {
            (End::Front, _) | (_, None) => self.db_lo(),
            (End::Back, Some(scan_limit)) => self
                .db_hi()
                // If there is a next page, additionally scan the cursor upperbound.
                .saturating_sub(self.has_cursor_next_page() as u64)
                .saturating_sub(scan_limit)
                .max(self.db_lo()),
        }
    }

    /// Exclusive upperbound of range of transactions to scan.
    pub(crate) fn scan_hi(&self) -> u64 {
        match (self.end, self.scan_limit) {
            (End::Back, _) | (_, None) => self.db_hi(),
            (End::Front, Some(scan_limit)) => self
                .db_lo()
                // If there is a previous page, additionally scan the cursor lowerbound.
                .saturating_add(self.has_cursor_prev_page() as u64)
                .saturating_add(scan_limit)
                .min(self.db_hi()),
        }
    }

    /// The first transaction scanned, ignoring transactions pointed at by cursors.
    pub(crate) fn scan_start_cursor(&self) -> u64 {
        let skip_cursor_lo = self.end == End::Front && self.has_cursor_prev_page();
        self.scan_lo().saturating_add(skip_cursor_lo as u64)
    }

    /// The last transaction scanned, ignoring transactions pointed at by cursors.
    pub(crate) fn scan_end_cursor(&self) -> u64 {
        let skip_cursor_hi = self.end == End::Back && self.has_cursor_next_page();
        self.scan_hi().saturating_sub(skip_cursor_hi as u64 + 1)
    }

    /// Whether there are more transactions to scan before this page.
    pub(crate) fn scan_has_prev_page(&self) -> bool {
        self.tx_lo < self.scan_start_cursor()
    }

    /// Whether there are more transactions to scan after this page.
    pub(crate) fn scan_has_next_page(&self) -> bool {
        self.scan_end_cursor() + 1 < self.tx_hi
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

    let (_, mut subquery) = subqueries.pop()?;

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
            "{} <= tx_sequence_number AND tx_sequence_number < {}",
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
