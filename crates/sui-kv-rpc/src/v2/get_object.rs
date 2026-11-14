// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::StreamExt;
use futures::TryStreamExt;
use futures::future;
use futures::stream;
use futures::stream::BoxStream;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use sui_kvstore::ObjectStatus;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsResponse;
use sui_rpc::proto::sui::rpc::v2::Object as ProtoObject;
use sui_rpc::proto::sui::rpc::v2::{GetObjectRequest, GetObjectResponse, GetObjectResult};
use sui_rpc_api::grpc::v2::ledger_service::{ObjectSelector, validate_get_object_requests};
use sui_rpc_api::{NotServedHereCause, ObjectNotFoundError, RpcError, not_served_here};
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::object::Object;
use sui_types::storage::ObjectKey;

use crate::KvRpcServer;
use crate::PackageResolver;
use crate::bigtable_client::BigTableClient;
use crate::config::{DEFAULT_RENDER_AHEAD, PipelineStage};
use crate::pipeline::{
    KeyedBatchOutput, MarkedKeyedUpstream, MissingRow, RenderAheadError, ResolvedWatermarked,
    SeekUnit, Watermarked, pipelined_chunks, pipelined_keyed_batches, render_ahead,
};
use crate::render::needs_only_object_id_and_version;
use crate::render::object_to_response;
use crate::render::object_version_to_response;
use crate::request_cache::CacheFetcher;
use crate::served_height::ServedHeight;

pub const MAX_BATCH_REQUESTS: usize = 1000;

pub(crate) async fn get_object(
    server: &KvRpcServer,
    request: tonic::Request<GetObjectRequest>,
) -> Result<GetObjectResponse, RpcError> {
    let served_height = ServedHeight::of(&request);
    let GetObjectRequest {
        object_id,
        version,
        read_mask,
        at_checkpoint,
        ..
    } = request.into_inner();
    let (requests, read_mask) =
        validate_get_object_requests(vec![(object_id, version, at_checkpoint)], read_mask)?;
    let (address, selector) = requests[0];
    let object_id: ObjectID = address.into();
    let version_only = needs_only_object_id_and_version(&read_mask);
    let client = server.limited_client("GetObject");
    let objects = ObjectReads {
        client: client.clone(),
        version_only,
    };
    let found = match selector {
        ObjectSelector::Latest => objects
            .latest(object_id)
            .await?
            .ok_or_else(|| ObjectNotFoundError::new(address))?,
        ObjectSelector::Version(version) => objects
            .exact(ObjectKey(object_id, version.into()))
            .await?
            .ok_or_else(|| ObjectNotFoundError::new_with_version(address, version))?,
        ObjectSelector::AtCheckpoint(checkpoint) => {
            let at_checkpoint = server.at_checkpoint_reads(client, served_height)?;
            if checkpoint > at_checkpoint.served_height.checkpoint {
                return Err(not_yet_indexed(checkpoint, at_checkpoint.served_height));
            }
            let not_found = || ObjectNotFoundError::new_at_checkpoint(address, checkpoint);
            let version = at_checkpoint
                .live_version_at_checkpoint(object_id, checkpoint)
                .await?
                .ok_or_else(not_found)?;
            // A live version's row is in the objects table, so the index answers a
            // version-only read without touching it.
            if version_only {
                Found::Version(version)
            } else {
                let key = ObjectKey(object_id, version);
                objects
                    .exact(key)
                    .await?
                    .ok_or_else(|| missing_indexed_row(key))?
            }
        }
    };
    let message = found
        .render(object_id, &read_mask, &server.package_resolver)
        .await;
    Ok(GetObjectResponse::new(message))
}

