// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::RangeInclusive;

use crate::Result;
use crate::RpcError;
use crate::RpcService;
use crate::grpc::v2::render::RenderContext;
use crate::ledger_history::query_options::below_serving_floor;
use crate::metrics::ListStreamMetrics;
use futures::StreamExt;
use futures::stream::BoxStream;
use sui_rpc::proto::sui::rpc::v2::ListObjectVersionsRequest;
use sui_rpc::proto::sui::rpc::v2::Object;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::digests::ObjectDigest;
use sui_types::object::Object as StoredObject;
use sui_types::storage::ObjectOrTombstone;
use tokio_util::sync::CancellationToken;

use super::chunked_scan::ChunkArgs;
use super::chunked_scan::ChunkedScan;
use super::chunked_scan::ScanChunkDone;
use super::chunked_scan::cancelled;
use super::chunked_scan::spawn_list_chunk;
use super::ledger_read::storage_error;
use crate::ledger_history::object_versions::ObjectRemovalKind;
use crate::ledger_history::object_versions::ObjectVersionFrames;
use crate::ledger_history::object_versions::ObjectVersionItem;
use crate::ledger_history::object_versions::ObjectVersionsQuery;
use crate::reader::StateReader;
use removals::removal;
use retention::OldestRetainedVersion;
use retention::Retention;
use retention::read_floor;

mod history;
mod removals;
mod retention;

const METHOD: &str = "list_object_versions";
/// Listings are not told apart by read mask: only the chunk timers are recorded, and they are
/// labeled by method alone.
const RESOLUTION: &str = "all";

/// Serve `ListObjectVersions` from this node's object store, within its retention window. Every
/// listing is bounded by a checkpoint the rpc indexes have committed: `at_checkpoint`, or the
/// highest indexed checkpoint when it is unset. The object's version at the end of that
/// checkpoint, the request's version range, and its cursors fold into one inclusive version
/// range (see [`ObjectVersionsQuery::plan`]), whose items, live versions and the tombstones at
/// which the object was deleted or wrapped, are read in chunks of blocking reads
/// ([`ChunkedScan`]), so a slow client holds neither an iterator nor a blocking thread.
///
/// Pruning removes the versions an object replaced, and the tombstones it left, at or before the
/// end of the pruned floor checkpoint ([`retention::read_floor`]), keeping its live version at
/// the end of it. Its oldest retained version is that version, or for an object with none, its
/// first item after the floor, live or a removal (see
/// [`retention::first_retained_version_after_removal`]). A listing never starts or ends inside the
/// pruned versions without saying so: when the range reaches below the oldest retained version, an
/// ascending listing fails before it lists anything, and a descending one lists the retained
/// items and then fails, both with
/// [`crate::ledger_history::query_options::below_oldest_retained_version`]. An object created
/// after the floor checkpoint keeps its whole history.
///
/// A listing whose removal mask selects `transaction` finds each removal's transaction, and so
/// has the higher floor of the object floor and the last checkpoint whose transaction data is
/// pruned; above it, a transaction it cannot find is `INTERNAL`. Without `transaction`, removals
/// are listed without looking for their transactions.
///
/// Nodes that keep no replaced versions, or run without rpc indexes, answer `UNIMPLEMENTED`
/// (`NOT_SERVED_HERE`); nodes whose indexes have not committed a checkpoint yet, or are still
/// backfilling history after a restore, answer `UNAVAILABLE` (`NOT_READY`) (see
/// [`crate::reader::StateReader::at_checkpoint_upper_bound`]).
pub(super) async fn list_object_versions(
    service: RpcService,
    request: ListObjectVersionsRequest,
) -> Result<ObjectVersionFrames> {
    let endpoint = service.config.ledger_history().list_object_versions();
    let query = ObjectVersionsQuery::new(
        &request,
        endpoint.default_limit_items,
        endpoint.max_limit_items,
    )?;
    let chunk_metrics = service
        .list_metrics
        .as_ref()
        .map(|metrics| metrics.stream_metrics(METHOD, RESOLUTION));
    let listing = spawn_list_chunk(chunk_metrics.clone(), {
        let service = service.clone();
        let query = query.clone();
        move |_| Listing::plan(&service.reader, &query)
    })
    .await
    .map_err(storage_error)??;
    // Tests pause here to prune between planning a listing and reading its first chunk.
    sui_macros::fail_point_async!("list-object-versions-after-plan");

    let end = listing.end;
    let listed = list_items(
        service,
        query.clone(),
        listing,
        endpoint.chunk_max,
        chunk_metrics,
    );
    Ok(query.frames(listed, Ok(end)))
}

