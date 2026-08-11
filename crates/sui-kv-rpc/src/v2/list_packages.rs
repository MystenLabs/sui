// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `MovePackageService.ListPackages`: every Move package write (publishes and upgrades alike) in
//! a checkpoint range, in transaction order, one item per write.
//!
//! Matching transactions come from the `AnyPackageWrite` bitmap dimension — a
//! transaction-granular index — and each is expanded into its package writes from the
//! transaction's effects, in effects order, which `write_index` is defined over. The expansion
//! mirrors the unfiltered `ListEvents` path (a transaction-granular source serving
//! sub-transaction items); the discovery mirrors the filtered `ListTransactions` path.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream::BoxStream;
use sui_inverted_index::ScanDirection;
use sui_inverted_index::ScanStop;
use sui_kvstore::BitmapIndexSpec;
use sui_kvstore::TransactionData;
use sui_kvstore::tables::transactions::col;
use sui_rpc::proto::sui::rpc::v2::ListPackagesRequest;
use sui_rpc::proto::sui::rpc::v2::ListPackagesResponse;
use sui_rpc::proto::sui::rpc::v2::PackageVersion;
use sui_rpc::proto::sui::rpc::v2::PackageWriteFilter;
use sui_rpc::proto::sui::rpc::v2::QueryEnd;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2::TransactionFilter;
use sui_rpc::proto::sui::rpc::v2::TransactionLiteral;
use sui_rpc::proto::sui::rpc::v2::TransactionTerm;
use sui_rpc::proto::sui::rpc::v2::Watermark;
use sui_rpc::proto::sui::rpc::v2::transaction_literal::Predicate;
use sui_rpc_api::RpcError;
use sui_rpc_api::ledger_history::query_options::CheckpointRange;
use sui_rpc_api::ledger_history::query_options::PackagePosition;
use sui_rpc_api::ledger_history::query_options::PackageScanBounds;
use sui_rpc_api::ledger_history::query_options::QueryOptions;
use sui_rpc_api::ledger_history::query_options::RangeExhaustion;
use sui_rpc_api::ledger_history::query_options::ResolvedPackageRange;
use sui_rpc_api::ledger_history::watermark::ScanTerminal;
use sui_rpc_api::ledger_history::watermark::advance_covered_bound_before_checkpoint;
use sui_rpc_api::ledger_history::watermark::boundary_watermark;
use sui_rpc_api::ledger_history::watermark::item_watermark;
use sui_rpc_api::ledger_history::watermark::scan_frontier_cursor_cp;
use sui_rpc_cursor::Position;
use sui_types::base_types::ObjectRef;
use sui_types::effects::TransactionEffectsAPI;
use tracing::Instrument;
use tracing::debug_span;
use tracing::info;

use crate::bigtable_client::BigTableClient;
use crate::config::PipelineStage;
use crate::operation::QueryContext;
use crate::pipeline::RenderAheadError;
use crate::pipeline::ResolvedScanStop;
use crate::pipeline::ResolvedWatermarked;
use crate::pipeline::Watermarked;
use crate::pipeline::pipelined_chunks;
use crate::pipeline::render_ahead;
use crate::pipeline::resolve_scan_watermarks;
use crate::pipeline::take_items;
use crate::v2::list_transactions::fetch_transactions;
use crate::v2::list_transactions::fetch_tx_seq_digests;

/// Metrics resolution label: `ListPackages` serves refs only, with no per-item
/// rendering choices.
const RESOLUTION: &str = "refs";

pub(crate) type ListPackagesStream = BoxStream<'static, Result<ListPackagesResponse, RpcError>>;

