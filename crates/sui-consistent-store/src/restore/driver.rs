// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Generic restore driver.
//!
//! The driver consumes a [`RestoreSource`] (a pluggable supply of
//! live objects) and bulk-loads every registered [`Restore`]
//! pipeline into a single [`Db`]. Per-shard cursors are persisted
//! in the framework's `__restore` column family so a crashed
//! restore picks up exactly where the last commit landed:
//!
//! - Each shard runs as its own tokio task and consumes its
//!   [`RestoreChunk`] stream sequentially.
//! - Per chunk, one atomic [`Batch`] carries every registered
//!   pipeline's data writes plus the cursor advance for every
//!   pipeline. The atomicity guarantees crash recovery never
//!   double-applies a non-idempotent write (such as a `merge`).
//! - When a shard's stream ends, the driver marks that shard
//!   `Done` for every pipeline in one atomic batch.
//! - When every shard is `Done` for every pipeline, the driver
//!   transitions each pipeline's row to
//!   [`restore_state::Complete`].
//!
//! A single asynchronous mutex serialises every `__restore`
//! commit (per-chunk and shard-`Done`). Each batch snapshots a
//! pipeline's full `__restore` row — including every *other*
//! shard's cursor — at staging time, so commits must be totally
//! ordered by the mutex: a batch committed after the lock is
//! released could land after a peer shard's newer commit and
//! rewind that peer's persisted cursor, making a crash-resume
//! replay the peer's last chunk and double-apply its
//! non-idempotent (`merge`) writes. Cross-shard parallelism
//! comes from the source-fetch and stage phases (where the real
//! work lives). RocksDB's own WAL serialises writes anyway, so
//! the mutex costs little beyond bookkeeping.

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Context as _;
use anyhow::bail;
use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use futures::stream;
use futures::stream::BoxStream;
use sui_futures::service::Service;
use sui_futures::stream::TrySpawnStreamExt;
use sui_types::object::Object;
use tokio::sync::Mutex;
use tracing::info;
use tracing::warn;

use crate::Batch;
use crate::ChainId;
use crate::Db;
use crate::FrameworkSchema;
use crate::PipelineTaskKey;
use crate::RestoreState;
use crate::Watermark;
use crate::restore::Restore;
use crate::restore::metrics::RestoreMetrics;
use crate::restore_state;
use crate::restore_state::ShardProgress;
use crate::restore_state::shard_progress;

/// One chunk of objects yielded by a [`RestoreSource`] stream.
///
/// The driver writes every object in `objects` through every
/// registered pipeline and advances the shard's resume cursor to
/// `cursor` in one atomic [`Batch`].
pub struct RestoreChunk {
    /// Live objects to feed to every registered pipeline.
    pub objects: Vec<Object>,

    /// Opaque cursor describing the position of this chunk in the
    /// source's stream. The driver persists `cursor` atomically
    /// with `objects`' writes; on restart the source is asked to
    /// resume from immediately after this cursor.
    ///
    /// Encoding is the source's responsibility. The driver only
    /// stores and compares these as raw bytes.
    pub cursor: Bytes,
}

