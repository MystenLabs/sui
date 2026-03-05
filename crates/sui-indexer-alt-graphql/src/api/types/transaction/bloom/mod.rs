// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ops::RangeInclusive;

use anyhow::Context as _;
use async_graphql::Context;
use diesel::QueryableByName;
use diesel::sql_types::BigInt;
use diesel::sql_types::Integer;
use diesel::sql_types::SmallInt;
use sui_indexer_alt_reader::kv_loader::KvLoader;
use sui_indexer_alt_reader::kv_loader::TransactionContents;
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_indexer_alt_schema::blooms::blocked::BlockedBloomProbe;
use sui_indexer_alt_schema::blooms::bloom::BloomProbe;
use sui_indexer_alt_schema::cp_bloom_blocks::CP_BLOCK_SIZE;
use sui_indexer_alt_schema::cp_bloom_blocks::CpBlockedBloomFilter;
use sui_indexer_alt_schema::cp_bloom_blocks::cp_block_index;
use sui_indexer_alt_schema::cp_blooms::CpBloomFilter;
use sui_package_resolver::PackageStore as _;
use sui_pg_db::query::Query;
use sui_sql_macro::query;
use sui_types::base_types::ExecutionDigests;
use sui_types::digests::TransactionDigest;

use crate::api::scalars::module_filter::ModuleFilter;
use crate::api::scalars::type_filter::TypeFilter;
use crate::api::types::event::CScanEvent;
use crate::api::types::event::Event;
use crate::api::types::event::ScanEventCursor;
use crate::api::types::event::filter::EventFilter;
use crate::api::types::transaction::CScanTransaction;
use crate::api::types::transaction::ScanTransactionCursor;
use crate::api::types::transaction::filter::TransactionFilter;
use crate::error::RpcError;
use crate::pagination::Page;
use crate::scope::Scope;

mod scan;
use scan::BloomScan;

pub(super) type EventsBySequenceNumbers = BTreeMap<ScanEventCursor, Event>;

struct CandidateTxn {
    cp_sequence_number: u64,
    tx_sequence_number: u64,
    digest: TransactionDigest,
}

pub(crate) trait CpBoundsCursor {
    fn cp_sequence_number(&self) -> u64;
}

impl CpBoundsCursor for CScanTransaction {
    fn cp_sequence_number(&self) -> u64 {
        self.cp_sequence_number
    }
}

impl CpBoundsCursor for CScanEvent {
    fn cp_sequence_number(&self) -> u64 {
        self.cp_sequence_number
    }
}

pub(super) type TransactionsBySequenceNumbers =
    BTreeMap<ScanTransactionCursor, (TransactionDigest, TransactionContents)>;

/// Scans a checkpoint range for transactions matching a filter. Uses bloom filters
/// as a pre-filter to find candidate checkpoints, then loads each candidate's
/// transactions from KV and checks against the filter (`filter.matches()`).
pub(crate) async fn transactions(
    ctx: &Context<'_>,
    scope: &Scope,
    page: &Page<CScanTransaction>,
    filter: &TransactionFilter,
    cp_bounds: RangeInclusive<u64>,
) -> Result<TransactionsBySequenceNumbers, RpcError> {
    if !validate_tx_filter(scope, filter).await {
        return Ok(BTreeMap::new());
    }

    let kv_loader: &KvLoader = ctx.data()?;
    let filter_values = filter.bloom_probe_values();
    let mut scan = BloomScan::new(page, &cp_bounds);
    let mut result = BTreeMap::new();

    while let Some(candidate_cps) = scan.next(ctx, &filter_values, page).await? {
        let txns = candidate_txns(kv_loader, &candidate_cps).await?;
        let digests = txns.iter().map(|t| t.digest).collect();
        let mut transactions_by_digest = kv_loader
            .load_many_transactions(digests)
            .await
            .context("Failed to load transactions")?;

        for txn in txns {
            let contents = transactions_by_digest
                .remove(&txn.digest)
                .with_context(|| {
                    format!("Failed to fetch Transaction with digest {}", txn.digest)
                })?;
            if filter.matches(&contents) {
                let cursor = ScanTransactionCursor {
                    tx_sequence_number: txn.tx_sequence_number,
                    cp_sequence_number: txn.cp_sequence_number,
                };
                result.insert(cursor, (txn.digest, contents));
            }
        }

        scan.update(&candidate_cps, result.len());
    }

    Ok(result)
}