/// What a fullnode listing reads and how it ends, resolved before it lists anything.
#[derive(Debug)]
struct Listing {
    /// Where the first chunk starts reading: the retained versions to read, or `None` if none
    /// are left to list.
    initial_state: Option<ObjectVersionScanState>,
    /// How the listing ends once it has read every item it lists below its limit, unless
    /// `below_oldest_retained` is set.
    end: QueryEndReason,
    /// Set when a descending listing reaches below the object's oldest retained version: once
    /// it has read every retained item below its limit, it fails with this version's error, as of
    /// the last time the floor was read.
    below_oldest_retained: Option<OldestRetainedVersion>,
}

impl Listing {
    fn plan(reader: &StateReader, query: &ObjectVersionsQuery) -> Result<Self> {
        reader.require_object_version_history()?;
        let upper_bound = reader.at_checkpoint_upper_bound()?;
        let checkpoint = query.bounding_checkpoint(&upper_bound)?;
        let newest_at_tip = reader.get_object_version_at_checkpoint(
            query.object_id,
            upper_bound.highest_indexed(),
            &upper_bound,
        )?;
        let newest_at_tip = stored_version_of_indexed(reader, query.object_id, newest_at_tip)?;
        // An object with no retained rows either never existed or was deleted or wrapped at or
        // before the object floor, and the two cannot be told apart. Refuse it rather than list
        // it as empty, before any cursor is checked against the tip. On a node that has pruned no
        // objects it never existed, whatever the transaction floor. The floor is read after the
        // version: pruning raises it no later than it retracts rows.
        if newest_at_tip.is_none()
            && reader.at_checkpoint_lower_bound()?.pruned_floor().is_some()
            && let Some(floor_checkpoint) = read_floor(reader, query.lists_removal_transactions())?
        {
            return Err(below_serving_floor(floor_checkpoint));
        }
        let newest_visible = match query.at_checkpoint {
            Some(checkpoint) => {
                let indexed = reader.get_object_version_at_checkpoint(
                    query.object_id,
                    checkpoint,
                    &upper_bound,
                )?;
                stored_version_of_indexed(reader, query.object_id, indexed)?
            }
            None => newest_at_tip,
        };
        let plan = query.plan(&upper_bound, newest_visible)?;
        // After the read: pruning may have raised the floor past the bounding checkpoint while it
        // ran (see `StateReader::at_checkpoint_lower_bound`).
        reader.at_checkpoint_lower_bound()?.check(checkpoint)?;

        let Some(versions) = plan.versions else {
            return Ok(Self {
                initial_state: None,
                end: plan.end,
                below_oldest_retained: None,
            });
        };
        let tip = IndexedTip {
            newest_version: newest_at_tip.unwrap_or(*versions.end()),
            transactions_end: reader.transactions_end(upper_bound.highest_indexed())?,
        };
        let retention = Retention::read(
            reader,
            query.object_id,
            &tip,
            query.lists_removal_transactions(),
        )?;
        let Some(oldest_retained) = retention
            .oldest_retained()?
            .filter(|oldest_retained| *versions.start() < oldest_retained.version)
        else {
            return Ok(Self {
                initial_state: Some(ObjectVersionScanState {
                    versions,
                    retention,
                    tip,
                }),
                end: plan.end,
                below_oldest_retained: None,
            });
        };
        if !query.descending {
            return Err(oldest_retained.below_error());
        }
        Ok(Self {
            initial_state: (oldest_retained.version <= *versions.end()).then(|| {
                ObjectVersionScanState {
                    versions: oldest_retained.version..=*versions.end(),
                    retention,
                    tip,
                }
            }),
            end: plan.end,
            below_oldest_retained: Some(oldest_retained),
        })
    }
}

/// The version in the object store of the item the version index records at `indexed` (see
/// [`stored_version_at`]).
fn stored_version_of_indexed(
    reader: &StateReader,
    object_id: ObjectID,
    indexed: Option<SequenceNumber>,
) -> Result<Option<SequenceNumber>> {
    let Some(indexed) = indexed else {
        return Ok(None);
    };
    let first_stored = reader
        .object_versions_iter(object_id, indexed..=SequenceNumber::MAX, false)?
        .next()
        .transpose()?;
    Ok(Some(stored_version_at(indexed, first_stored.as_ref())))
}