/// Pluggable supply of live objects for a [`RestoreDriver`].
///
/// Sources expose one or more independent *shards*, each of which
/// the driver iterates as a sequential stream of
/// [`RestoreChunk`]s. Shards are disjoint slices of the live
/// object set so the driver may iterate them in parallel without
/// observing the same object twice.
#[async_trait]
pub trait RestoreSource: Send + Sync + 'static {
    /// Anchor checkpoint for this restore. Tip indexing resumes
    /// at `target_checkpoint + 1` once the driver finishes.
    fn target_checkpoint(&self) -> u64;

    /// Full anchor [`Watermark`] for this restore.
    ///
    /// The driver writes this row into the framework's
    /// `__watermark` CF for every registered pipeline at the
    /// end of the run, so the framework's tip-indexing path
    /// resumes at `target_checkpoint + 1` (and other watermark
    /// fields are populated for any consumer that reads them).
    ///
    /// Default impl returns a watermark with only
    /// `checkpoint_hi_inclusive` set — sources that know more
    /// (epoch, tx count, timestamp) should override.
    fn target_watermark(&self) -> Watermark {
        Watermark::for_checkpoint(self.target_checkpoint())
    }

    /// Chain identifier the restored data belongs to.
    ///
    /// The driver writes this row into the framework's
    /// `__chain_id` CF for every registered pipeline at the
    /// end of the run. Tip indexing's `accepts_chain_id` check
    /// will then refuse the first checkpoint if it belongs to
    /// a different chain than the one we restored from,
    /// instead of silently recording the incoming chain id as
    /// authoritative.
    ///
    /// Required (no default) because the whole point of this
    /// method is to plug the "silently accept any chain" gap
    /// — a `[0u8; 32]` default would just paper over it.
    fn target_chain_id(&self) -> ChainId;

    /// Number of shards the source is split into. The driver
    /// will call [`stream`](Self::stream) once per `shard_id`
    /// in `0..shards()`. Sources that do not naturally shard
    /// should return `1`.
    fn shards(&self) -> u32 {
        1
    }

    /// Open the stream for `shard_id`, resuming from `cursor`.
    ///
    /// If `cursor` is `None`, the shard starts from the
    /// beginning. Otherwise the source resumes from immediately
    /// after the cursor that was last yielded via
    /// [`RestoreChunk::cursor`].
    ///
    /// Each chunk's `objects` and `cursor` must fit in memory and
    /// the per-pipeline writes derived from `objects` must fit in
    /// a single [`Batch`]. The chunk is the unit of atomic
    /// commit and resume granularity.
    fn stream(
        &self,
        shard_id: u32,
        cursor: Option<Bytes>,
    ) -> BoxStream<'_, anyhow::Result<RestoreChunk>>;
}

/// Knobs for [`RestoreDriver::run`].
#[derive(Clone, Debug, Default)]
pub struct RestoreDriverConfig {
    /// Maximum number of shards to iterate concurrently. `None`
    /// uses the source's shard count, i.e. every shard runs in
    /// parallel.
    pub shard_concurrency: Option<usize>,
}

/// Type-erased view of a registered [`Restore`] pipeline.
///
/// `dyn` lets the driver hold pipelines of unrelated concrete
/// types in one `Vec`; the blanket impl below forwards through
/// to the trait method.
trait DynRestore<S>: Send + Sync {
    fn name(&self) -> &'static str;
    fn restore(&self, schema: &S, object: &Object, batch: &mut Batch) -> anyhow::Result<()>;
}

impl<R> DynRestore<R::Schema> for R
where
    R: Restore + Send + Sync,
{
    fn name(&self) -> &'static str {
        R::NAME
    }

    fn restore(
        &self,
        schema: &R::Schema,
        object: &Object,
        batch: &mut Batch,
    ) -> anyhow::Result<()> {
        Restore::restore(self, schema, object, batch)
    }
}

/// Single registered pipeline (without its mutable state).
struct PipelineEntry<S: Send + Sync + 'static> {
    pipeline: Arc<dyn DynRestore<S>>,
    key: PipelineTaskKey,
}

/// Drives a bulk-load of registered pipelines from a
/// [`RestoreSource`].
///
/// Constructed via [`RestoreDriver::new`]; pipelines are added
/// with [`RestoreDriver::register`]; the actual run is started
/// with [`RestoreDriver::run`], which returns a [`Service`].
pub struct RestoreDriver<S: Send + Sync + 'static, Src: RestoreSource> {
    db: Db,
    schema: Arc<S>,
    source: Arc<Src>,
    config: RestoreDriverConfig,
    metrics: Arc<RestoreMetrics>,
    /// Pipelines registered so far, in registration order. Order
    /// determines the parallel `Vec` index used in `initial_states`.
    pipelines: Vec<PipelineEntry<S>>,
    /// Mirrors `pipelines`: each entry holds the [`RestoreState`]
    /// loaded from `__restore` at registration time. The driver
    /// hands this off to the shared `Vec<Mutex<RestoreState>>`
    /// when [`run`](Self::run) is called.
    initial_states: Vec<RestoreState>,
}