/// Scans a checkpoint range for transactions matching a filter. Uses bloom filters
/// as a pre-filter to find candidate checkpoints, then loads each candidate's
/// events from KV and checks against the filter (`filter.matches()`).
pub(crate) async fn events(
    ctx: &Context<'_>,
    scope: &Scope,
    filter: &EventFilter,
    page: &Page<CScanEvent>,
    cp_bounds: RangeInclusive<u64>,
) -> Result<EventsBySequenceNumbers, RpcError> {
    if !validate_event_filter(scope, filter).await {
        return Ok(BTreeMap::new());
    }

    let kv_loader: &KvLoader = ctx.data()?;
    let filter_values = filter.bloom_probe_values();
    let mut scan = BloomScan::new(page, &cp_bounds);
    let mut result = BTreeMap::new();

    while let Some(candidate_cps) = scan.next(ctx, &filter_values, page).await? {
        let txns = candidate_txns(kv_loader, &candidate_cps).await?;
        let digests = txns.iter().map(|t| t.digest).collect();
        let events_by_digest = kv_loader
            .load_many_transaction_events(digests)
            .await
            .context("Failed to load transaction events")?;

        for txn in &txns {
            let contents = events_by_digest
                .get(&txn.digest)
                .with_context(|| format!("Missing events for transaction {}", txn.digest))?;
            for (idx, native) in contents.events()?.into_iter().enumerate() {
                if filter.matches(&native) {
                    let sequence_number = idx as u64;
                    result.insert(
                        ScanEventCursor {
                            cp_sequence_number: txn.cp_sequence_number,
                            tx_sequence_number: txn.tx_sequence_number,
                            ev_sequence_number: sequence_number,
                        },
                        Event {
                            scope: scope.clone(),
                            native,
                            transaction_digest: txn.digest,
                            sequence_number,
                            timestamp_ms: contents.timestamp_ms(),
                        },
                    );
                }
            }
        }

        scan.update(&candidate_cps, result.len());
    }

    Ok(result)
}

/// Load checkpoints for the given candidate CPs to get transaction digests with checkpoint and transaction sequence numbers.
async fn candidate_txns(
    kv_loader: &KvLoader,
    candidate_cps: &[u64],
) -> Result<Vec<CandidateTxn>, RpcError> {
    let checkpoints = kv_loader
        .load_many_checkpoints(candidate_cps.to_vec())
        .await
        .context("Failed to load checkpoint transactions")?;
    Ok(checkpoints
        .into_values()
        .flat_map(|(summary, content, _)| {
            let cp_seq = summary.sequence_number;
            content
                .enumerate_transactions(&summary)
                .map(
                    move |(tx_seq, &ExecutionDigests { transaction, .. })| CandidateTxn {
                        tx_sequence_number: tx_seq,
                        cp_sequence_number: cp_seq,
                        digest: transaction,
                    },
                )
                .collect::<Vec<_>>()
        })
        .collect())
}