pub(crate) async fn list_packages(
    ctx: QueryContext,
    request: ListPackagesRequest,
) -> Result<ListPackagesStream, RpcError> {
    let started = Instant::now();
    let client: BigTableClient = ctx.client().clone();
    let checkpoint_hi_exclusive = ctx.checkpoint_hi_exclusive();
    let lh = ctx.ledger_history();
    let endpoint = lh.list_packages();
    let tx_seq_digest_stage = ctx.stage(PipelineStage::TxSeqDigest);
    let transactions_stage = ctx.stage(PipelineStage::Transactions);

    let checkpoint_range = CheckpointRange::from_request(
        request.start_checkpoint,
        request.end_checkpoint,
        checkpoint_hi_exclusive,
    )?;
    let options = QueryOptions::packages_from_proto(
        request.options.as_ref(),
        endpoint.default_limit_items,
        endpoint.max_limit_items,
    )?;
    let limit_items = options.limit_items;
    let ordering = options.ordering;
    let direction = options.scan_direction();

    let package_range = resolve_package_range(&client, checkpoint_range, &options)
        .instrument(debug_span!("resolve_package_range"))
        .await?;
    let exhaustion = package_range.exhaustion;
    let entry_checkpoint = package_range.entry_checkpoint;
    let range_end_checkpoint = package_range.end_checkpoint;
    let range_end_position = package_range.end_position;
    let bounds = package_range.bounds;

    if package_range.is_empty() {
        info!(
            limit_items,
            ?ordering,
            emitted = 0,
            elapsed_ms = started.elapsed().as_millis(),
            "list_packages: empty range"
        );
        // Empty resolved ranges still surface their terminal cursor, but
        // natural completion claims no checkpoint.
        let response = range_end_response(
            &options,
            exhaustion,
            package_position_cursor(range_end_checkpoint, range_end_position),
            None,
            true,
        )
        .0;
        return Ok(async_stream::try_stream! {
            ctx.inc_stream_watermark_frames();
            ctx.observe_stream_first_frame_latency(RESOLUTION, started.elapsed());
            let yield_started = Instant::now();
            yield response;
            ctx.observe_stream_frame_yield_wait(RESOLUTION, yield_started.elapsed());
        }
        .boxed());
    }

    let Some(tx_range) = bounds.tx_range() else {
        // Non-empty bounds always cover at least one transaction.
        return Err(RpcError::new(
            tonic::Code::Internal,
            "list_packages: non-empty bounds with no transaction range",
        ));
    };

    // Stage A: matched transaction sequence numbers from the `AnyPackageWrite`
    // bitmap. The take is at transaction granularity with headroom for the (at
    // most two) boundary transactions whose cursor-covered writes expand to
    // nothing: every other matched transaction contributes at least one
    // in-bounds write, so `limit_items` writes are always reachable within
    // `limit_items + 2` transactions.
    let scan_budget = ctx.scan_budget(BitmapIndexSpec::tx());
    let query = ctx.transaction_filter_query(&package_write_filter())?;
    let seq_stream = client.eval_bitmap_query_stream(
        query,
        tx_range,
        BitmapIndexSpec::tx(),
        direction,
        scan_budget,
        ctx.bitmap_skip_policy(),
        ctx.bitmap_scan_observer(),
    );
    let seq_stream = take_items(seq_stream, limit_items.saturating_add(2));

    // Stage B: tx sequence numbers -> tx_seq_digest rows.
    let digest_stream = pipelined_chunks(
        seq_stream,
        tx_seq_digest_stage.chunk_size,
        tx_seq_digest_stage.concurrency,
        {
            let client = client.clone();
            move |seqs| {
                let client = client.clone();
                async move {
                    fetch_tx_seq_digests(client, seqs)
                        .await
                        .map(|s| s.map_err(ScanStop::Fault).boxed())
                        .map_err(ScanStop::Fault)
                }
            }
        },
    );

    // Stage C: tx_seq_digest rows -> transaction bodies. Only the effects
    // identify package writes; the checkpoint number stamps cursors.
    let columns: Arc<[&'static str]> = Arc::from([col::EFFECTS, col::CHECKPOINT_NUMBER]);
    let tx_stream = pipelined_chunks(
        digest_stream,
        transactions_stage.chunk_size,
        transactions_stage.concurrency,
        {
            let client = client.clone();
            let columns = columns.clone();
            move |rows| {
                let client = client.clone();
                let columns = columns.clone();
                async move {
                    fetch_transactions(client, columns, rows)
                        .await
                        .map(|s| s.map_err(ScanStop::Fault).boxed())
                        .map_err(ScanStop::Fault)
                }
            }
        },
    );

    // Stage D: expand each transaction into its package writes, skipping the
    // coordinates outside the resolved bounds (a re-included boundary
    // transaction's already-served writes). Watermarks pass through, lifted
    // into package coordinates at the start of their frontier transaction.
    let ascending = options.is_ascending();
    let write_stream: BoxStream<
        'static,
        Result<Watermarked<PackageWriteItem, PackagePosition>, ScanStop>,
    > = async_stream::try_stream! {
        futures::pin_mut!(tx_stream);
        while let Some(item) = tx_stream.next().await {
            match item? {
                Watermarked::Item((tx_seq, _tx_offset, tx)) => {
                    for write in expand_package_writes(tx_seq, &tx, bounds, ascending)? {
                        yield Watermarked::Item(write);
                    }
                }
                Watermarked::Watermark(tx_seq) => {
                    yield Watermarked::Watermark(PackagePosition::start_of_tx(tx_seq));
                }
            }
        }
    }
    .boxed();

    let tx_resolver = client.tx_wm_resolver(direction);
    let mut resolved_stream = render_ahead(
        resolve_scan_watermarks(
            write_stream,
            move |position: PackagePosition| tx_resolver(position.tx_seq),
            PackagePosition::start_of_tx,
        ),
        endpoint.render_ahead,
        move |write: PackageWriteItem| async move {
            let render_started = Instant::now();
            Ok::<_, RpcError>((write, render_started.elapsed()))
        },
    );

    Ok(async_stream::try_stream! {
        let mut emitted = 0usize;
        let mut first_frame_emitted = false;
        let mut covered_checkpoint_bound: Option<u64> = None;
        let terminal_reason = loop {
            let Some(item) = ctx.next_response_item(RESOLUTION, &mut resolved_stream).await else {
                let (response, reason) = range_end_response(
                    &options,
                    exhaustion,
                    package_position_cursor(range_end_checkpoint, range_end_position),
                    covered_checkpoint_bound,
                    false,
                );
                ctx.inc_stream_watermark_frames();
                if !first_frame_emitted {
                    ctx.observe_stream_first_frame_latency(RESOLUTION, started.elapsed());
                }
                let yield_started = Instant::now();
                yield response;
                ctx.observe_stream_frame_yield_wait(RESOLUTION, yield_started.elapsed());
                break reason;
            };
            match item {
                Ok(ResolvedWatermarked::Item((write, render_elapsed))) => {
                    covered_checkpoint_bound = advance_covered_bound_before_checkpoint(
                        covered_checkpoint_bound,
                        write.checkpoint,
                        entry_checkpoint,
                        &options,
                    );
                    let watermark = item_watermark(
                        package_position_cursor(write.checkpoint, write.position),
                        covered_checkpoint_bound,
                    );
                    emitted += 1;
                    ctx.observe_response_render(RESOLUTION, render_elapsed);
                    let mut response = package_item_response(&write.object_ref, watermark);
                    let item_limit = emitted == limit_items;
                    if item_limit {
                        let mut end = QueryEnd::default();
                        end.reason = Some(QueryEndReason::ItemLimit as i32);
                        response.end = Some(end);
                    }
                    ctx.observe_response_page_bytes(RESOLUTION, prost::Message::encoded_len(&response));
                    if !first_frame_emitted {
                        ctx.observe_stream_first_frame_latency(RESOLUTION, started.elapsed());
                        first_frame_emitted = true;
                    }
                    let yield_started = Instant::now();
                    yield response;
                    ctx.observe_stream_frame_yield_wait(RESOLUTION, yield_started.elapsed());
                    if item_limit {
                        break QueryEndReason::ItemLimit;
                    }
                }
                Ok(ResolvedWatermarked::Watermark {
                    position,
                    cp: checkpoint_at_frontier,
                }) => {
                    let watermark = package_frontier_watermark(
                        &options,
                        direction,
                        entry_checkpoint,
                        &mut covered_checkpoint_bound,
                        position,
                        Some(checkpoint_at_frontier),
                    )?;
                    let response = watermark_response(watermark);
                    ctx.inc_stream_watermark_frames();
                    if !first_frame_emitted {
                        ctx.observe_stream_first_frame_latency(RESOLUTION, started.elapsed());
                        first_frame_emitted = true;
                    }
                    let yield_started = Instant::now();
                    yield response;
                    ctx.observe_stream_frame_yield_wait(RESOLUTION, yield_started.elapsed());
                }
                Err(RenderAheadError::Upstream(stop)) => {
                    let response = terminal_response_from_scan_stop(
                        stop,
                        &options,
                        direction,
                        entry_checkpoint,
                        &mut covered_checkpoint_bound,
                    )?;
                    ctx.inc_stream_watermark_frames();
                    if !first_frame_emitted {
                        ctx.observe_stream_first_frame_latency(RESOLUTION, started.elapsed());
                    }
                    let yield_started = Instant::now();
                    yield response;
                    ctx.observe_stream_frame_yield_wait(RESOLUTION, yield_started.elapsed());
                    break QueryEndReason::ScanLimit;
                }
                Err(RenderAheadError::Render(error)) => Err(error)?,
            }
        };
        info!(
            limit_items,
            ?ordering,
            emitted,
            ?terminal_reason,
            elapsed_ms = started.elapsed().as_millis(),
            "list_packages: done"
        );
    }
    .boxed())
}

/// One package write ready to serve: its scan coordinate, the written ref from
/// the effects, and the checkpoint that stamps its cursor.
#[derive(Clone, Copy, Debug)]
struct PackageWriteItem {
    position: PackagePosition,
    object_ref: ObjectRef,
    checkpoint: u64,
}

/// The `AnyPackageWrite` bitmap dimension: every transaction that wrote a Move
/// package, first publishes and upgrades alike.
fn package_write_filter() -> TransactionFilter {
    let mut literal = TransactionLiteral::default();
    literal.predicate = Some(Predicate::PackageWrite(PackageWriteFilter::default()));

    TransactionFilter::default().with_terms(vec![
        TransactionTerm::default().with_literals(vec![literal]),
    ])
}

/// Expand a transaction's effects into its in-bounds package writes: the
/// written refs of published packages, indexed in effects order (which
/// `write_index` is defined over), reversed within the transaction for
/// descending scans, and filtered to the resolved bounds.
fn expand_package_writes(
    tx_seq: u64,
    tx: &TransactionData,
    bounds: PackageScanBounds,
    ascending: bool,
) -> Result<Vec<PackageWriteItem>, ScanStop> {
    let effects = tx.effects.as_ref().ok_or_else(|| {
        ScanStop::Fault(anyhow::anyhow!(
            "list_packages: matched transaction {tx_seq} has no effects column"
        ))
    })?;

    let published: BTreeSet<_> = effects.published_packages().into_iter().collect();
    let mut writes: Vec<(u32, ObjectRef)> = effects
        .written()
        .into_iter()
        .filter(|(id, _, _)| published.contains(id))
        .enumerate()
        .map(|(i, write)| (i as u32, write))
        .collect();
    if !ascending {
        writes.reverse();
    }

    Ok(writes
        .into_iter()
        .filter_map(|(write_index, object_ref)| {
            let position = PackagePosition {
                tx_seq,
                write_index,
            };
            bounds.contains(position).then_some(PackageWriteItem {
                position,
                object_ref,
                checkpoint: tx.checkpoint_number,
            })
        })
        .collect())
}

fn package_position_cursor(checkpoint: u64, position: PackagePosition) -> Position {
    Position::Packages {
        checkpoint,
        tx_seq: position.tx_seq,
        write_index: position.write_index,
    }
}

fn package_version_proto((object_id, version, _): &ObjectRef) -> PackageVersion {
    let mut package = PackageVersion::default();
    package.package_id = Some(object_id.to_canonical_string(true));
    package.version = Some(version.value());
    package
}

fn watermark_response(watermark: Watermark) -> ListPackagesResponse {
    let mut response = ListPackagesResponse::default();
    response.watermark = Some(watermark);
    response
}

fn package_item_response(object_ref: &ObjectRef, watermark: Watermark) -> ListPackagesResponse {
    let mut response = ListPackagesResponse::default();
    response.package = Some(package_version_proto(object_ref));
    response.watermark = Some(watermark);
    response
}

fn end_response(watermark: Watermark, reason: QueryEndReason) -> ListPackagesResponse {
    let mut end = QueryEnd::default();
    end.reason = Some(reason as i32);

    let mut response = ListPackagesResponse::default();
    response.watermark = Some(watermark);
    response.end = Some(end);
    response
}

/// Trailing terminal frame for range exhaustion. Reason and watermark derive
/// from one `ScanTerminal`, so they cannot disagree. Natural completion of an
/// empty interval retains its cursor but claims no checkpoint.
fn range_end_response(
    options: &QueryOptions,
    exhaustion: RangeExhaustion,
    position: Position,
    covered_checkpoint_bound: Option<u64>,
    interval_empty: bool,
) -> (ListPackagesResponse, QueryEndReason) {
    let terminal = ScanTerminal::from_range_exhaustion(exhaustion, position, interval_empty);
    let reason = terminal.reason();
    (
        end_response(
            terminal.into_watermark(options, covered_checkpoint_bound),
            reason,
        ),
        reason,
    )
}

fn package_frontier_watermark(
    options: &QueryOptions,
    direction: ScanDirection,
    entry_checkpoint: u64,
    covered_checkpoint_bound: &mut Option<u64>,
    position: PackagePosition,
    checkpoint_at_frontier: Option<u64>,
) -> Result<Watermark, RpcError> {
    if let Some(checkpoint) = checkpoint_at_frontier {
        *covered_checkpoint_bound = advance_covered_bound_before_checkpoint(
            *covered_checkpoint_bound,
            checkpoint,
            entry_checkpoint,
            options,
        );
    }
    let cursor_checkpoint =
        scan_frontier_cursor_cp(checkpoint_at_frontier, position.tx_seq, direction).ok_or_else(
            || {
                RpcError::new(
                    tonic::Code::Internal,
                    format!(
                        "package scan frontier {}/{} has no checkpoint mapping",
                        position.tx_seq, position.write_index
                    ),
                )
            },
        )?;
    Ok(boundary_watermark(
        package_position_cursor(cursor_checkpoint, position),
        *covered_checkpoint_bound,
    ))
}

fn terminal_response_from_scan_stop(
    stop: ResolvedScanStop<PackagePosition>,
    options: &QueryOptions,
    direction: ScanDirection,
    entry_checkpoint: u64,
    covered_checkpoint_bound: &mut Option<u64>,
) -> Result<ListPackagesResponse, RpcError> {
    let (position, checkpoint) = stop.into_scan_limit()?;
    let terminal = ScanTerminal::ScanLimit {
        watermark: package_frontier_watermark(
            options,
            direction,
            entry_checkpoint,
            covered_checkpoint_bound,
            position,
            checkpoint,
        )?,
    };
    let reason = terminal.reason();
    Ok(end_response(
        terminal.into_watermark(options, *covered_checkpoint_bound),
        reason,
    ))
}

/// Resolve the explicit package-coordinate scan window from the logical
/// checkpoint bounds.
async fn resolve_package_range(
    client: &BigTableClient,
    checkpoint_range: CheckpointRange,
    options: &QueryOptions,
) -> Result<ResolvedPackageRange, RpcError> {
    let cp_range = checkpoint_range.resolve(options);
    if cp_range.is_empty() {
        let tx_boundary =
            checkpoint_to_tx_boundary(client, cp_range.terminal_checkpoint(options.ordering))
                .await?;
        return Ok(ResolvedPackageRange::empty_at(
            cp_range.terminal_checkpoint(options.ordering),
            PackagePosition::start_of_tx(tx_boundary),
            cp_range.exhaustion,
        ));
    }

    let tx_range = client
        .checkpoint_to_tx_range(cp_range.range.clone())
        .await?;
    Ok(options.apply_package_cursor_bounds(ResolvedPackageRange {
        bounds: PackageScanBounds::tx_span(tx_range.start, tx_range.end),
        entry_checkpoint: if options.is_ascending() {
            cp_range.range.start
        } else {
            cp_range.range.end.saturating_sub(1)
        },
        end_checkpoint: cp_range.terminal_checkpoint(options.ordering),
        end_position: if options.is_ascending() {
            PackagePosition::start_of_tx(tx_range.end)
        } else {
            PackagePosition::start_of_tx(tx_range.start)
        },
        exhaustion: cp_range.exhaustion,
    }))
}

async fn checkpoint_to_tx_boundary(
    client: &BigTableClient,
    checkpoint: u64,
) -> Result<u64, RpcError> {
    if checkpoint == 0 {
        return Ok(0);
    }
    Ok(client.checkpoint_to_tx_range(0..checkpoint).await?.end)
}