impl<S: Send + Sync + 'static, Src: RestoreSource> RestoreDriver<S, Src> {
    /// Build a driver bound to `db` / `schema` and `source`. The
    /// returned driver has no pipelines yet — call
    /// [`register`](Self::register) before [`run`](Self::run).
    pub fn new(
        db: Db,
        schema: Arc<S>,
        source: Src,
        config: RestoreDriverConfig,
        metrics: Arc<RestoreMetrics>,
    ) -> Self {
        Self {
            db,
            schema,
            source: Arc::new(source),
            config,
            metrics,
            pipelines: Vec::new(),
            initial_states: Vec::new(),
        }
    }

    /// Register a pipeline to be restored. Reads the persisted
    /// [`RestoreState`] for the pipeline, validates it against
    /// the source's `target_checkpoint`, and (if no row exists)
    /// persists an empty `InProgress` so a subsequent crash
    /// resumes consistently.
    ///
    /// Returns `Err` if the pipeline is already `Complete`, if
    /// the persisted `target_checkpoint` disagrees with the
    /// source's, or if the pipeline name has already been
    /// registered with this driver.
    pub fn register<P>(&mut self, pipeline: P) -> anyhow::Result<&mut Self>
    where
        P: Restore<Schema = S> + Send + Sync + 'static,
    {
        let key = PipelineTaskKey::new(P::NAME);

        if self.pipelines.iter().any(|e| e.key == key) {
            bail!("pipeline {:?} already registered", P::NAME);
        }

        let framework = self.db.framework();
        let target_checkpoint = self.source.target_checkpoint();
        let existing = framework
            .restore
            .get(&key)
            .with_context(|| format!("read restore state for pipeline {:?}", P::NAME))?;

        let state = match existing.as_ref().and_then(|s| s.state.as_ref()) {
            None => {
                let fresh = RestoreState::default().with_in_progress(restore_state::InProgress {
                    target_checkpoint,
                    shards: BTreeMap::new(),
                });
                let owned_framework = FrameworkSchema::new(self.db.clone());
                let mut batch = self.db.batch();
                batch.put(&owned_framework.restore, &key, &fresh)?;
                batch.commit()?;
                fresh
            }
            Some(restore_state::State::InProgress(ip)) => {
                if ip.target_checkpoint != target_checkpoint {
                    bail!(
                        "pipeline {:?} has an in-progress restore at checkpoint {} \
                         but the source's target is {}",
                        P::NAME,
                        ip.target_checkpoint,
                        target_checkpoint,
                    );
                }
                existing.expect("Some matched above")
            }
            Some(restore_state::State::Complete(c)) => {
                bail!(
                    "pipeline {:?} already restored at checkpoint {}",
                    P::NAME,
                    c.restored_at,
                );
            }
        };

        self.pipelines.push(PipelineEntry {
            pipeline: Arc::new(pipeline),
            key,
        });
        self.initial_states.push(state);
        Ok(self)
    }

    /// Spawn the restore as a [`Service`]: one tokio task per
    /// shard (up to [`RestoreDriverConfig::shard_concurrency`])
    /// plus a finaliser that transitions each pipeline to
    /// [`restore_state::Complete`] once every shard is `Done`.
    ///
    /// The returned `Service`'s primary task completes once every
    /// shard's stream is exhausted and every pipeline is
    /// finalised (or any task fails).
    pub fn run(self) -> anyhow::Result<Service> {
        if self.pipelines.is_empty() {
            bail!("no pipelines registered with the restore driver");
        }

        let RestoreDriver {
            db,
            schema,
            source,
            config,
            metrics,
            pipelines,
            initial_states,
        } = self;

        let target_checkpoint = source.target_checkpoint();
        let target_watermark = source.target_watermark();
        let target_chain_id = source.target_chain_id();
        let shard_count = source.shards();
        let shard_concurrency = config
            .shard_concurrency
            .unwrap_or(shard_count.max(1) as usize);
        let pipelines = Arc::new(pipelines);
        let states = Arc::new(Mutex::new(initial_states));

        // `restore_shards_done` is incremented by each shard task as it
        // confirms its shard is `Done` (including shards already
        // complete on resume), so it climbs from 0 to `shard_count`
        // and reflects cumulative cross-resume progress.
        metrics.restore_shards_total.set(shard_count as i64);
        metrics.restore_shards_done.set(0);

        let svc = Service::new().spawn(async move {
            stream::iter(0..shard_count)
                .try_for_each_spawned(shard_concurrency, |shard_id| {
                    let source = source.clone();
                    let pipelines = pipelines.clone();
                    let states = states.clone();
                    let db = db.clone();
                    let schema = schema.clone();
                    let metrics = metrics.clone();
                    async move {
                        run_shard(shard_id, source, pipelines, states, db, schema, metrics)
                            .await
                            .with_context(|| format!("shard {shard_id} restore failed"))
                    }
                })
                .await?;

            finalize(
                target_checkpoint,
                &target_watermark,
                target_chain_id,
                shard_count,
                pipelines,
                states,
                db,
            )
            .await
            .context("finalizer failed")
        });

        Ok(svc)
    }
}

