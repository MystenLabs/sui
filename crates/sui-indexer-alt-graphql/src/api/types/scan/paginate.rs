// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::connection::Connection;
use sui_indexer_alt_reader::checkpoints::CheckpointKey;

use crate::api::scalars::cursor::JsonCursor;
use crate::api::types::event::Event;
use crate::api::types::event::EventScanCursor;
use crate::api::types::event::SCEvent;
use crate::api::types::event::filter::EventFilter;
use crate::api::types::scan::ScanError;
use crate::api::types::scan::cursor::cp_ev_bounds;
use crate::api::types::scan::cursor::cp_tx_bounds;
use crate::api::types::scan::directional_iter;
use crate::api::types::scan::lookups::DigestsByCheckpoint;
use crate::api::types::scan::lookups::EventsByDigest;
use crate::api::types::scan::lookups::TransactionsByDigest;
use crate::api::types::transaction::SCTransaction;
use crate::api::types::transaction::Transaction;
use crate::api::types::transaction::TransactionContents;
use crate::api::types::transaction::TransactionCursor;
use crate::api::types::transaction::filter::TransactionFilter;
use crate::error::RpcError;
use crate::pagination::Page;
use crate::scope::Scope;

// Transactions in candidate checkpoints that match the filters, paginated.
pub(super) fn transaction_results(
    scope: Scope,
    filter: &TransactionFilter,
    page: &Page<SCTransaction>,
    candidate_cps: &[u64],
    digests: &DigestsByCheckpoint,
    native_transactions: &TransactionsByDigest,
) -> Result<Connection<String, Transaction>, RpcError<ScanError>> {
    let mut results = Vec::new();
    let limit = page.limit_with_overhead();

    'outer: for &cp_sequence_number in candidate_cps {
        let checkpoint_digests = digests
            .get(&CheckpointKey(cp_sequence_number))
            .with_context(|| {
                format!("Missing transaction digests for checkpoint {cp_sequence_number}")
            })?;

        let bounds = directional_iter(
            page,
            cp_tx_bounds(page, cp_sequence_number, checkpoint_digests.len()),
        );

        for tx_sequence_number in bounds {
            let digest = &checkpoint_digests[tx_sequence_number];
            let native_transaction = native_transactions
                .get(digest)
                .with_context(|| format!("Missing transaction data for digest {digest}"))?;

            if !filter.matches(native_transaction) {
                continue;
            }

            let cursor = TransactionCursor {
                tx_sequence_number: tx_sequence_number as u64,
                cp_sequence_number,
            };

            results.push((
                cursor,
                Transaction {
                    digest: *digest,
                    contents: TransactionContents {
                        scope: scope.clone(),
                        contents: Some(Arc::new(native_transaction.clone())),
                    },
                },
            ));

            if results.len() >= limit {
                break 'outer;
            }
        }
    }

    if !page.is_from_front() {
        results.reverse();
    }

    page.paginate_results(results, |(s, _)| JsonCursor::new(*s), |(_, tx)| Ok(tx))
}

// Events in candidate checkpoints that match the filters, paginated.
pub(super) fn event_results(
    scope: Scope,
    filter: &EventFilter,
    page: &Page<SCEvent>,
    candidate_cps: &[u64],
    digests: &DigestsByCheckpoint,
    events_by_digest: &EventsByDigest,
) -> Result<Connection<String, Event>, RpcError<ScanError>> {
    let mut results = Vec::new();
    let limit = page.limit_with_overhead();

    'outer: for &cp_sequence_number in candidate_cps {
        let checkpoint_digests = digests
            .get(&CheckpointKey(cp_sequence_number))
            .with_context(|| format!("Missing digests for checkpoint {cp_sequence_number}"))?;

        let tx_iter = directional_iter(
            page,
            cp_tx_bounds(page, cp_sequence_number, checkpoint_digests.len()),
        );

        for tx_idx in tx_iter {
            let digest = &checkpoint_digests[tx_idx];
            let Some(events_contents) = events_by_digest.get(digest) else {
                continue;
            };

            let events = events_contents.events()?;
            let ev_iter = directional_iter(
                page,
                cp_ev_bounds(page, cp_sequence_number, tx_idx, events.len()),
            );

            for ev_idx in ev_iter {
                let native = &events[ev_idx];
                if !filter.matches(native) {
                    continue;
                }

                let cursor = EventScanCursor {
                    tx_sequence_number: tx_idx as u64,
                    ev_sequence_number: ev_idx as u64,
                    cp_sequence_number,
                };

                results.push((
                    cursor,
                    Event {
                        scope: scope.clone(),
                        native: native.clone(),
                        transaction_digest: *digest,
                        sequence_number: ev_idx as u64,
                        timestamp_ms: events_contents.timestamp_ms(),
                    },
                ));

                if results.len() >= limit {
                    break 'outer;
                }
            }
        }
    }

    if !page.is_from_front() {
        results.reverse();
    }

    page.paginate_results(results, |(c, _)| JsonCursor::new(*c), |(_, e)| Ok(e))
}
