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
use sui_rpc_api::ledger_history::query_options::IntraTxCoordinate;
use sui_rpc_api::ledger_history::query_options::QueryOptions;
use sui_rpc_api::ledger_history::query_options::ResolvedCheckpointRange;
use sui_rpc_api::ledger_history::query_options::ResolvedScan;
use sui_rpc_api::ledger_history::query_options::ScanBounds;
use sui_rpc_api::ledger_history::query_options::validate_checkpoint_bounds;
use sui_rpc_api::ledger_history::response::end_response;
use sui_rpc_api::ledger_history::response::item_response;
use sui_rpc_api::ledger_history::response::range_end_response;
use sui_rpc_api::ledger_history::response::watermark_response;
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

    validate_checkpoint_bounds(request.start_checkpoint, request.end_checkpoint)?;
    let options = QueryOptions::packages_from_proto(
        request.options.as_ref(),
        endpoint.default_limit_items,
        endpoint.max_limit_items,
    )?;
    let checkpoint_range = ResolvedCheckpointRange::from_request(
        request.start_checkpoint,
        request.end_checkpoint,
        checkpoint_hi_exclusive,
        &options,
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
        Result<Watermarked<PackageWriteItem, IntraTxCoordinate>, ScanStop>,
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
                    yield Watermarked::Watermark(IntraTxCoordinate::start_of_tx(tx_seq));
                }
            }
        }
    }
    .boxed();

    let tx_resolver = client.tx_wm_resolver(direction);
    let mut resolved_stream = render_ahead(
        resolve_scan_watermarks(
            write_stream,
            move |position: IntraTxCoordinate| tx_resolver(position.tx_seq),
            IntraTxCoordinate::start_of_tx,
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
                    let mut response = item_response::<ListPackagesResponse>(
                        package_version_proto(&write.object_ref),
                        watermark,
                    );
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
    position: IntraTxCoordinate,
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
    bounds: ScanBounds<IntraTxCoordinate>,
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
            let position = IntraTxCoordinate {
                tx_seq,
                index: write_index,
            };
            bounds.contains(position).then_some(PackageWriteItem {
                position,
                object_ref,
                checkpoint: tx.checkpoint_number,
            })
        })
        .collect())
}

fn package_position_cursor(checkpoint: u64, position: IntraTxCoordinate) -> Position {
    Position::Packages {
        checkpoint,
        tx_seq: position.tx_seq,
        write_index: position.index,
    }
}

fn package_version_proto((object_id, version, _): &ObjectRef) -> PackageVersion {
    let mut package = PackageVersion::default();
    package.package_id = Some(object_id.to_canonical_string(true));
    package.version = Some(version.value());
    package
}

fn package_frontier_watermark(
    options: &QueryOptions,
    direction: ScanDirection,
    entry_checkpoint: u64,
    covered_checkpoint_bound: &mut Option<u64>,
    position: IntraTxCoordinate,
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
                        position.tx_seq, position.index
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
    stop: ResolvedScanStop<IntraTxCoordinate>,
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
/// checkpoint bounds. Package writes are intra-tx items, so this is the
/// `resolve_event_range` shape: project the checkpoint window onto a
/// transaction range and let the shared intra-tx machinery apply cursor
/// bounds. Empty checkpoint windows ride the same path.
async fn resolve_package_range(
    client: &BigTableClient,
    cp_range: ResolvedCheckpointRange,
    options: &QueryOptions,
) -> Result<ResolvedScan<IntraTxCoordinate>, RpcError> {
    let tx_range = client
        .checkpoint_to_tx_range(cp_range.range.clone())
        .await?;
    Ok(ResolvedScan::<IntraTxCoordinate>::resolve(
        cp_range,
        IntraTxCoordinate::tx_window(tx_range),
        options,
    ))
}