/// Drive one shard's stream end-to-end: pick up its persisted
/// cursor (if any), iterate the source-supplied chunks, and
/// commit each chunk atomically with the shard's per-pipeline
/// cursor advance. On stream end, mark the shard `Done` for
/// every pipeline.
async fn run_shard<S, Src>(
    shard_id: u32,
    source: Arc<Src>,
    pipelines: Arc<Vec<PipelineEntry<S>>>,
    states: Arc<Mutex<Vec<RestoreState>>>,
    db: Db,
    schema: Arc<S>,
    metrics: Arc<RestoreMetrics>,
) -> anyhow::Result<()>
where
    S: Send + Sync + 'static,
    Src: RestoreSource,
{
    let framework = FrameworkSchema::new(db.clone());

    // Pull the initial cursor from the snapshot of state taken
    // at registration. Validate that every pipeline's view of
    // the shard agrees: either all are unstarted, all are
    // mid-stream at the same cursor, or all are Done.
    let initial_cursor = {
        let states = states.lock().await;
        match shard_status(shard_id, pipelines.as_ref(), &states)? {
            ShardStatus::AllDone => {
                info!(
                    shard_id,
                    "shard already complete for every pipeline; skipping",
                );
                // Count shards finished by a prior run so the gauge
                // reflects cumulative progress, not just this session.
                metrics.restore_shards_done.inc();
                return Ok(());
            }
            ShardStatus::Fresh => None,
            ShardStatus::Resume(c) => Some(c),
        }
    };

    info!(shard_id, resume = initial_cursor.is_some(), "shard start");

    let mut stream = source.stream(shard_id, initial_cursor);
    let mut chunks = 0u64;
    let mut objects = 0u64;

    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.with_context(|| format!("source produced an error on shard {shard_id}"))?;
        let chunk_objs = chunk.objects.len() as u64;

        let mut batch = db.batch();
        for object in &chunk.objects {
            for entry in pipelines.iter() {
                entry
                    .pipeline
                    .restore(&schema, object, &mut batch)
                    .with_context(|| {
                        format!(
                            "pipeline {:?} failed to restore object {} on shard {shard_id}",
                            entry.pipeline.name(),
                            object.id(),
                        )
                    })?;
            }
        }

        let mut states = states.lock().await;
        for (i, entry) in pipelines.iter().enumerate() {
            update_shard_state(
                &mut states[i],
                shard_id,
                ShardProgress::default().with_in_progress(chunk.cursor.clone()),
            )?;
            batch.put(&framework.restore, &entry.key, &states[i])?;
        }
        batch.commit()?;
        drop(states);

        chunks += 1;
        objects += chunk_objs;
    }

    // Stream exhausted: flip the shard to Done for every pipeline.
    // Commit while still holding the states lock (as the chunk loop
    // above does): the staged rows snapshot peer shards' cursors, so
    // a commit outside the lock could land after a peer's newer chunk
    // commit and rewind its persisted cursor, making a crash-resume
    // replay that chunk and double-apply its merge writes.
    let mut batch = db.batch();
    let mut states = states.lock().await;
    for (i, entry) in pipelines.iter().enumerate() {
        update_shard_state(
            &mut states[i],
            shard_id,
            ShardProgress::default().with_done(shard_progress::Done {}),
        )?;
        batch.put(&framework.restore, &entry.key, &states[i])?;
    }
    batch.commit()?;
    drop(states);

    metrics.restore_shards_done.inc();
    info!(shard_id, chunks, objects, "shard done");
    Ok(())
}

