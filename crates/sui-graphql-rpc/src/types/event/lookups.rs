// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data::pg::bytea_literal,
    filter, query,
    raw_query::RawQuery,
    types::{
        cursor::Page,
        digest::Digest,
        sui_address::SuiAddress,
        type_filter::{ModuleFilter, TypeFilter},
    },
};

use std::fmt::Write;

use super::Cursor;

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
            let package = tag.address;
            let module = tag.module.to_string();
            let mut name = tag.name.as_str().to_owned();
            let (table, col_name) = if tag.type_params.is_empty() {
                ("event_struct_name", "type_name")
            } else {
                let mut prefix = "<";
                for param in &tag.type_params {
                    name += prefix;
                    // SAFETY: write! to String always succeeds.
                    write!(
                        name,
                        "{}",
                        param.to_canonical_display(/* with_prefix */ true)
                    )
                    .unwrap();
                    prefix = ", ";
                }
                name += ">";
                ("event_struct_instantiation", "type_instantiation")
            };

            filter!(
                select_ev(sender, table),
                format!(
                    "package = {} and module = {{}} and {} = {{}}",
                    bytea_literal(package.as_slice()),
                    col_name
                ),
                module,
                name
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
/// constrain the query. By default, if neither a transaction digest nor an `after` cursor are
/// specified, then the events query will only have an upper bound, determined by the `before`
/// cursor or the current latest tx sequence number. If a transaction digest is specified, we add a
/// `tx_eq` CTE that the lower and upper bounds will compare to. This also means an additional check
/// that the `tx_eq` CTE returns a non-empty result.
///
/// `tx_hi` represents the current latest tx sequence number.
pub(crate) fn add_bounds(
    mut query: RawQuery,
    tx_digest_filter: &Option<Digest>,
    page: &Page<Cursor>,
    tx_hi: u64,
) -> RawQuery {
    let mut ctes = vec![];

    let mut has_digest_cte = false;
    if let Some(digest) = tx_digest_filter {
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
        has_digest_cte = true;
    };

    let mut has_select_lo = false;
    let select_lo = match (page.after(), has_digest_cte) {
        (Some(after), _) => {
            // if both a digest and after cursor are present, then select the larger tx sequence
            // number
            let select_tx_lo = if has_digest_cte {
                format!("SELECT GREATEST({}, (SELECT eq FROM tx_eq))", after.tx)
            } else {
                format!("SELECT {}", after.tx)
            };
            // If `tx_lo` matches `after` cursor, then we should use the `after` cursor's event
            // sequence number
            let select_ev_lo = format!(
                "SELECT CASE WHEN (SELECT lo FROM tx_lo) = {after_tx} THEN 0 ELSE {after_ev} END",
                after_tx = after.tx,
                after_ev = after.e
            );
            Some(format!(
                r#"
                tx_lo AS ({select_tx_lo} AS lo),
                ev_lo AS ({select_ev_lo} AS lo)
                "#
            ))
        }
        // No `after` cursor, has digest
        (None, true) => Some(
            r#"
            tx_lo AS (SELECT eq AS lo FROM tx_eq),
            ev_lo AS (SELECT 0 AS lo)
            "#
            .to_string(),
        ),
        // Neither, don't need lower bounds
        (None, false) => None,
    };

    if let Some(select_lo) = select_lo {
        ctes.push(select_lo);
        has_select_lo = true;
    };

    let (select_tx_hi, select_ev_hi) = match page.before() {
        Some(before) => {
            let tx_hi = if has_digest_cte {
                // Select the smaller of the digest or before cursor, to exclude txs between the two,
                // if any
                format!("SELECT LEAST({}, (SELECT eq FROM tx_eq))", before.tx)
            } else {
                format!("SELECT {}", before.tx)
            };
            let ev_hi = format!(
                // check for equality so that if the digest and before cursor are the same tx, we
                // don't miss the before cursor's event sequence number
                "SELECT CASE WHEN (SELECT hi FROM tx_hi) = {before_tx} THEN {u64_max} ELSE {before_ev} END",
                before_tx=before.tx,
                u64_max=u64::MAX,
                before_ev=before.e
            );
            (tx_hi, ev_hi)
        }
        None => (
            format!("SELECT {}", tx_hi.to_string()),
            format!("SELECT {}", u64::MAX.to_string()),
        ),
    };

    ctes.push(format!(
        r#"
        tx_hi AS ({select_tx_hi} AS hi),
        ev_hi AS ({select_ev_hi} AS hi)
    "#,
    ));

    for cte in ctes {
        query = query.with(cte);
    }

    // This is needed to make sure that if a transaction digest is specified, the corresponding
    // `tx_eq` must yield a non-empty result. Otherwise, the CTE setup will fallback to defaults
    // and we will return an unexpected response.
    if has_digest_cte {
        query = filter!(query, "EXISTS (SELECT 1 FROM tx_eq)");
    }

    if has_select_lo {
        query = query
            .filter("(SELECT lo FROM tx_lo) <= tx_sequence_number")
            .filter(
                "(ROW(tx_sequence_number, event_sequence_number) >= \
     ((SELECT lo FROM tx_lo), (SELECT lo FROM ev_lo)))",
            );
    }

    query
        .filter("tx_sequence_number <= (SELECT hi FROM tx_hi)")
        .filter(
            "(ROW(tx_sequence_number, event_sequence_number) <= \
     ((SELECT hi FROM tx_hi), (SELECT hi FROM ev_hi)))",
        )
}
