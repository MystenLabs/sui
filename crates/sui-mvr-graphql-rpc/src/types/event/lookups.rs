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

/// Adds filters to bound an events query from above and below based on cursors and filters. The
/// query will always at least be bounded by `tx_hi`, the current exclusive upperbound on
/// transaction sequence numbers, based on the consistency cursor.
pub(crate) fn add_bounds(
    mut query: RawQuery,
    tx_digest_filter: &Option<Digest>,
    page: &Page<Cursor>,
    tx_hi: i64,
) -> RawQuery {
    query = filter!(query, format!("tx_sequence_number < {}", tx_hi));

    if let Some(after) = page.after() {
        query = filter!(
            query,
            format!(
                "ROW(tx_sequence_number, event_sequence_number) >= ({}, {})",
                after.tx, after.e
            )
        );
    }

    if let Some(before) = page.before() {
        query = filter!(
            query,
            format!(
                "ROW(tx_sequence_number, event_sequence_number) <= ({}, {})",
                before.tx, before.e
            )
        );
    }

    if let Some(digest) = tx_digest_filter {
        query = filter!(
            query,
            format!(
                "tx_sequence_number = (SELECT tx_sequence_number FROM tx_digests WHERE tx_digest = {})",
                bytea_literal(digest.as_slice()),
            )
        );
    }

    query
}