/// Final transition: read each pipeline's `__restore` row,
/// confirm every shard is `Done`, and atomically rewrite the
/// row as `Complete { restored_at: target_checkpoint }`.
/// `__watermark` is set so tip indexing resumes at the right
/// checkpoint, and `__chain_id` is set so the framework's
/// chain-id check refuses checkpoints from a different chain.
/// All three writes commit together in one batch.
async fn finalize<S>(
    target_checkpoint: u64,
    target_watermark: &Watermark,
    target_chain_id: ChainId,
    shard_count: u32,
    pipelines: Arc<Vec<PipelineEntry<S>>>,
    states: Arc<Mutex<Vec<RestoreState>>>,
    db: Db,
) -> anyhow::Result<()>
where
    S: Send + Sync + 'static,
{
    let framework = FrameworkSchema::new(db.clone());
    let states = states.lock().await;
    let mut batch = db.batch();

    for (i, entry) in pipelines.iter().enumerate() {
        let in_progress = match states[i].state.as_ref() {
            Some(restore_state::State::InProgress(ip)) => ip,
            Some(restore_state::State::Complete(_)) => {
                warn!(
                    pipeline = entry.pipeline.name(),
                    "pipeline already Complete at finalize time; skipping",
                );
                continue;
            }
            None => bail!(
                "pipeline {:?} has no state at finalize",
                entry.pipeline.name()
            ),
        };

        for shard in 0..shard_count {
            let entry_state = in_progress
                .shards
                .get(&shard)
                .and_then(|s| s.state.as_ref());
            if !matches!(entry_state, Some(shard_progress::State::Done(_))) {
                bail!(
                    "pipeline {:?} shard {shard} is not Done at finalize time",
                    entry.pipeline.name(),
                );
            }
        }

        let complete = RestoreState::default().with_complete(restore_state::Complete {
            restored_at: target_checkpoint,
        });
        batch.put(&framework.restore, &entry.key, &complete)?;
        batch.put(&framework.watermarks, &entry.key, target_watermark)?;
        batch.put(&framework.chain_ids, &entry.key, &target_chain_id)?;
    }

    batch.commit()?;
    Ok(())
}

/// Summary of a shard's persisted state across all pipelines,
/// used by [`run_shard`] to decide whether to start fresh,
/// resume from a cursor, or skip the shard outright.
enum ShardStatus {
    /// No pipeline has touched this shard yet.
    Fresh,
    /// All pipelines have the shard mid-stream at the same
    /// cursor — resume from there.
    Resume(Bytes),
    /// Every pipeline has the shard marked `Done`.
    AllDone,
}