/// The version in the object store of the item the version index records at `indexed`, given
/// the object's first stored item at or above `indexed`. They differ for a deletion or wrap by a
/// transaction with V1 effects: the index records `TransactionEffectsAPI::lamport_version`,
/// which V1 effects recompute from the modified objects alone, while the tombstone sits at the
/// version the transaction executed at, which also counts its read-only inputs. No item lies
/// between the two, so when the store holds nothing at `indexed`, a tombstone right above it is
/// that removal.
fn stored_version_at(
    indexed: SequenceNumber,
    first_stored: Option<&ObjectOrTombstone>,
) -> SequenceNumber {
    match first_stored {
        Some(ObjectOrTombstone::Tombstone((_, version, _))) if *version > indexed => *version,
        _ => indexed,
    }
}

/// The object's newest version, and the end of the transaction sequence, at the highest
/// checkpoint the indexes had committed when the listing was planned. Every read that finds the
/// transaction behind a removal is bounded by it, so it reads only what the indexes have
/// committed.
#[derive(Clone, Copy, Debug)]
struct IndexedTip {
    newest_version: SequenceNumber,
    /// One past the sequence number of the last transaction the rpc indexes have committed.
    transactions_end: u64,
}

/// How the object was removed at the tombstone whose digest is `digest`.
fn removal_kind(digest: &ObjectDigest) -> Result<ObjectRemovalKind> {
    if digest.is_deleted() {
        Ok(ObjectRemovalKind::Deleted)
    } else if digest.is_wrapped() {
        Ok(ObjectRemovalKind::Wrapped)
    } else {
        Err(storage_error(format!(
            "tombstone with unexpected digest {digest}"
        )))
    }
}

fn stored_version(item: &ObjectOrTombstone) -> SequenceNumber {
    match item {
        ObjectOrTombstone::Object(object) => object.version(),
        ObjectOrTombstone::Tombstone((_, version, _)) => *version,
    }
}

fn next_version(version: SequenceNumber) -> SequenceNumber {
    SequenceNumber::from_u64(version.value().saturating_add(1))
}

/// One chunk worker's input: the versions left to read, and what bounds them.
#[derive(Debug)]
struct ObjectVersionScanState {
    versions: RangeInclusive<SequenceNumber>,
    retention: Retention,
    tip: IndexedTip,
}

/// The items of `listing`, rendered, in listing order, up to the query's limit, then its
/// [`Listing::below_oldest_retained`] error, if set.
fn list_items(
    service: RpcService,
    query: ObjectVersionsQuery,
    listing: Listing,
    chunk_max: usize,
    chunk_metrics: Option<ListStreamMetrics>,
) -> BoxStream<'static, Result<ObjectVersionItem>> {
    let limit = query.limit;
    async_stream::try_stream! {
        let mut below_oldest_retained = listing.below_oldest_retained;
        if let Some(initial_state) = listing.initial_state {
            let mut scan = ChunkedScan::new(initial_state, limit, chunk_max, 0, move |state, args| {
                let service = service.clone();
                let query = query.clone();
                spawn_list_chunk(chunk_metrics.clone(), move |_| {
                    next_object_version_chunk(&service, &query, state, args)
                })
            });
            while let Some(item) = scan.next_item().await? {
                yield item;
            }
            // Pruning may have raised the floor since the listing was planned: name it as the
            // last chunk read it.
            if let Some(retention) = scan.into_terminal()
                && let Some(planned) = below_oldest_retained
            {
                below_oldest_retained = Some(retention.oldest_retained()?.unwrap_or(planned));
            }
        }
        if let Some(oldest_retained) = below_oldest_retained {
            Err::<(), _>(oldest_retained.below_error())?;
        }
    }
    .boxed()
}

type ObjectVersionChunkDone = ScanChunkDone<ObjectVersionScanState, ObjectVersionItem, Retention>;

/// Read and render up to `args.chunk_item_limit` items from `state.versions`, then check the
/// retention floor against the versions read.
fn next_object_version_chunk(
    service: &RpcService,
    query: &ObjectVersionsQuery,
    state: ObjectVersionScanState,
    args: ChunkArgs,
) -> Result<ObjectVersionChunkDone> {
    let render_context = RenderContext::empty(service.reader.inner());
    let chunk = read_items(
        &service.reader,
        query,
        state,
        args.chunk_item_limit,
        &args.cancel,
        |object| service.render_object_to_proto(object, &query.read_mask, &render_context),
    )?;
    Ok(ScanChunkDone {
        produced: chunk.items.len(),
        items: chunk.items,
        next_state: chunk.next_state,
        terminal: chunk.retention,
        remaining_scan_budget: args.scan_budget,
    })
}