/// Serve a batch as its distinct lookups, read and rendered as they become ready:
///
/// 1. Each lookup first learns which `objects` row answers it, if any. An exact version names its
///    row. An `at_checkpoint` lookup scans `object_versions_by_checkpoint` for the version live at
///    that checkpoint, because BigTable cannot return the newest row of several ranges in one
///    `ReadRows` (`rows_limit` covers the whole request). A latest lookup reads its object with
///    one reverse scan of `objects`. The scans run in groups of the `objects` stage's
///    `max-seeks-per-read`, each group one read of a [`pipelined_chunks`] stage, and every group
///    starts at once. A group's answers move on together, so the rows they name fill at most one
///    `objects` read.
/// 2. The rows the lookups name are read as the lookups arrive, by [`pipelined_keyed_batches`]
///    under the `objects` stage ([`crate::config::StagesConfig::objects`]): reads of at most
///    `max-seeks-per-read` keys, at most `concurrency` of them at a time, and each row at most
///    once per request. A lookup whose row is missing is `NOT_FOUND`, unless the version index
///    named the row: then the index and `objects` disagree, and the lookup is `INTERNAL`.
/// 3. Each lookup is rendered once, at most `DEFAULT_RENDER_AHEAD` at a time (the list endpoints'
///    default render-ahead), and every request for it gets the result.
///
/// Every BigTable read of the batch, scans and `objects` reads alike, shares one request-scoped
/// limiter. Rendering is not limited by it: package loads go through the server-wide
/// [`PackageResolver`].
///
/// When the read mask asks for nothing but `object_id` and `version`, an `at_checkpoint` lookup is
/// answered by its scan alone, and the `objects` reads fetch only row keys, not object contents.
pub(crate) async fn batch_get_objects(
    server: &KvRpcServer,
    request: tonic::Request<BatchGetObjectsRequest>,
) -> Result<BatchGetObjectsResponse, RpcError> {
    let served_height = ServedHeight::of(&request);
    let BatchGetObjectsRequest {
        requests,
        read_mask,
        ..
    } = request.into_inner();
    if requests.len() > MAX_BATCH_REQUESTS {
        return Err(RpcError::new(
            tonic::Code::InvalidArgument,
            format!("number of batch requests exceed limit of {MAX_BATCH_REQUESTS}"),
        ));
    }
    let requests = requests
        .into_iter()
        .map(|req| (req.object_id, req.version, req.at_checkpoint))
        .collect();
    let (requests, read_mask) = validate_get_object_requests(requests, read_mask)?;
    let version_only = needs_only_object_id_and_version(&read_mask);

    // Requests for the same object state share one lookup: `request_lookups[i]` is the index in
    // `lookups` of request `i`'s lookup.
    let mut lookups = Vec::new();
    let mut lookup_indexes = HashMap::new();
    let request_lookups: Vec<usize> = requests
        .into_iter()
        .map(|lookup| {
            *lookup_indexes.entry(lookup).or_insert_with(|| {
                lookups.push(lookup);
                lookups.len() - 1
            })
        })
        .collect();
    let lookup_count = lookups.len();

    // Every read of the batch goes through this client, under one request-wide limiter.
    let limited_client = server.limited_client("BatchGetObjects");
    let objects = ObjectReads {
        client: limited_client.clone(),
        version_only,
    };

    let mut answered_without_scan = Vec::new();
    let mut scans = Vec::new();
    let mut at_checkpoint_lookups = Vec::new();
    for (index, &(address, selector)) in lookups.iter().enumerate() {
        let object_id: ObjectID = address.into();
        match selector {
            ObjectSelector::Latest => scans.push((index, Scan::Latest(object_id))),
            ObjectSelector::Version(version) => answered_without_scan.push((
                index,
                ObjectsStageInput::RequestedRow(ObjectKey(object_id, version.into())),
            )),
            ObjectSelector::AtCheckpoint(checkpoint) => {
                at_checkpoint_lookups.push((index, object_id, checkpoint))
            }
        }
    }
    if !at_checkpoint_lookups.is_empty() {
        let reads = Arc::new(server.at_checkpoint_reads(limited_client.clone(), served_height)?);
        for (index, object_id, checkpoint) in at_checkpoint_lookups {
            if checkpoint > reads.served_height.checkpoint {
                let error = not_yet_indexed(checkpoint, reads.served_height);
                answered_without_scan.push((index, ObjectsStageInput::Failed(error)));
            } else {
                let reads = reads.clone();
                let scan = Scan::AtCheckpoint {
                    reads,
                    object_id,
                    checkpoint,
                };
                scans.push((index, scan));
            }
        }
    }

    // A group's drainer task runs its scans at once and to their ends, so each scan returns its
    // permit whether or not the `objects` reads below are taking lookups. Every group starts at
    // once: the request limiter alone bounds how many scans reach BigTable.
    let objects_stage = server.stages.stage(PipelineStage::Objects);
    let scans_per_group = objects_stage.max_seeks_per_read;
    let scan_groups = scans.len().div_ceil(scans_per_group).max(1);
    let scans: BoxStream<'static, Result<Watermarked<(usize, Scan)>, RpcError>> =
        stream::iter(scans.into_iter().map(|scan| Ok(Watermarked::Item(scan)))).boxed();
    let scanned = pipelined_chunks(scans, scans_per_group, scan_groups, None, SeekUnit::Item, {
        let objects = objects.clone();
        move |group: Vec<(usize, Scan)>| {
            let objects = objects.clone();
            async move {
                let answers = future::try_join_all(group.into_iter().map(|(index, scan)| {
                    let objects = &objects;
                    async move { Ok::<_, RpcError>((index, scan.run(objects).await?)) }
                }))
                .await?;
                Ok::<_, RpcError>(stream::iter(answers.into_iter().map(Ok)).boxed())
            }
        }
    });
    // Exact versions go out in key order, so an `objects` read covers one run of adjacent keys.
    answered_without_scan.sort_by_key(|(_, answer)| answer.row_key());
    let answered_without_scan = stream::iter(
        answered_without_scan
            .into_iter()
            .map(|answer| Ok(Watermarked::Item(answer))),
    )
    .boxed();
    let lookups_with_keys: MarkedKeyedUpstream<(usize, ObjectsStageInput), ObjectKey, RpcError> =
        stream::select(answered_without_scan, scanned)
            .map_ok(|frame| {
                frame.map_item(|(index, answer)| {
                    let keys = answer.row_keys();
                    ((index, answer), keys)
                })
            })
            .boxed();

    let fetcher: Arc<dyn CacheFetcher<ObjectKey, Found>> = Arc::new(objects);
    let answered: BoxStream<'static, Result<ResolvedWatermarked<LookupWithRows>, RpcError>> =
        pipelined_keyed_batches(
            lookups_with_keys,
            objects_stage.max_seeks_per_read,
            objects_stage.concurrency,
            objects_stage.min_seeks_per_split_read,
            fetcher,
            MissingRow::Omit,
        )
        // The lookups carry no watermarks.
        .try_filter_map(|frame| {
            future::ready(Ok(match frame {
                Watermarked::Item(item) => Some(ResolvedWatermarked::Item(item)),
                Watermarked::Watermark(_) => None,
            }))
        })
        .boxed();

    let mut rendered = render_ahead(answered, DEFAULT_RENDER_AHEAD, {
        let read_mask = Arc::new(read_mask);
        let resolver = server.package_resolver.clone();
        move |((index, answer), rows)| {
            let (address, selector) = lookups[index];
            let read_mask = read_mask.clone();
            let resolver = resolver.clone();
            async move {
                let result = match answer.complete(&rows) {
                    Ok(Some(found)) => GetObjectResult::new_object(
                        found.render(address.into(), &read_mask, &resolver).await,
                    ),
                    Ok(None) => {
                        let not_found = match selector {
                            ObjectSelector::Latest => ObjectNotFoundError::new(address),
                            ObjectSelector::Version(version) => {
                                ObjectNotFoundError::new_with_version(address, version)
                            }
                            ObjectSelector::AtCheckpoint(checkpoint) => {
                                ObjectNotFoundError::new_at_checkpoint(address, checkpoint)
                            }
                        };
                        GetObjectResult::new_error(RpcError::from(not_found).into_status_proto())
                    }
                    Err(err) => GetObjectResult::new_error(err.into_status_proto()),
                };
                Ok::<_, Infallible>((index, result))
            }
        }
    });

    let mut results: Vec<Option<GetObjectResult>> = vec![None; lookup_count];
    while let Some(frame) = rendered.next().await {
        match frame {
            Ok(ResolvedWatermarked::Item((index, result))) => results[index] = Some(result),
            Ok(ResolvedWatermarked::Watermark { .. }) => {}
            Err(RenderAheadError::Upstream(err)) => return Err(err),
            Err(RenderAheadError::Render(never)) => match never {},
        }
    }

    // A lookup's result goes to every request for it; the last of those requests takes it.
    let mut requests_left = vec![0usize; lookup_count];
    for &lookup in &request_lookups {
        requests_left[lookup] += 1;
    }
    let objects = request_lookups
        .into_iter()
        .map(|lookup| {
            requests_left[lookup] -= 1;
            let result = if requests_left[lookup] == 0 {
                results[lookup].take()
            } else {
                results[lookup].clone()
            };
            result.ok_or_else(|| {
                RpcError::new(
                    tonic::Code::Internal,
                    "BatchGetObjects: a lookup ended without a result",
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BatchGetObjectsResponse::new(objects))
}

/// An object as a read found it: the whole object, or, for a read mask that needs only
/// `object_id` and `version`, just its version.
#[derive(Clone)]
enum Found {
    Object(Arc<Object>),
    Version(SequenceNumber),
}

impl Found {
    async fn render(
        &self,
        object_id: ObjectID,
        read_mask: &FieldMaskTree,
        resolver: &PackageResolver,
    ) -> ProtoObject {
        match self {
            Found::Object(object) => object_to_response(object, read_mask, resolver).await,
            Found::Version(version) => object_version_to_response(object_id, *version),
        }
    }
}

/// A batch lookup after its `objects` reads: its index in the batch's lookups, its
/// [`ObjectsStageInput`], and the rows of the keys it listed.
type LookupWithRows = KeyedBatchOutput<(usize, ObjectsStageInput), ObjectKey, Found>;

/// A scan that a batch lookup starts with, before any `objects` read.
enum Scan {
    /// The newest version of the object, from one reverse scan of `objects`.
    Latest(ObjectID),
    /// The version live at the end of `checkpoint`, from `object_versions_by_checkpoint`.
    AtCheckpoint {
        reads: Arc<AtCheckpointReads>,
        object_id: ObjectID,
        checkpoint: u64,
    },
}

impl Scan {
    async fn run(self, objects: &ObjectReads) -> Result<ObjectsStageInput, RpcError> {
        Ok(match self {
            Scan::Latest(object_id) => ObjectsStageInput::Scanned(objects.latest(object_id).await?),
            Scan::AtCheckpoint {
                reads,
                object_id,
                checkpoint,
            } => match reads
                .live_version_at_checkpoint(object_id, checkpoint)
                .await?
            {
                None => ObjectsStageInput::Scanned(None),
                // A live version's row is in the objects table, so the index answers a
                // version-only read without touching it.
                Some(version) if objects.version_only => {
                    ObjectsStageInput::Scanned(Some(Found::Version(version)))
                }
                Some(version) => ObjectsStageInput::IndexedRow(ObjectKey(object_id, version)),
            },
        })
    }
}

/// What a batch lookup carries into the `objects` stage: its answer as far as it is known
/// before the `objects` reads.
enum ObjectsStageInput {
    /// The `objects` row at the version the request asked for answers the lookup; without the
    /// row it is `NOT_FOUND`.
    RequestedRow(ObjectKey),
    /// The `objects` row at this key, which the version index names as live, answers the
    /// lookup; without the row the index and `objects` disagree, and the lookup is `INTERNAL`.
    IndexedRow(ObjectKey),
    /// The lookup's scan answered it; `None` makes it `NOT_FOUND`.
    Scanned(Option<Found>),
    /// The lookup fails with this error, without a read.
    Failed(RpcError),
}

impl ObjectsStageInput {
    /// The `objects` row the lookup still needs, if any.
    fn row_key(&self) -> Option<ObjectKey> {
        match self {
            ObjectsStageInput::RequestedRow(key) | ObjectsStageInput::IndexedRow(key) => Some(*key),
            ObjectsStageInput::Scanned(_) | ObjectsStageInput::Failed(_) => None,
        }
    }

    /// The `objects` rows the lookup still needs.
    fn row_keys(&self) -> Vec<ObjectKey> {
        self.row_key().into_iter().collect()
    }

    /// The lookup's answer, given the rows the `objects` reads returned for its keys. `Ok(None)`
    /// means `NOT_FOUND`.
    fn complete(self, rows: &HashMap<ObjectKey, Found>) -> Result<Option<Found>, RpcError> {
        match self {
            ObjectsStageInput::RequestedRow(key) => Ok(rows.get(&key).cloned()),
            ObjectsStageInput::IndexedRow(key) => match rows.get(&key) {
                Some(found) => Ok(Some(found.clone())),
                None => Err(missing_indexed_row(key)),
            },
            ObjectsStageInput::Scanned(found) => Ok(found),
            ObjectsStageInput::Failed(err) => Err(err),
        }
    }
}

/// Reads of the `objects` table through a request's gated client: whole objects, or, with
/// `version_only`, just the versions of the live ones, read without the objects' contents.
#[derive(Clone)]
struct ObjectReads {
    client: BigTableClient,
    version_only: bool,
}

impl ObjectReads {
    /// The newest version of `object_id`, unless the object was deleted or wrapped at it.
    async fn latest(&self, object_id: ObjectID) -> Result<Option<Found>, RpcError> {
        Ok(if self.version_only {
            self.client
                .get_latest_object_version(object_id)
                .await?
                .map(Found::Version)
        } else {
            self.client
                .get_latest_object(object_id)
                .await?
                .map(|object| Found::Object(Arc::new(object)))
        })
    }

    /// The object at the version `key` names, unless the object was deleted or wrapped at it.
    async fn exact(&self, key: ObjectKey) -> Result<Option<Found>, RpcError> {
        // The read runs to its end, so it finishes as completed rather than cut off.
        let rows: Vec<_> = self.fetch(vec![key]).await?.try_collect().await?;
        Ok(rows.into_iter().next().map(|(_, found)| found))
    }
}

#[async_trait::async_trait]
impl CacheFetcher<ObjectKey, Found> for ObjectReads {
    async fn fetch(
        &self,
        keys: Vec<ObjectKey>,
    ) -> Result<BoxStream<'static, Result<(ObjectKey, Found), RpcError>>, RpcError> {
        Ok(if self.version_only {
            self.client
                .get_existing_object_keys_stream(keys)
                .await?
                .map_ok(|key| (key, Found::Version(key.1)))
                .map_err(RpcError::from)
                .boxed()
        } else {
            self.client
                .get_objects_stream(keys)
                .await?
                .map_ok(|object| {
                    (
                        ObjectKey(object.id(), object.version()),
                        Found::Object(Arc::new(object)),
                    )
                })
                .map_err(RpcError::from)
                .boxed()
        })
    }
}

/// What a request's `at_checkpoint` reads run with: the request-scoped gated client for their
/// `object_versions_by_checkpoint` scans, and the served height that bounds them.
struct AtCheckpointReads {
    client: BigTableClient,
    served_height: ServedHeight,
}

impl KvRpcServer {
    /// `Unimplemented` / `NOT_SERVED_HERE` (cause `disabled_by_config`) unless `at_checkpoint`
    /// object reads are enabled; otherwise the scans read through `client`, the request's gated
    /// client, at `served_height`, the height the request was admitted at (`Unavailable` if it
    /// had none).
    fn at_checkpoint_reads(
        &self,
        client: BigTableClient,
        served_height: Result<ServedHeight, RpcError>,
    ) -> Result<AtCheckpointReads, RpcError> {
        if !self.object_at_checkpoint_enabled {
            return Err(not_served_here(
                NotServedHereCause::DisabledByConfig,
                "at_checkpoint object reads are not enabled on this instance",
            ));
        }
        Ok(AtCheckpointReads {
            served_height: served_height?,
            client,
        })
    }
}

impl AtCheckpointReads {
    /// The version `object_id` was live at, at the end of `checkpoint`. `None` if it did not
    /// exist yet, or had been deleted or wrapped.
    async fn live_version_at_checkpoint(
        &self,
        object_id: ObjectID,
        checkpoint: u64,
    ) -> Result<Option<SequenceNumber>, RpcError> {
        Ok(
            match self
                .client
                .get_object_version_at_checkpoint(object_id, checkpoint)
                .await?
            {
                Some((version, ObjectStatus::Live)) => Some(version),
                Some((_, ObjectStatus::Deleted | ObjectStatus::Wrapped)) | None => None,
            },
        )
    }
}

/// The error for an `at_checkpoint` above the served height. The response does not say which
/// checkpoint it was read at, so answering from a partially indexed table could hand the caller
/// a wrong answer to persist; fail closed instead.
fn not_yet_indexed(checkpoint: u64, served_height: ServedHeight) -> RpcError {
    sui_rpc_api::checkpoint_not_yet_indexed_error(checkpoint, served_height.checkpoint)
}

/// The error for a version that `object_versions_by_checkpoint` names as live but that has no
/// live `objects` row: the index and storage diverged, so the object's absence is not an answer.
fn missing_indexed_row(key: ObjectKey) -> RpcError {
    RpcError::new(
        tonic::Code::Internal,
        format!("object_versions_by_checkpoint names {key:?} as live, but objects has no live row"),
    )
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::time::Duration;

    use sui_kvstore::WatermarkV1;
    use sui_kvstore::tables::object_versions_by_checkpoint;
    use sui_kvstore::tables::objects;
    use sui_kvstore::testing::MockBigtableServer;
    use sui_rpc::proto::sui::rpc::v2::get_object_result;
    use sui_types::base_types::ObjectID;
    use sui_types::base_types::SuiAddress;
    use sui_types::digests::TransactionDigest;
    use sui_types::object::Owner;

    use super::*;
    use crate::KvRpcConfig;
    use crate::LedgerHistoryConfig;
    use crate::StageConfig;
    use crate::StagesConfig;
    use crate::service_info_watermark_pipelines;

    /// A kv-rpc server over the mock BigTable at `addr`, with every watermark it serves at
    /// checkpoint 10 and its service info loaded.
    async fn start_kv_rpc(
        addr: SocketAddr,
        stages: StagesConfig,
        enable_object_at_checkpoint: bool,
    ) -> KvRpcServer {
        let request_bigtable_concurrency = KvRpcConfig::default().request_bigtable_concurrency();
        start_kv_rpc_with_request_concurrency(
            addr,
            stages,
            enable_object_at_checkpoint,
            request_bigtable_concurrency,
        )
        .await
    }

    /// [`start_kv_rpc`], with each request limited to `request_bigtable_concurrency` BigTable
    /// reads at once.
    async fn start_kv_rpc_with_request_concurrency(
        addr: SocketAddr,
        stages: StagesConfig,
        enable_object_at_checkpoint: bool,
        request_bigtable_concurrency: usize,
    ) -> KvRpcServer {
        let mut client = sui_kvstore::BigTableClient::new_local(addr.to_string(), "test".into())
            .await
            .expect("connect to mock BigTable");
        for pipeline in service_info_watermark_pipelines(
            LedgerHistoryConfig::default().bitmap_schema(),
            enable_object_at_checkpoint,
        ) {
            client
                .create_pipeline_watermark_if_absent(
                    pipeline,
                    &WatermarkV1 {
                        epoch_hi_inclusive: 0,
                        checkpoint_hi_inclusive: Some(10),
                        tx_hi: 0,
                        timestamp_ms_hi_inclusive: 10_000,
                        reader_lo: 0,
                        pruner_hi: 0,
                        pruner_timestamp_ms: 0,
                        bucket_start_cp: None,
                    },
                )
                .await
                .expect("seed watermark");
        }
        let config = KvRpcConfig::default();
        let kv_rpc = KvRpcServer::new_local_with_config(
            addr.to_string(),
            "test".to_string(),
            None,
            LedgerHistoryConfig::default(),
            request_bigtable_concurrency,
            config.monotonic_read_wait_timeout(),
            stages,
            config.watermark_source(),
            enable_object_at_checkpoint,
        )
        .await
        .expect("start kv-rpc server");
        tokio::time::timeout(
            Duration::from_secs(5),
            kv_rpc.cache.subscribe().wait_for(Option::is_some),
        )
        .await
        .expect("service info never refreshed")
        .expect("service info sender dropped");
        kv_rpc
    }

    fn mask(paths: &[&str]) -> prost_types::FieldMask {
        prost_types::FieldMask {
            paths: paths.iter().map(|path| path.to_string()).collect(),
        }
    }

    /// For each `objects`-table read since the last clear, whether it asked for values to be
    /// stripped.
    async fn objects_reads_stripped(mock: &MockBigtableServer) -> Vec<bool> {
        mock.read_rows_calls()
            .await
            .into_iter()
            .filter(|call| call.table == objects::NAME)
            .map(|call| call.values_stripped)
            .collect()
    }

    /// A read mask that asks for no field beyond `object_id` and `version`, including one that
    /// names only `version` and an empty one, never reads object contents: the latest and
    /// exact-version reads fetch only row keys, and `at_checkpoint` reads are answered by the
    /// version index without an `objects` read. Like a read of the object, the answer always
    /// carries both `object_id` and `version`, and nothing else.
    #[tokio::test]
    async fn test_version_only_masks_skip_object_contents() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), true).await;

        // One object, live at version 3 from checkpoint 5.
        let id = ObjectID::random();
        let version = SequenceNumber::from_u64(3);
        let object = Object::with_id_owner_version_for_testing(
            id,
            version,
            Owner::AddressOwner(SuiAddress::ZERO),
        );
        mock.insert_row(
            objects::NAME,
            objects::encode_key(&ObjectKey(id, version)),
            objects::encode(&object).unwrap(),
        )
        .await;
        mock.insert_row(
            object_versions_by_checkpoint::NAME,
            object_versions_by_checkpoint::encode_key(&id, 5),
            object_versions_by_checkpoint::encode(version, ObjectStatus::Live),
        )
        .await;

        // Latest, exact version, and at checkpoint 7.
        let selectors = [(None, None), (Some(3), None), (None, Some(7))];
        let request = |(version, at_checkpoint): (Option<u64>, Option<u64>), paths: &[&str]| {
            let mut req = GetObjectRequest::default();
            req.object_id = Some(id.to_canonical_string(true));
            req.version = version;
            req.at_checkpoint = at_checkpoint;
            req.read_mask = Some(mask(paths));
            req
        };

        let mut expected = ProtoObject::default();
        expected.object_id = Some(id.to_canonical_string(true));
        expected.version = Some(3);
        for paths in [&["object_id", "version"][..], &["version"], &[]] {
            mock.clear_read_rows_calls().await;
            for selector in selectors {
                let rendered =
                    get_object(&kv_rpc, kv_rpc.admitted_request(request(selector, paths)))
                        .await
                        .unwrap()
                        .object
                        .unwrap();
                assert_eq!(rendered, expected, "{selector:?} {paths:?}");
            }
            let mut batch = BatchGetObjectsRequest::default();
            batch.requests = selectors.map(|selector| request(selector, &[])).to_vec();
            batch.read_mask = Some(mask(paths));
            for item in batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
                .await
                .unwrap()
                .objects
            {
                assert_eq!(item.object(), &expected, "{item:?} {paths:?}");
            }
            // Latest and exact version, once each for the single reads and the batch; none for
            // `at_checkpoint`.
            assert_eq!(
                objects_reads_stripped(&mock).await,
                [true; 4],
                "version-only reads fetch only row keys: {paths:?}",
            );
        }

        mock_server.abort();
    }

    /// A read mask that needs no object contents answers a deleted object as `NOT_FOUND`, like a
    /// read of its contents does, both latest and at the version that deleted it.
    #[tokio::test]
    async fn test_version_only_reads_of_deleted_object_are_not_found() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), false).await;

        // Live at version 1, deleted at version 2.
        let id = ObjectID::random();
        let live = SequenceNumber::from_u64(1);
        let object = Object::with_id_owner_version_for_testing(
            id,
            live,
            Owner::AddressOwner(SuiAddress::ZERO),
        );
        mock.insert_row(
            objects::NAME,
            objects::encode_key(&ObjectKey(id, live)),
            objects::encode(&object).unwrap(),
        )
        .await;
        mock.insert_row(
            objects::NAME,
            objects::encode_key(&ObjectKey(id, SequenceNumber::from_u64(2))),
            objects::encode_tombstone(ObjectStatus::Deleted, &TransactionDigest::random()).unwrap(),
        )
        .await;

        let request = |version: Option<u64>| {
            let mut req = GetObjectRequest::default();
            req.object_id = Some(id.to_canonical_string(true));
            req.version = version;
            req
        };
        let version_only = ["object_id", "version"];
        for paths in [&version_only[..], &["object_id", "version", "digest"]] {
            for version in [None, Some(2)] {
                let mut req = request(version);
                req.read_mask = Some(mask(paths));
                let status: tonic::Status = get_object(&kv_rpc, kv_rpc.admitted_request(req))
                    .await
                    .unwrap_err()
                    .into();
                assert_eq!(
                    status.code(),
                    tonic::Code::NotFound,
                    "{version:?} {paths:?}"
                );
            }
            let mut req = request(Some(1));
            req.read_mask = Some(mask(paths));
            let found = get_object(&kv_rpc, kv_rpc.admitted_request(req))
                .await
                .unwrap()
                .object
                .unwrap();
            assert_eq!(found.version, Some(1), "{paths:?}");
        }

        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = vec![request(None), request(Some(2)), request(Some(1))];
        batch.read_mask = Some(mask(&version_only));
        let items = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
            .await
            .unwrap()
            .objects;
        assert_eq!(
            items[0].error().code,
            tonic::Code::NotFound as i32,
            "latest"
        );
        assert_eq!(items[1].error().code, tonic::Code::NotFound as i32, "at 2");
        assert_eq!(items[2].object().version, Some(1), "at 1");

        mock_server.abort();
    }

    /// A version that the version index names as live but that has no `objects` row means the
    /// index and storage diverged: an `at_checkpoint` read of it is `INTERNAL`, not `NOT_FOUND`,
    /// both alone and in a batch. A request that names the same version itself is `NOT_FOUND`.
    #[tokio::test]
    async fn test_at_checkpoint_version_missing_from_objects_is_internal() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), true).await;

        // The index has the object live at version 3 from checkpoint 5; `objects` has no row.
        let id = ObjectID::random();
        mock.insert_row(
            object_versions_by_checkpoint::NAME,
            object_versions_by_checkpoint::encode_key(&id, 5),
            object_versions_by_checkpoint::encode(SequenceNumber::from_u64(3), ObjectStatus::Live),
        )
        .await;

        let request = |version: Option<u64>, at_checkpoint: Option<u64>| {
            let mut req = GetObjectRequest::default();
            req.object_id = Some(id.to_canonical_string(true));
            req.version = version;
            req.at_checkpoint = at_checkpoint;
            req
        };
        let contents = ["object_id", "version", "digest"];

        let mut req = request(None, Some(7));
        req.read_mask = Some(mask(&contents));
        let status: tonic::Status = get_object(&kv_rpc, kv_rpc.admitted_request(req))
            .await
            .unwrap_err()
            .into();
        assert_eq!(status.code(), tonic::Code::Internal, "{status:?}");

        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = vec![request(None, Some(7)), request(Some(3), None)];
        batch.read_mask = Some(mask(&contents));
        let items = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
            .await
            .unwrap()
            .objects;
        assert_eq!(
            items[0].error().code,
            tonic::Code::Internal as i32,
            "at checkpoint 7"
        );
        assert_eq!(
            items[1].error().code,
            tonic::Code::NotFound as i32,
            "version 3"
        );

        mock_server.abort();
    }

    /// On an instance without `at_checkpoint` reads, an `at_checkpoint` request is refused as
    /// `Unimplemented` with `NOT_SERVED_HERE` / `disabled_by_config`, and so is a whole batch
    /// that contains one. The refusal depends only on the request's shape, so a checkpoint above
    /// the served height gets it too, not `OUT_OF_RANGE`.
    #[tokio::test]
    async fn test_at_checkpoint_refused_as_disabled_by_config() {
        use sui_rpc::proto::google::rpc::ErrorInfo;
        use sui_rpc::proto::sui::rpc::v2::ErrorReason;

        fn assert_disabled_by_config(error: RpcError) {
            let status = error.into_status_proto();
            assert_eq!(status.code, tonic::Code::Unimplemented as i32, "{status:?}");
            let error_infos: Vec<ErrorInfo> = status
                .details
                .iter()
                .filter_map(|detail| detail.to_msg::<ErrorInfo>().ok())
                .collect();
            let [error_info] = error_infos.as_slice() else {
                panic!("expected exactly one ErrorInfo in {status:?}");
            };
            assert_eq!(
                error_info.reason,
                ErrorReason::NotServedHere.as_str_name(),
                "{status:?}"
            );
            assert_eq!(error_info.domain, ErrorReason::DOMAIN, "{status:?}");
            assert_eq!(
                error_info.metadata,
                [("cause".to_owned(), "disabled_by_config".to_owned())].into(),
                "{status:?}"
            );
        }

        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), false).await;

        let request = |at_checkpoint: Option<u64>| {
            let mut req = GetObjectRequest::default();
            req.object_id = Some(ObjectID::random().to_canonical_string(true));
            req.at_checkpoint = at_checkpoint;
            req
        };

        // The served height is 10.
        for checkpoint in [5, 11] {
            assert_disabled_by_config(
                get_object(&kv_rpc, kv_rpc.admitted_request(request(Some(checkpoint))))
                    .await
                    .unwrap_err(),
            );
        }

        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = vec![request(None), request(Some(5)), request(Some(11))];
        assert_disabled_by_config(
            batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
                .await
                .unwrap_err(),
        );

        mock_server.abort();
    }

    /// On an instance without `at_checkpoint` reads, a batch of exact versions goes out as
    /// `objects` multi-gets of at most the stage's `max-seeks-per-read` keys, and every item is
    /// answered, whether its version is stored or not.
    #[tokio::test]
    async fn test_batch_exact_versions_split_into_objects_stage_reads() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let stages = StagesConfig {
            objects: Some(StageConfig {
                max_seeks_per_read: Some(2),
                ..Default::default()
            }),
            ..Default::default()
        };
        let kv_rpc = start_kv_rpc(addr, stages, false).await;

        // Versions 1 to 5 are requested; only the odd ones are stored.
        let id = ObjectID::random();
        let mut stored = HashMap::new();
        for version in [1, 3, 5] {
            let object = Object::with_id_owner_version_for_testing(
                id,
                SequenceNumber::from_u64(version),
                Owner::AddressOwner(SuiAddress::ZERO),
            );
            mock.insert_row(
                objects::NAME,
                objects::encode_key(&ObjectKey(id, object.version())),
                objects::encode(&object).unwrap(),
            )
            .await;
            stored.insert(version, object);
        }
        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = (1..=5)
            .map(|version| {
                let mut req = GetObjectRequest::default();
                req.object_id = Some(id.to_canonical_string(true));
                req.version = Some(version);
                req
            })
            .collect();
        batch.read_mask = Some(mask(&["version", "digest"]));

        mock.clear_read_rows_calls().await;
        let response = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
            .await
            .unwrap();

        assert_eq!(response.objects.len(), 5);
        for (index, item) in response.objects.iter().enumerate() {
            let version = index as u64 + 1;
            match stored.get(&version) {
                Some(object) => {
                    assert_eq!(item.object().version, Some(version), "{item:?}");
                    assert_eq!(
                        item.object().digest,
                        Some(object.digest().to_string()),
                        "{item:?}"
                    );
                }
                None => assert_eq!(item.error().code, tonic::Code::NotFound as i32, "{item:?}"),
            }
        }

        let reads: Vec<_> = mock
            .read_rows_calls()
            .await
            .into_iter()
            .filter(|call| call.table == objects::NAME)
            .map(|call| call.row_keys)
            .collect();
        assert_eq!(
            reads.len(),
            3,
            "five keys in reads of at most two: {reads:?}"
        );
        assert!(reads.iter().all(|keys| keys.len() <= 2), "{reads:?}");
        let mut read_keys: Vec<_> = reads.into_iter().flatten().collect();
        read_keys.sort();
        let requested_keys: Vec<_> = (1..=5)
            .map(|version| objects::encode_key(&ObjectKey(id, SequenceNumber::from_u64(version))))
            .collect();
        assert_eq!(read_keys, requested_keys, "each key is read exactly once");

        mock_server.abort();
    }

    /// The `objects` rows that `at_checkpoint` scans name go out together, as reads of the
    /// stage's `max-seeks-per-read` keys, even when the scans finish a few at a time.
    #[tokio::test]
    async fn test_batch_at_checkpoint_rows_go_out_as_objects_stage_reads() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let stages = StagesConfig {
            objects: Some(StageConfig {
                max_seeks_per_read: Some(4),
                ..Default::default()
            }),
            ..Default::default()
        };
        // Two permits per request, and 10ms per row read, so the scans finish two at a time.
        let kv_rpc = start_kv_rpc_with_request_concurrency(addr, stages, true, 2).await;
        mock.set_read_rows_latency(Duration::from_millis(10)).await;

        // Eight objects, each live at version 1 from checkpoint 1.
        let stored: Vec<Object> = (0..8)
            .map(|_| {
                Object::with_id_owner_version_for_testing(
                    ObjectID::random(),
                    SequenceNumber::from_u64(1),
                    Owner::AddressOwner(SuiAddress::ZERO),
                )
            })
            .collect();
        for object in &stored {
            mock.insert_row(
                objects::NAME,
                objects::encode_key(&ObjectKey(object.id(), object.version())),
                objects::encode(object).unwrap(),
            )
            .await;
            mock.insert_row(
                object_versions_by_checkpoint::NAME,
                object_versions_by_checkpoint::encode_key(&object.id(), 1),
                object_versions_by_checkpoint::encode(object.version(), ObjectStatus::Live),
            )
            .await;
        }
        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = stored
            .iter()
            .map(|object| {
                let mut req = GetObjectRequest::default();
                req.object_id = Some(object.id().to_canonical_string(true));
                req.at_checkpoint = Some(5);
                req
            })
            .collect();
        batch.read_mask = Some(mask(&["version", "digest"]));

        mock.clear_read_rows_calls().await;
        let response = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
            .await
            .unwrap();
        for item in &response.objects {
            assert_eq!(item.object().version, Some(1), "{item:?}");
        }

        let read_sizes: Vec<usize> = mock
            .read_rows_calls()
            .await
            .into_iter()
            .filter(|call| call.table == objects::NAME)
            .map(|call| call.row_keys.len())
            .collect();
        assert_eq!(read_sizes, [4, 4], "eight rows in reads of four");

        mock_server.abort();
    }

    /// Every request of a batch gets its own lookup's answer, in request order, including a
    /// request that repeats an earlier one, and each `objects` row is read once: an exact version
    /// and an `at_checkpoint` lookup that resolves to it share one read.
    #[tokio::test]
    async fn test_batch_answers_each_request_and_reads_each_row_once() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), true).await;

        // `pinned` is live at version 3 from checkpoint 5; `latest` has one version, 1.
        let pinned = Object::with_id_owner_version_for_testing(
            ObjectID::random(),
            SequenceNumber::from_u64(3),
            Owner::AddressOwner(SuiAddress::ZERO),
        );
        let latest = Object::with_id_owner_version_for_testing(
            ObjectID::random(),
            SequenceNumber::from_u64(1),
            Owner::AddressOwner(SuiAddress::ZERO),
        );
        for object in [&pinned, &latest] {
            mock.insert_row(
                objects::NAME,
                objects::encode_key(&ObjectKey(object.id(), object.version())),
                objects::encode(object).unwrap(),
            )
            .await;
        }
        mock.insert_row(
            object_versions_by_checkpoint::NAME,
            object_versions_by_checkpoint::encode_key(&pinned.id(), 5),
            object_versions_by_checkpoint::encode(pinned.version(), ObjectStatus::Live),
        )
        .await;

        let request = |object: &Object, version: Option<u64>, at_checkpoint: Option<u64>| {
            let mut req = GetObjectRequest::default();
            req.object_id = Some(object.id().to_canonical_string(true));
            req.version = version;
            req.at_checkpoint = at_checkpoint;
            req
        };
        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = vec![
            request(&pinned, Some(3), None),
            request(&latest, None, None),
            request(&pinned, None, Some(7)),
            request(&pinned, Some(3), None),
            request(&pinned, Some(4), None),
            request(&latest, None, None),
            request(&pinned, None, Some(11)),
        ];
        batch.read_mask = Some(mask(&["object_id", "version", "digest"]));

        mock.clear_read_rows_calls().await;
        let answers: Vec<_> = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
            .await
            .unwrap()
            .objects
            .into_iter()
            .map(|item| match item.result {
                Some(get_object_result::Result::Object(object)) => {
                    Ok((object.object_id, object.version, object.digest))
                }
                Some(get_object_result::Result::Error(status)) => Err(status.code),
                other => panic!("unexpected result: {other:?}"),
            })
            .collect();
        let found = |object: &Object| -> Result<_, i32> {
            Ok((
                Some(object.id().to_canonical_string(true)),
                Some(object.version().value()),
                Some(object.digest().to_string()),
            ))
        };
        assert_eq!(
            answers,
            vec![
                found(&pinned),
                found(&latest),
                found(&pinned),
                found(&pinned),
                Err(tonic::Code::NotFound as i32),
                found(&latest),
                Err(tonic::Code::OutOfRange as i32),
            ],
        );

        let pinned_key = objects::encode_key(&ObjectKey(pinned.id(), pinned.version()));
        let pinned_reads = mock
            .read_rows_calls()
            .await
            .into_iter()
            .filter(|call| call.table == objects::NAME)
            .flat_map(|call| call.row_keys)
            .filter(|key| *key == pinned_key)
            .count();
        assert_eq!(pinned_reads, 1, "three requests share one read of the row");

        mock_server.abort();
    }

    /// A batch whose scans outnumber its request's permits still completes: a scan that holds a
    /// permit runs to its end and returns the permit, whether or not the `objects` reads it feeds
    /// are taking lookups.
    #[tokio::test]
    async fn test_batch_scans_do_not_starve_object_reads_of_permits() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        // Two permits per request, and one `objects` read of one key at a time.
        let stages = StagesConfig {
            objects: Some(StageConfig {
                max_seeks_per_read: Some(1),
                concurrency: Some(1),
                ..Default::default()
            }),
            ..Default::default()
        };
        let kv_rpc = start_kv_rpc_with_request_concurrency(addr, stages, true, 2).await;

        // Eight objects, each live at version 1 from checkpoint 1.
        let stored: Vec<Object> = (0..8)
            .map(|_| {
                Object::with_id_owner_version_for_testing(
                    ObjectID::random(),
                    SequenceNumber::from_u64(1),
                    Owner::AddressOwner(SuiAddress::ZERO),
                )
            })
            .collect();
        for object in &stored {
            mock.insert_row(
                objects::NAME,
                objects::encode_key(&ObjectKey(object.id(), object.version())),
                objects::encode(object).unwrap(),
            )
            .await;
            mock.insert_row(
                object_versions_by_checkpoint::NAME,
                object_versions_by_checkpoint::encode_key(&object.id(), 1),
                object_versions_by_checkpoint::encode(object.version(), ObjectStatus::Live),
            )
            .await;
        }

        // Each object at version 1, latest, and at checkpoint 5.
        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = stored
            .iter()
            .flat_map(|object| {
                [(Some(1), None), (None, None), (None, Some(5))].map(|(version, at_checkpoint)| {
                    let mut req = GetObjectRequest::default();
                    req.object_id = Some(object.id().to_canonical_string(true));
                    req.version = version;
                    req.at_checkpoint = at_checkpoint;
                    req
                })
            })
            .collect();
        batch.read_mask = Some(mask(&["object_id", "version", "digest"]));

        let response = tokio::time::timeout(
            Duration::from_secs(10),
            batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch)),
        )
        .await
        .expect("the batch stalled on its own limiter")
        .unwrap();
        assert_eq!(response.objects.len(), 24);
        for item in &response.objects {
            assert_eq!(item.object().version, Some(1), "{item:?}");
        }

        mock_server.abort();
    }

    #[tokio::test]
    async fn test_batch_get_objects_limit_exceeded() {
        let mock = MockBigtableServer::new();
        let (addr, server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), false).await;

        let requests = (0..MAX_BATCH_REQUESTS + 1)
            .map(|_| {
                let mut req = GetObjectRequest::default();
                req.object_id = Some(ObjectID::random().to_canonical_string(true));
                req.version = Some(1);
                req
            })
            .collect();

        let mut req = BatchGetObjectsRequest::default();
        req.requests = requests;

        let err = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(req))
            .await
            .unwrap_err();
        let status: tonic::Status = err.into();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);

        server.abort();
    }
}