/// The checkpoints that might contain the filter criteria.
///
/// Does a coarse filter over checkpoints ranges using cp_bloom_blocks,
/// then a finer filter over those ranges for checkpoint matches using cp_blooms.
pub(super) async fn candidate_cps<C>(
    ctx: &Context<'_>,
    filter_values: &[[u8; 32]],
    cp_lo: u64,
    cp_hi_inclusive: u64,
    page: &Page<C>,
    candidate_limit: usize,
) -> Result<Vec<u64>, RpcError> {
    if filter_values.is_empty() {
        return Ok(if page.is_from_front() {
            (cp_lo..=cp_hi_inclusive).take(candidate_limit).collect()
        } else {
            (cp_lo..=cp_hi_inclusive)
                .rev()
                .take(candidate_limit)
                .collect()
        });
    }
    let pg_reader: &PgReader = ctx.data()?;
    let mut conn = pg_reader
        .connect()
        .await
        .context("Failed to connect to database for bloom filter scan")?;

    let cp_block_lo = cp_block_index(cp_lo);
    let cp_block_hi_inclusive = cp_block_index(cp_hi_inclusive);

    // Block index and probe for each block in the range. Seeds vary per block, so we must
    // construct probes for each block.
    let probes_by_block = (cp_block_lo..=cp_block_hi_inclusive).flat_map(|id| {
        CpBlockedBloomFilter::probe(id as u128, filter_values)
            .into_iter()
            .map(move |probe| (id, probe))
    });

    let q_block_probes = cp_block_probes_sql(probes_by_block);
    let q_bloom_check = cp_bloom_check_sql(&CpBloomFilter::probe(filter_values));

    let block_size = CP_BLOCK_SIZE as i64;
    let adjusted_limit = candidate_limit as i64;

    // For each unique (cp_block_index, bloom_block_index) probe pair, fetch the bloom block
    // row once via index lookup, then check all bit probes against it.
    // The NOT EXISTS short-circuits on the first failing probe per pair.
    // The GROUP BY / HAVING ensures ALL bloom_block_indices pass per cp_block_index.
    let matched_blocks = query!(
        r#"
        SELECT
            cp_bloom_blocks.cp_block_index,
            cp_bloom_blocks.cp_block_index * {BigInt} AS cp_lo,
            cp_bloom_blocks.cp_block_index * {BigInt} + {BigInt} - 1 AS cp_hi_inclusive
        FROM
            (SELECT DISTINCT cp_block_index, bloom_block_index, bloom_count
             FROM block_byte_probes) unique_probes
        JOIN cp_bloom_blocks USING (cp_block_index, bloom_block_index)
        WHERE NOT EXISTS (
            SELECT 1
            FROM block_byte_probes
            WHERE block_byte_probes.cp_block_index = cp_bloom_blocks.cp_block_index
                AND block_byte_probes.bloom_block_index = cp_bloom_blocks.bloom_block_index
                AND (get_byte(
                    cp_bloom_blocks.bloom_filter,
                    block_byte_probes.byte_pos % length(cp_bloom_blocks.bloom_filter)
                ) & block_byte_probes.bit_mask) <> block_byte_probes.bit_mask
        )
        GROUP BY
            cp_bloom_blocks.cp_block_index, unique_probes.bloom_count
        HAVING
            COUNT(*) = unique_probes.bloom_count
        ORDER BY
            cp_lo {}
        LIMIT
            {BigInt}
        "#,
        block_size,
        block_size,
        block_size,
        page.order_by_direction(),
        adjusted_limit,
    );

    // For each matched block, scan cp_blooms until we have adjusted_limit checkpoints that
    // match the probe.
    let query = query!(
        r#"
        WITH
        block_byte_probes AS ({}),

        matched_blocks AS ({})

        SELECT
            cp_sequence_number::BIGINT
        FROM
            matched_blocks
        CROSS JOIN LATERAL (
            SELECT
                cp_sequence_number
            FROM
                cp_blooms
            WHERE
                cp_sequence_number BETWEEN GREATEST(matched_blocks.cp_lo, {BigInt})
                    AND LEAST(matched_blocks.cp_hi_inclusive, {BigInt})
                AND {}
            ORDER BY
                cp_sequence_number {}
        ) cp_blooms
        LIMIT
            {BigInt}
        "#,
        q_block_probes,
        matched_blocks,
        cp_lo as i64,
        cp_hi_inclusive as i64,
        q_bloom_check,
        page.order_by_direction(),
        adjusted_limit,
    );

    #[derive(QueryableByName)]
    struct CpResult {
        #[diesel(sql_type = BigInt)]
        cp_sequence_number: i64,
    }

    let results: Vec<CpResult> = conn
        .results(query)
        .await
        .context("Failed to execute bloom filter scan query")?;
    Ok(results
        .into_iter()
        .map(|r| r.cp_sequence_number as u64)
        .collect())
}

