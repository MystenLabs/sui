// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data::pg::bytea_literal,
    filter, query,
    raw_query::RawQuery,
    types::{
        cursor::Page,
        sui_address::SuiAddress,
        type_filter::{ModuleFilter, TypeFilter},
    },
};

use super::{Cursor, EventFilter};

fn select_ev(sender: Option<SuiAddress>, from: &str) -> RawQuery {
    let query = query!(format!(
        "SELECT tx_sequence_number, event_sequence_number FROM {}",
        from
    ));

    if let Some(sender) = sender {
        return query.filter(format!("sender = {}", bytea_literal(sender.as_slice())));
    }

    query
}

pub(crate) fn select_sender(sender: SuiAddress) -> RawQuery {
    select_ev(Some(sender), "event_senders")
}

pub(crate) fn select_event_type(event_type: &TypeFilter, sender: Option<SuiAddress>) -> RawQuery {
    match event_type {
        TypeFilter::ByModule(ModuleFilter::ByPackage(p)) => {
            filter!(
                select_ev(sender, "event_struct_package"),
                format!("package = {}", bytea_literal(p.as_slice()))
            )
        }
        TypeFilter::ByModule(ModuleFilter::ByModule(p, m)) => {
            filter!(
                select_ev(sender, "event_struct_module"),
                format!(
                    "package = {} and module = {{}}",
                    bytea_literal(p.as_slice())
                ),
                m
            )
        }
        TypeFilter::ByType(tag) => {
            let p = tag.address.to_vec();
            let m = tag.module.to_string();
            let exact = tag.to_canonical_string(/* with_prefix */ true);
            let t = exact.split("::").nth(2).unwrap();

            let (table, col_name) = if tag.type_params.is_empty() {
                ("event_struct_name", "type_name")
            } else {
                ("event_struct_name_instantiation", "type_instantiation")
            };

            filter!(
                select_ev(sender, table),
                format!(
                    "package = {} and module = {{}} and {} = {{}}",
                    bytea_literal(p.as_slice()),
                    col_name
                ),
                m,
                t
            )
        }
    }
}

pub(crate) fn select_emit_module(
    emit_module: &ModuleFilter,
    sender: Option<SuiAddress>,
) -> RawQuery {
    match emit_module {
        ModuleFilter::ByPackage(p) => {
            filter!(
                select_ev(sender, "event_emit_package"),
                format!("package = {}", bytea_literal(p.as_slice()))
            )
        }
        ModuleFilter::ByModule(p, m) => {
            filter!(
                select_ev(sender, "event_emit_module"),
                format!(
                    "package = {} and module = {{}}",
                    bytea_literal(p.as_slice())
                ),
                m
            )
        }
    }
}

/// Given a `RawQuery` representing a query for events, adds ctes and corresponding filters to
/// constrain the query. If a transaction digest is specified, we add a `tx_eq` CTE from which the
/// lower and upper bounds will draw from. This also means an additional check that the `tx_eq` CTE
/// returns a non-empty result. Otherwise, the lower bound is determined from `page.after()` or 0 by
/// default. The upper bound is determined by `page.before()`, or the current latest tx sequence
/// number.
pub(crate) fn add_bounds(
    mut query: RawQuery,
    filter: &EventFilter,
    page: &Page<Cursor>,
    tx_hi: u64,
) -> RawQuery {
    let mut ctes = vec![];

    if let Some(digest) = filter.transaction_digest {
        ctes.push(format!(
            r#"
            tx_eq AS (
                SELECT tx_sequence_number AS eq
                FROM tx_digests
                WHERE tx_digest = {}
            )
        "#,
            bytea_literal(digest.as_slice())
        ));
    }

    let (after_tx, after_ev) = page.after().map(|x| (x.tx, x.e)).unwrap_or((0, 0));

    let select_lo = if !ctes.is_empty() {
        format!("SELECT GREATEST({}, (SELECT eq FROM tx_eq))", after_tx)
    } else {
        format!("SELECT {}", after_tx)
    };

    ctes.push(format!(
        r#"
        tx_lo AS ({} AS lo),
        ev_lo AS (
            SELECT CASE
                WHEN (SELECT lo FROM tx_lo) > {} THEN {} ELSE {}
            END AS lo
        )
    "#,
        select_lo, after_tx, 0, after_ev
    ));

    let (before_tx, before_ev) = page
        .before()
        .map(|x| (x.tx, x.e))
        .unwrap_or((tx_hi, u64::MAX));

    let select_hi = if ctes.len() == 2 {
        format!("SELECT LEAST({}, (SELECT eq FROM tx_eq))", before_tx)
    } else {
        format!("SELECT {}", before_tx)
    };

    ctes.push(format!(
        r#"
        tx_hi AS ({} AS hi),
        ev_hi AS (
            SELECT CASE
                WHEN (SELECT hi FROM tx_hi) < {} THEN {} ELSE {}
            END AS hi
        )
    "#,
        select_hi,
        before_tx,
        u64::MAX,
        before_ev
    ));

    for cte in ctes {
        query = query.with(cte);
    }

    // This is needed to make sure that if a transaction digest is specified, the corresponding
    // `tx_eq` must yield a non-empty result. Otherwise, the CTE setup will fallback to defaults
    // and we will return an unexpected response.
    if filter.transaction_digest.is_some() {
        query = filter!(query, "EXISTS (SELECT 1 FROM tx_eq)");
    }

    query
        .filter("(SELECT lo FROM tx_lo) <= tx_sequence_number")
        .filter("tx_sequence_number <= (SELECT hi FROM tx_hi)")
        .filter(
            "(ROW(tx_sequence_number, event_sequence_number) >= \
     ((SELECT lo FROM tx_lo), (SELECT lo FROM ev_lo)))",
        )
        .filter(
            "(ROW(tx_sequence_number, event_sequence_number) <= \
     ((SELECT hi FROM tx_hi), (SELECT hi FROM ev_hi)))",
        )
}