/// The items one chunk read, and where the next chunk starts.
struct ChunkRead {
    items: Vec<ObjectVersionItem>,
    /// `None` once every version is read.
    next_state: Option<ObjectVersionScanState>,
    /// The retention floor, as the chunk last read it.
    retention: Retention,
}

/// Read up to `limit` items from `state.versions` in listing order, rendering live versions with
/// `render` and, when the query lists them, finding the transaction behind each removal, then
/// check the retention floor against the versions read. Fails with `CANCELLED` once `cancel`
/// fires, and with `INTERNAL` for a removal above the floor whose transaction cannot be found.
fn read_items(
    reader: &StateReader,
    query: &ObjectVersionsQuery,
    state: ObjectVersionScanState,
    limit: usize,
    cancel: &CancellationToken,
    render: impl Fn(&StoredObject) -> Object,
) -> Result<ChunkRead> {
    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    let ObjectVersionScanState {
        versions,
        retention,
        tip,
    } = state;
    let stored = reader
        .object_versions_iter(query.object_id, versions.clone(), query.descending)?
        .take(limit)
        .collect::<Result<Vec<_>>>()?;

    // A full chunk may leave versions past its last one; a short one read the whole range.
    let last_read = stored
        .last()
        .filter(|_| stored.len() == limit)
        .map(stored_version);
    let lowest_read = match last_read {
        Some(last) if query.descending => last,
        _ => *versions.start(),
    };
    let retention = retention.recheck(reader, query.object_id, lowest_read, &tip)?;
    let remaining = last_read.and_then(|last| {
        if query.descending {
            let below = last.value().checked_sub(1)?;
            (*versions.start() <= SequenceNumber::from_u64(below))
                .then(|| *versions.start()..=SequenceNumber::from_u64(below))
        } else {
            let above = SequenceNumber::from_u64(last.value().checked_add(1)?);
            (above <= *versions.end()).then(|| above..=*versions.end())
        }
    });

    let items = stored
        .into_iter()
        .map(|item| {
            if cancel.is_cancelled() {
                return Err(cancelled());
            }
            match item {
                ObjectOrTombstone::Object(object) => Ok(ObjectVersionItem::Live {
                    object: render(&object),
                    version: object.version(),
                }),
                ObjectOrTombstone::Tombstone((_, version, digest)) => {
                    let kind = removal_kind(&digest)?;
                    let transaction = if query.lists_removal_transactions() {
                        let Some(removal) = removal(reader, query.object_id, version, &tip)?
                            .filter(|removal| removal.confirmed)
                        else {
                            // Object pruning since the chunk's floor read may have taken the
                            // transaction: the recheck fails the listing with the floor error.
                            // Checkpoint pruning deletes effects before it raises the floor
                            // `read_floor` reads, so a prune racing this read can still end
                            // the listing here, with `INTERNAL`.
                            retention.recheck(reader, query.object_id, lowest_read, &tip)?;
                            return Err(RpcError::new(
                                tonic::Code::Internal,
                                format!(
                                    "missing the transaction that removed object {} at version \
                                     {}, above the retention floor",
                                    query.object_id,
                                    version.value()
                                ),
                            ));
                        };
                        Some(removal.transaction.digest)
                    } else {
                        None
                    };
                    Ok(ObjectVersionItem::Removal {
                        version,
                        kind,
                        transaction,
                    })
                }
            }
        })
        .collect::<Result<_>>()?;
    Ok(ChunkRead {
        items,
        next_state: remaining.map(|versions| ObjectVersionScanState {
            versions,
            retention,
            tip,
        }),
        retention,
    })
}

#[cfg(test)]
mod tests {
    use sui_types::object::Owner;

    use super::*;

    /// A V1 removal indexed below its tombstone resolves to the tombstone. Anything else at or
    /// above the indexed version, or nothing, leaves the indexed version as it is.
    #[test]
    fn removal_indexed_below_its_tombstone_resolves_to_it() {
        let id = ObjectID::from_single_byte(7);
        let version = SequenceNumber::from_u64;
        let tombstone = |at| {
            ObjectOrTombstone::Tombstone((id, version(at), ObjectDigest::OBJECT_DIGEST_DELETED))
        };
        let live = ObjectOrTombstone::Object(StoredObject::with_id_owner_version_for_testing(
            id,
            version(6),
            Owner::Immutable,
        ));

        assert_eq!(
            stored_version_at(version(5), Some(&tombstone(6))),
            version(6)
        );
        assert_eq!(
            stored_version_at(version(5), Some(&tombstone(5))),
            version(5)
        );
        assert_eq!(stored_version_at(version(5), Some(&live)), version(5));
        assert_eq!(stored_version_at(version(5), None), version(5));
    }
}