/// Inspect the persisted shard state across all pipelines and
/// project it into a [`ShardStatus`]. Errors if pipelines
/// disagree (an indication of corrupt state or a misuse).
fn shard_status<S>(
    shard_id: u32,
    pipelines: &[PipelineEntry<S>],
    states: &[RestoreState],
) -> anyhow::Result<ShardStatus>
where
    S: Send + Sync + 'static,
{
    let mut cursor: Option<Bytes> = None;
    let mut done = 0usize;
    let mut fresh = 0usize;

    if pipelines.len() != states.len() {
        bail!(
            "shard {shard_id}: pipelines ({}) and states ({}) have different lengths",
            pipelines.len(),
            states.len(),
        );
    }

    for i in 0..pipelines.len() {
        let entry = &pipelines[i];
        let state = &states[i];
        let in_progress = match state.state.as_ref() {
            Some(restore_state::State::InProgress(ip)) => ip,
            Some(restore_state::State::Complete(_)) => {
                bail!(
                    "pipeline {:?} is Complete but the driver is still running",
                    entry.pipeline.name(),
                );
            }
            None => bail!(
                "pipeline {:?} has no state (driver invariant)",
                entry.pipeline.name(),
            ),
        };

        match in_progress
            .shards
            .get(&shard_id)
            .and_then(|s| s.state.as_ref())
        {
            None => fresh += 1,
            Some(shard_progress::State::Done(_)) => done += 1,
            Some(shard_progress::State::InProgress(c)) => match cursor.as_ref() {
                None => cursor = Some(c.clone()),
                Some(existing) if existing == c => {}
                Some(existing) => bail!(
                    "pipeline cursors disagree on shard {shard_id}: \
                     pipeline {:?} has {c:?}, prior pipelines had {existing:?}",
                    entry.pipeline.name(),
                ),
            },
        }
    }

    match (done, fresh, cursor) {
        (d, _, _) if d == pipelines.len() => Ok(ShardStatus::AllDone),
        (0, f, None) if f == pipelines.len() => Ok(ShardStatus::Fresh),
        (0, 0, Some(c)) => Ok(ShardStatus::Resume(c)),
        (done, fresh, cursor) => bail!(
            "shard {shard_id}: pipelines disagree on shard state \
             (done={done}, fresh={fresh}, resume={})",
            cursor.is_some(),
        ),
    }
}