/// SQL fragment that produces rows of probes (cp_block_index, bloom_block_index, byte_pos, bit_mask, bloom_count)
/// using UNNEST. `bloom_count` is the number of distinct bloom_block_indices per cp_block_index,
/// used in the HAVING clause to ensure all bloom blocks pass.
fn cp_block_probes_sql(probes: impl Iterator<Item = (i64, BlockedBloomProbe)>) -> Query<'static> {
    let mut cp_block_indices = vec![];
    let mut bloom_indicies = vec![];
    let mut byte_offsets = vec![];
    let mut bit_masks = vec![];
    let mut bloom_counts = vec![];

    // Collect probes grouped by cp_block_index to compute bloom_count.
    let mut probes_by_block: BTreeMap<i64, Vec<BlockedBloomProbe>> = BTreeMap::new();
    for (cp_block_index, blocked_probe) in probes {
        probes_by_block
            .entry(cp_block_index)
            .or_default()
            .push(blocked_probe);
    }

    for (cp_block_index, block_probes) in &probes_by_block {
        let bloom_count: i64 = block_probes
            .iter()
            .map(|p| p.block_idx)
            .collect::<BTreeSet<_>>()
            .len() as i64;
        for blocked_probe in block_probes {
            for &(offset, mask) in &blocked_probe.probe.bit_probes {
                cp_block_indices.push(*cp_block_index);
                bloom_indicies.push(blocked_probe.block_idx as i16);
                byte_offsets.push(offset as i32);
                bit_masks.push(mask as i32);
                bloom_counts.push(bloom_count);
            }
        }
    }

    query!(
        r#"
        SELECT
            UNNEST({Array<BigInt>}) cp_block_index,
            UNNEST({Array<SmallInt>}) bloom_block_index,
            UNNEST({Array<Integer>}) byte_pos,
            UNNEST({Array<Integer>}) bit_mask,
            UNNEST({Array<BigInt>}) bloom_count
        "#,
        cp_block_indices,
        bloom_indicies,
        byte_offsets,
        bit_masks,
        bloom_counts,
    )
}

/// Check if all filter values are present in a checkpoint's bloom filter.
fn cp_bloom_check_sql(probe: &BloomProbe) -> Query<'static> {
    if probe.bit_probes.is_empty() {
        return query!("TRUE");
    }

    let mut condition = query!("TRUE");
    for &(offset, mask) in &probe.bit_probes {
        condition += query!(
            " AND (get_byte(cp_blooms.bloom_filter, {Integer} % length(cp_blooms.bloom_filter)) & {Integer}) = {Integer}",
            offset as i32,
            mask as i32,
            mask as i32,
        );
    }
    condition
}

/// Validates that the module/function referenced by a transaction filter exists on-chain.
/// Returns false if the module or function doesn't exist within a successfully loaded package,
/// allowing callers to short-circuit and return empty results. Returns true if the package
/// cannot be fetched (e.g. kv_packages not populated), so the scan proceeds normally.
async fn validate_tx_filter(scope: &Scope, filter: &TransactionFilter) -> bool {
    let Some(function) = &filter.function else {
        return true;
    };
    let Some(module_name) = function.module() else {
        // Package-only filter — can't validate further without scanning all modules.
        return true;
    };
    let resolver = scope.package_resolver();
    let Ok(package) = resolver
        .package_store()
        .fetch(function.package().into())
        .await
    else {
        // Package not in store (e.g. kv_packages not populated) — can't validate, proceed.
        return true;
    };
    let Ok(module) = package.module(module_name) else {
        return false;
    };
    match function.name() {
        Some(name) => module.function_def(name).is_ok_and(|f| f.is_some()),
        None => true,
    }
}

/// Validates that the module or event type referenced by an event filter exists on-chain.
/// Returns false only when the package was successfully loaded but the module/type doesn't
/// exist within it. Returns true if the package cannot be fetched, so the scan proceeds.
async fn validate_event_filter(scope: &Scope, filter: &EventFilter) -> bool {
    if let Some(module_filter) = &filter.module {
        if let ModuleFilter::Module(package_addr, module_name) = module_filter {
            let resolver = scope.package_resolver();
            let Ok(package) = resolver.package_store().fetch((*package_addr).into()).await else {
                return true;
            };
            return package.module(module_name).is_ok();
        }
    }
    if let Some(type_filter) = &filter.type_ {
        match type_filter {
            TypeFilter::Package(_) => return true,
            TypeFilter::Module(package_addr, module_name) => {
                let resolver = scope.package_resolver();
                let Ok(package) = resolver.package_store().fetch((*package_addr).into()).await
                else {
                    return true;
                };
                return package.module(module_name).is_ok();
            }
            TypeFilter::Type(struct_tag) => {
                let resolver = scope.package_resolver();
                let Ok(package) = resolver
                    .package_store()
                    .fetch(struct_tag.address.into())
                    .await
                else {
                    return true;
                };
                let Ok(module) = package.module(struct_tag.module.as_str()) else {
                    return false;
                };
                return module
                    .data_def(struct_tag.name.as_str())
                    .is_ok_and(|d| d.is_some());
            }
        }
    }
    true
}
