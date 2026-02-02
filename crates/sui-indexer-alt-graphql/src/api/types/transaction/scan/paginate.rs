// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;
use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::connection::Connection;
use itertools::Either;
use sui_indexer_alt_reader::checkpoints::CheckpointKey;

use crate::api::scalars::cursor::JsonCursor;
use crate::api::types::transaction::SCTransaction;
use crate::api::types::transaction::Transaction;
use crate::api::types::transaction::TransactionContents;
use crate::api::types::transaction::TransactionCursor;
use crate::api::types::transaction::filter::TransactionFilter;
use crate::api::types::transaction::scan::ScanError;
use crate::api::types::transaction::scan::lookup::DigestsByCheckpoint;
use crate::api::types::transaction::scan::lookup::TransactionsByDigest;
use crate::error::RpcError;
use crate::pagination::Page;
use crate::scope::Scope;

pub(super) fn results(
    scope: Scope,
    filter: &TransactionFilter,
    page: &Page<SCTransaction>,
    candidate_cps: Vec<u64>,
    digests: DigestsByCheckpoint,
    native_transactions: TransactionsByDigest,
) -> Result<Connection<String, Transaction>, RpcError<ScanError>> {
    let mut results = Vec::new();
    let limit = page.limit_with_overhead();

    'outer: for cp_sequence_number in candidate_cps {
        let checkpoint_digests = digests
            .get(&CheckpointKey(cp_sequence_number))
            .with_context(|| {
                format!("Missing transaction digests for checkpoint {cp_sequence_number}")
            })?;

        let bounds: Either<Range<usize>, std::iter::Rev<Range<usize>>> = if page.is_from_front() {
            Either::Left(cp_tx_bounds(
                page,
                cp_sequence_number,
                checkpoint_digests.len(),
            ))
        } else {
            Either::Right(cp_tx_bounds(page, cp_sequence_number, checkpoint_digests.len()).rev())
        };

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

fn cp_tx_bounds(
    page: &Page<SCTransaction>,
    cp_sequence_number: u64,
    tx_count: usize,
) -> Range<usize> {
    let tx_lo = page
        .after()
        .filter(|c| c.cp_sequence_number == cp_sequence_number)
        .map(|c| c.tx_sequence_number as usize)
        .unwrap_or(0)
        .min(tx_count);

    let tx_hi = page
        .before()
        .filter(|c| c.cp_sequence_number == cp_sequence_number)
        .map(|c| (c.tx_sequence_number as usize).saturating_add(1))
        .unwrap_or(tx_count)
        .max(tx_lo)
        .min(tx_count);

    tx_lo..tx_hi
}