/// Mutate `state.in_progress.shards[shard_id]` to `entry`.
/// Errors if `state` is not [`restore_state::State::InProgress`].
fn update_shard_state(
    state: &mut RestoreState,
    shard_id: u32,
    entry: ShardProgress,
) -> anyhow::Result<()> {
    let in_progress = match state.state.as_mut() {
        Some(restore_state::State::InProgress(ip)) => ip,
        _ => bail!("update_shard_state: state is not InProgress"),
    };
    in_progress.shards.insert(shard_id, entry);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex as StdMutex;

    use sui_indexer_alt_framework::pipeline::Processor;
    use sui_types::base_types::ObjectID;
    use sui_types::object::Object;

    use super::*;
    use crate::restore::test_pipeline::ObjectIdKey;
    use crate::restore::test_pipeline::ObjectVersionPipeline;
    use crate::restore::test_pipeline::U64Be;
    use crate::restore::test_pipeline::open;

    /// In-memory [`RestoreSource`] that yields a pre-baked map
    /// of `shard_id -> Vec<RestoreChunk>`. The cursor of each
    /// chunk is the encoded index of that chunk within its
    /// shard (big-endian `u32`), so resume-from-cursor works
    /// by skipping any chunk whose index is `<= cursor`.
    struct VecSource {
        target: u64,
        chain_id: ChainId,
        shards: Vec<Vec<RestoreChunk>>,
        /// Tally of how many chunks each shard yielded across
        /// every `stream()` call. Tests use this to assert that
        /// resume skips already-restored chunks.
        yielded: Arc<StdMutex<HashMap<u32, usize>>>,
    }

    impl VecSource {
        fn new(target: u64, shards: Vec<Vec<RestoreChunk>>) -> Self {
            Self {
                target,
                chain_id: ChainId([1u8; 32]),
                shards,
                yielded: Arc::new(StdMutex::new(HashMap::new())),
            }
        }
    }

    #[async_trait]
    impl RestoreSource for VecSource {
        fn target_checkpoint(&self) -> u64 {
            self.target
        }

        fn target_chain_id(&self) -> ChainId {
            self.chain_id
        }

        fn shards(&self) -> u32 {
            self.shards.len() as u32
        }

        fn stream(
            &self,
            shard_id: u32,
            cursor: Option<Bytes>,
        ) -> BoxStream<'_, anyhow::Result<RestoreChunk>> {
            let resume_after = cursor.map(|c| {
                let mut buf = [0u8; 4];
                buf.copy_from_slice(&c[..4]);
                u32::from_be_bytes(buf)
            });
            let yielded = self.yielded.clone();
            let chunks: Vec<_> = self.shards[shard_id as usize]
                .iter()
                .enumerate()
                .filter_map(move |(i, chunk)| {
                    let i = i as u32;
                    if let Some(after) = resume_after
                        && i <= after
                    {
                        None
                    } else {
                        Some(RestoreChunk {
                            objects: chunk.objects.clone(),
                            cursor: chunk.cursor.clone(),
                        })
                    }
                })
                .collect();
            *yielded.lock().unwrap().entry(shard_id).or_insert(0) += chunks.len();
            stream::iter(chunks.into_iter().map(Ok)).boxed()
        }
    }

    fn chunk(idx: u32, objects: Vec<Object>) -> RestoreChunk {
        RestoreChunk {
            objects,
            cursor: Bytes::copy_from_slice(&idx.to_be_bytes()),
        }
    }

    fn obj(id: u8) -> Object {
        Object::immutable_with_id_for_testing(ObjectID::from_single_byte(id))
    }

    /// Assert the persisted pipeline state is `Complete { restored_at }`,
    /// the watermark row matches `expected_watermark`, and the chain
    /// id is pinned to `expected_chain_id`.
    fn assert_complete(
        db: &Db,
        name: &str,
        expected_watermark: &Watermark,
        expected_chain_id: ChainId,
    ) {
        let key = PipelineTaskKey::new(name);
        let state = db.framework().restore.get(&key).unwrap().unwrap();
        match state.state.unwrap() {
            restore_state::State::Complete(c) => {
                assert_eq!(c.restored_at, expected_watermark.checkpoint_hi_inclusive,)
            }
            other => panic!("expected Complete, got {other:?}"),
        }
        let watermark = db.framework().watermarks.get(&key).unwrap().unwrap();
        assert_eq!(&watermark, expected_watermark);
        let chain_id = db.framework().chain_ids.get(&key).unwrap().unwrap();
        assert_eq!(chain_id, expected_chain_id);
    }

    /// One-shard run: a single shard with two chunks. Verifies
    /// every object is restored and the pipeline transitions to
    /// `Complete`.
    #[tokio::test]
    async fn single_shard_runs_to_completion() {
        let (_dir, db, schema) = open();
        let schema = Arc::new(schema);
        let objects = [obj(1), obj(2), obj(3)];
        let source = VecSource::new(
            42,
            vec![vec![
                chunk(0, objects[..2].to_vec()),
                chunk(1, vec![objects[2].clone()]),
            ]],
        );

        let mut driver = RestoreDriver::new(
            db.clone(),
            schema.clone(),
            source,
            RestoreDriverConfig::default(),
            RestoreMetrics::new(None, &prometheus::Registry::new()),
        );
        driver.register(ObjectVersionPipeline).unwrap();
        driver.run().unwrap().shutdown().await.unwrap();

        for o in &objects {
            assert_eq!(
                schema.versions.get(&ObjectIdKey::new(o.id())).unwrap(),
                Some(U64Be(o.version().value())),
            );
        }
        assert_complete(
            &db,
            ObjectVersionPipeline::NAME,
            &Watermark::for_checkpoint(42),
            ChainId([1u8; 32]),
        );
    }

    /// Multiple shards in parallel: every shard's objects land
    /// in the schema and the pipeline finishes Complete.
    #[tokio::test]
    async fn multiple_shards_run_to_completion() {
        let (_dir, db, schema) = open();
        let schema = Arc::new(schema);
        let objects = [obj(1), obj(2), obj(3), obj(4)];
        let source = VecSource::new(
            7,
            vec![
                vec![chunk(0, objects[..2].to_vec())],
                vec![
                    chunk(0, vec![objects[2].clone()]),
                    chunk(1, vec![objects[3].clone()]),
                ],
                vec![], // empty shard
            ],
        );

        let mut driver = RestoreDriver::new(
            db.clone(),
            schema.clone(),
            source,
            RestoreDriverConfig::default(),
            RestoreMetrics::new(None, &prometheus::Registry::new()),
        );
        driver.register(ObjectVersionPipeline).unwrap();
        driver.run().unwrap().shutdown().await.unwrap();

        for o in &objects {
            assert_eq!(
                schema.versions.get(&ObjectIdKey::new(o.id())).unwrap(),
                Some(U64Be(o.version().value())),
            );
        }
        assert_complete(
            &db,
            ObjectVersionPipeline::NAME,
            &Watermark::for_checkpoint(7),
            ChainId([1u8; 32]),
        );
    }

    /// Resume from a partial restore: pre-seed the pipeline's
    /// `__restore` row with one shard already mid-stream and
    /// another already Done, then run the driver and confirm
    /// it only emits the missing chunks.
    #[tokio::test]
    async fn resume_skips_already_committed_chunks() {
        let (_dir, db, schema) = open();
        let schema = Arc::new(schema);

        // Pre-seed: shard 0 is mid-stream at cursor=0 (i.e.
        // chunk index 0 was already committed), shard 1 is
        // Done.
        let key = PipelineTaskKey::new(ObjectVersionPipeline::NAME);
        let in_progress = restore_state::InProgress {
            target_checkpoint: 99,
            shards: [
                (
                    0u32,
                    ShardProgress::default()
                        .with_in_progress(Bytes::copy_from_slice(&0u32.to_be_bytes())),
                ),
                (
                    1u32,
                    ShardProgress::default().with_done(shard_progress::Done {}),
                ),
            ]
            .into_iter()
            .collect(),
        };
        let seeded = RestoreState::default().with_in_progress(in_progress);
        let framework = FrameworkSchema::new(db.clone());
        let mut batch = db.batch();
        batch.put(&framework.restore, &key, &seeded).unwrap();
        batch.commit().unwrap();

        // Drive: shard 0 yields two chunks; shard 1 would
        // yield one. With the seed, we expect shard 0's chunk
        // 1 only and nothing from shard 1.
        let o1 = obj(1);
        let o2 = obj(2);
        let o3 = obj(3);
        let source = VecSource::new(
            99,
            vec![
                vec![chunk(0, vec![o1.clone()]), chunk(1, vec![o2.clone()])],
                vec![chunk(0, vec![o3.clone()])],
            ],
        );
        let yielded_handle = source.yielded.clone();

        let metrics = RestoreMetrics::new(None, &prometheus::Registry::new());
        let mut driver = RestoreDriver::new(
            db.clone(),
            schema.clone(),
            source,
            RestoreDriverConfig::default(),
            metrics.clone(),
        );
        driver.register(ObjectVersionPipeline).unwrap();
        driver.run().unwrap().shutdown().await.unwrap();

        // The shards-done gauge counts both the shard finished this
        // session (shard 0) and the one already Done on resume (shard
        // 1), so it reaches the shard total — i.e. it reflects
        // cumulative progress, not just this session's work.
        assert_eq!(metrics.restore_shards_total.get(), 2);
        assert_eq!(metrics.restore_shards_done.get(), 2);

        // Shard 0: one chunk (chunk 1) actually yielded; shard
        // 1 either wasn't streamed at all or yielded zero
        // chunks (it was already Done).
        let yielded = yielded_handle.lock().unwrap();
        assert_eq!(yielded.get(&0).copied().unwrap_or(0), 1);
        assert_eq!(yielded.get(&1).copied().unwrap_or(0), 0);
        drop(yielded);

        // Only shard 0's chunk 1 ran, so object 2 landed.
        assert_eq!(
            schema.versions.get(&ObjectIdKey::new(o2.id())).unwrap(),
            Some(U64Be(o2.version().value())),
        );
        // Object 3 (shard 1) was already Done before our run.
        assert_eq!(
            schema.versions.get(&ObjectIdKey::new(o3.id())).unwrap(),
            None,
        );

        assert_complete(
            &db,
            ObjectVersionPipeline::NAME,
            &Watermark::for_checkpoint(99),
            ChainId([1u8; 32]),
        );
    }
}
