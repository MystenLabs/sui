// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The read side of execution: a read-only `BackingStore` ([`ScanStore`]) for executing a single
//! transaction, backed by the objects present in a streamed checkpoint's object set, plus the
//! shared [`PackageCache`] those reads resolve packages against. Dynamic-field child reads and
//! `Receiving` reads are served from the same object set (a checkpoint carries the objects
//! execution loaded). [`prefetch_package_closure`] warms the cache with a checkpoint's package
//! closure up front; a miss during execution falls back to a lazy gRPC + on-disk fetch.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use anyhow::Context as _;
use move_core_types::language_storage::TypeTag;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, VersionNumber};
use sui_types::committee::EpochId;
use sui_types::effects::{InputConsensusObject, TransactionEffects, TransactionEffectsAPI};
use sui_types::error::SuiResult;
use sui_types::full_checkpoint_content::{Checkpoint, ObjectSet};
use sui_types::is_system_package;
use sui_types::object::{Object, Owner};
use sui_types::storage::{
    BackingPackageStore, ObjectKey, ObjectStore, PackageObject, ParentSync, RuntimeObjectResolver,
};
use sui_types::transaction::{
    InputObjectKind, InputObjects, ObjectReadResult, ObjectReadResultKind, TransactionData,
    TransactionDataAPI, TransactionKind,
};
use tokio::runtime::Handle;
use tracing::warn;

use crate::grpc::RpcClient;

/// Shared, process-wide package cache: in-memory map layered over an optional on-disk cache,
/// falling back to a gRPC fetch from a fullnode. Packages are immutable, so a single entry per id
/// is sufficient. `None` is cached as a negative result to avoid refetching known-missing packages.
/// An `RwLock` (rather than a `Mutex`) lets concurrent execute workers clone their package copies
/// in parallel on the hit path; the exclusive lock is taken only to insert a freshly-loaded entry.
pub struct PackageCache {
    rpc: RpcClient,
    handle: Handle,
    disk: Option<PathBuf>,
    mem: RwLock<HashMap<ObjectID, Option<Object>>>,
}

impl PackageCache {
    pub fn new(rpc: RpcClient, handle: Handle, disk: Option<PathBuf>) -> anyhow::Result<Self> {
        if let Some(dir) = &disk {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("creating package cache dir {}", dir.display()))?;
        }
        Ok(Self {
            rpc,
            handle,
            disk,
            mem: RwLock::new(HashMap::new()),
        })
    }

    fn disk_path(&self, id: ObjectID) -> Option<PathBuf> {
        self.disk
            .as_ref()
            .map(|dir| dir.join(id.to_canonical_string(/* with_prefix */ true)))
    }

    /// Fetch a package object, consulting the in-memory cache, then the on-disk cache, then gRPC.
    /// Must be called from a blocking context (it drives the async fetch via [`Handle::block_on`]),
    /// never from a runtime worker thread.
    fn fetch(&self, id: ObjectID) -> Option<Object> {
        // Hit path under a shared read lock so concurrent workers clone in parallel.
        if let Some(hit) = self.mem.read().unwrap().get(&id) {
            return hit.clone();
        }

        // On-disk cache.
        if let Some(object) = self.read_disk(id) {
            self.mem.write().unwrap().insert(id, Some(object.clone()));
            return Some(object);
        }

        // Reaching here means neither the in-memory nor on-disk cache had the package, so the
        // prefetch closure ([`prefetch_package_closure`]) did not warm it — the correctness net
        // catching a closure gap. This should be rare; a steady stream of these points at a missing
        // prefetch source worth adding (e.g. a new runtime-only package edge).
        warn!(%id, "lazy package fetch during execution (prefetch miss)");

        // gRPC fetch. We may be on a `spawn_blocking` thread (the execution pipeline is synchronous),
        // where `block_on` is disallowed, so drive the async fetch by spawning it onto the runtime
        // and blocking on a channel for the result.
        let fetched = {
            let (tx, rx) = std::sync::mpsc::channel();
            let rpc = self.rpc.clone();
            self.handle.spawn(async move {
                let _ = tx.send(rpc.fetch_object(id).await);
            });
            match rx.recv() {
                Ok(Ok(object)) => object,
                Ok(Err(e)) => {
                    warn!(%id, "package fetch failed: {e:#}");
                    None
                }
                Err(_) => {
                    warn!(%id, "package fetch task dropped");
                    None
                }
            }
        };

        if let (Some(object), Some(path)) = (&fetched, self.disk_path(id))
            && let Ok(bytes) = bcs::to_bytes(object)
        {
            let _ = std::fs::write(&path, bytes);
        }
        self.mem.write().unwrap().insert(id, fetched.clone());
        fetched
    }

    /// Read a package object from the on-disk cache, if present and decodable. No network.
    fn read_disk(&self, id: ObjectID) -> Option<Object> {
        let path = self.disk_path(id)?;
        let bytes = std::fs::read(&path).ok()?;
        bcs::from_bytes::<Object>(&bytes).ok()
    }

    /// Warm the cache with `ids` using a single batched multi-get for the ones not already cached
    /// (in memory or on disk), returning every object now available for those ids. Async and called
    /// from the prefetch stage (no `block_on` bridge); a miss here just isn't inserted and falls
    /// through to the lazy [`Self::fetch`] during execution. Failures are logged and swallowed so a
    /// prefetch hiccup degrades to the lazy path rather than aborting.
    pub async fn prefetch(&self, ids: &[ObjectID]) -> Vec<Object> {
        let mut have = Vec::new();
        let mut to_fetch = Vec::new();
        {
            let mem = self.mem.read().unwrap();
            for &id in ids {
                match mem.get(&id) {
                    // Present (`Some(obj)`) or a cached negative (`None`); either way, no fetch.
                    Some(slot) => have.extend(slot.clone()),
                    None => to_fetch.push(id),
                }
            }
        }
        // Disk hits next: load them into memory, off the network path.
        to_fetch.retain(|&id| match self.read_disk(id) {
            Some(object) => {
                self.mem.write().unwrap().insert(id, Some(object.clone()));
                have.push(object);
                false
            }
            None => true,
        });
        if to_fetch.is_empty() {
            return have;
        }
        match self.rpc.fetch_objects(&to_fetch).await {
            Ok(fetched) => {
                let mut mem = self.mem.write().unwrap();
                for object in fetched {
                    let id = object.id();
                    if let Some(path) = self.disk_path(id)
                        && let Ok(bytes) = bcs::to_bytes(&object)
                    {
                        let _ = std::fs::write(&path, bytes);
                    }
                    mem.insert(id, Some(object.clone()));
                    have.push(object);
                }
            }
            Err(e) => warn!("batch package prefetch failed: {e:#}"),
        }
        have
    }
}

/// Warm `cache` with the package closure of `checkpoint`'s transactions, so the synchronous execute
/// stage reads packages from memory instead of fetching mid-execution.
///
/// The closure is computed statically (no execution): collect the *roots* — the packages the
/// commands reference (via the system's own [`input_objects`](sui_types::transaction::ProgrammableTransaction::input_objects)
/// derivation) plus the defining packages of the checkpoint's object types. Then add, for each
/// fetched root:
///   - its **linkage table** — the (upgraded) storage ids of its transitive declared dependencies,
///   - its **type-origin table** — the storage id of every package version that *introduced* one of
///     its own types. A type added in an upgrade (e.g. a Cetus type first defined in v10) records
///     that version as its origin, and the executor loads the module at that version; since such a
///     type need never appear as a static type argument, the linkage table alone misses it.
///
/// System packages are not prefetched here: they are served version-correctly per epoch from the
/// framework snapshot (see [`crate::context::EpochCtx`] and [`ScanStore::get_package_object`]).
///
/// This remains an over-approximation of what actually executes (and still misses some
/// runtime-only edges, e.g. versions pinned only by a stored object's provenance); a miss falls
/// through to the lazy [`PackageCache::fetch`], which is the correctness net.
pub(crate) async fn prefetch_package_closure(checkpoint: &Checkpoint, cache: &PackageCache) {
    let roots: Vec<ObjectID> = collect_roots(checkpoint).into_iter().collect();
    let root_pkgs = cache.prefetch(&roots).await;

    let mut linked = BTreeSet::new();
    for object in &root_pkgs {
        if let Some(pkg) = object.data.try_as_package() {
            for upgrade in pkg.linkage_table().values() {
                linked.insert(upgrade.upgraded_id);
            }
            for origin in pkg.type_origin_table() {
                linked.insert(origin.package);
            }
        }
    }
    let linked: Vec<ObjectID> = linked.into_iter().collect();
    cache.prefetch(&linked).await;
}

/// The root package set: the packages the transactions' commands reference, plus the defining
/// packages of every object type present in the checkpoint's object set.
fn collect_roots(checkpoint: &Checkpoint) -> BTreeSet<ObjectID> {
    let mut roots = BTreeSet::new();

    for executed in &checkpoint.transactions {
        let TransactionKind::ProgrammableTransaction(pt) = executed.transaction.kind() else {
            continue;
        };
        // Reuse the system's own command package-input derivation: it yields a `MovePackage` for
        // every package the commands reference — MoveCall targets and their type-argument packages,
        // `MakeMoveVec` element types, and `Publish`/`Upgrade` dependency packages. (A miss here
        // just falls through to the lazy fetch, so ignore the rare `input_objects` error.)
        if let Ok(input_objects) = pt.input_objects() {
            roots.extend(input_objects.into_iter().filter_map(|kind| match kind {
                InputObjectKind::MovePackage(id) => Some(id),
                _ => None,
            }));
        }
    }

    // Input object *types* aren't packages in `input_objects`, so pick up their defining packages
    // (and those of their type parameters) from the materialized object set.
    for object in checkpoint.object_set.iter() {
        if let Some(mo) = object.data.try_as_move() {
            let ty: TypeTag = mo.type_().clone().into();
            roots.extend(ty.all_addresses().into_iter().map(ObjectID::from));
        }
    }

    roots
}

/// Per-transaction read-only store over a checkpoint's object set, with package fallback.
pub struct ScanStore {
    /// All object versions present in the checkpoint, keyed by (id, version).
    objects: Arc<BTreeMap<ObjectKey, Object>>,
    /// Highest version seen per object id (for version-less `get_object`).
    latest: Arc<BTreeMap<ObjectID, SequenceNumber>>,
    /// Tombstone (deletion/wrap/unwrap-then-delete) versions per object id, gathered from every
    /// transaction's effects in the checkpoint. A child object that the bundled object set still
    /// carries at a stale version may actually have been *removed* at/under a given root version;
    /// without this, `read_child_object` would resurrect a deleted dynamic field. See
    /// `child_at_bound`.
    tombstones: Arc<BTreeMap<ObjectID, BTreeSet<SequenceNumber>>>,
    packages: Arc<PackageCache>,
    /// The system (framework) packages live during this epoch, keyed by id. System packages share a
    /// stable id across upgrades but differ in bytecode per protocol version, so they are served
    /// from here (resolved version-correctly from the framework snapshot) rather than from the
    /// fullnode, which only returns their latest version. See [`crate::context::EpochCtx`].
    system_packages: Arc<BTreeMap<ObjectID, Object>>,
}

impl ScanStore {
    /// Build the per-checkpoint object index once; cheap to clone (Arc) per transaction.
    pub fn index_object_set(
        object_set: &ObjectSet,
    ) -> (
        BTreeMap<ObjectKey, Object>,
        BTreeMap<ObjectID, SequenceNumber>,
    ) {
        let mut objects = BTreeMap::new();
        let mut latest: BTreeMap<ObjectID, SequenceNumber> = BTreeMap::new();
        for object in object_set.iter() {
            let id = object.id();
            let version = object.version();
            latest
                .entry(id)
                .and_modify(|v| {
                    if version > *v {
                        *v = version;
                    }
                })
                .or_insert(version);
            objects.insert(ObjectKey(id, version), object.clone());
        }
        (objects, latest)
    }

    pub fn new(
        objects: Arc<BTreeMap<ObjectKey, Object>>,
        latest: Arc<BTreeMap<ObjectID, SequenceNumber>>,
        tombstones: Arc<BTreeMap<ObjectID, BTreeSet<SequenceNumber>>>,
        packages: Arc<PackageCache>,
        system_packages: Arc<BTreeMap<ObjectID, Object>>,
    ) -> Self {
        Self {
            objects,
            latest,
            tombstones,
            packages,
            system_packages,
        }
    }
}

impl ObjectStore for ScanStore {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        let version = *self.latest.get(object_id)?;
        self.objects.get(&ObjectKey(*object_id, version)).cloned()
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: VersionNumber) -> Option<Object> {
        self.objects.get(&ObjectKey(*object_id, version)).cloned()
    }
}

impl BackingPackageStore for ScanStore {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        // System packages are versioned per epoch (stable id, different bytecode per protocol
        // version), so serve the epoch-correct snapshot copy rather than the fullnode's latest. Fall
        // through if absent (e.g. a system package not yet published at this protocol version).
        if is_system_package(*package_id)
            && let Some(object) = self.system_packages.get(package_id)
        {
            return Ok(Some(PackageObject::new(object.clone())));
        }
        // Prefer a package already present in the checkpoint's object set.
        if let Some(object) = self.get_object(package_id)
            && object.is_package()
        {
            return Ok(Some(PackageObject::new(object)));
        }
        Ok(self.packages.fetch(*package_id).map(PackageObject::new))
    }
}

impl RuntimeObjectResolver for ScanStore {
    fn read_child_object(
        &self,
        _parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        // Return the child at its root version, respecting within-checkpoint deletions. Does not
        // re-verify `parent` ownership (mirrors sui-replay-2: these are reconstructions of
        // already-validated executions).
        Ok(child_at_bound(
            &self.objects,
            &self.tombstones,
            *child,
            child_version_upper_bound,
        ))
    }

    fn get_object_received_at_version(
        &self,
        owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        // Per the trait contract every failure mode (absent / wrong version / wrong owner) is
        // `Ok(None)`, never an error.
        Ok(received_object(
            &self.objects,
            *owner,
            *receiving_object_id,
            receive_object_at_version,
        ))
    }
}

impl ParentSync for ScanStore {
    fn get_latest_parent_entry_ref_deprecated(&self, _object_id: ObjectID) -> Option<ObjectRef> {
        None
    }
}

/// The child object as of root version `bound`: the greatest live version `<= bound` in the index,
/// **unless** the child was removed (tombstoned) more recently than that live version. The bundled
/// object set carries only live object versions (no tombstones), so a child that was deleted within
/// the checkpoint still has a stale `<= bound` version present; consulting `tombstones` lets us
/// return `None` for it, matching on-chain liveness (and correctly returning the object again if it
/// was re-created after deletion at a version `<= bound`).
fn child_at_bound(
    objects: &BTreeMap<ObjectKey, Object>,
    tombstones: &BTreeMap<ObjectID, BTreeSet<SequenceNumber>>,
    id: ObjectID,
    bound: SequenceNumber,
) -> Option<Object> {
    let (live_key, obj) = objects
        .range(ObjectKey(id, SequenceNumber::from_u64(0))..=ObjectKey(id, bound))
        .next_back()?;
    // A tombstone version is the (new) version assigned when the object was removed, so it is
    // greater than the object's last live version. If the most recent tombstone `<= bound` is newer
    // than the most recent live version `<= bound`, the object is dead at `bound`.
    if let Some(tombs) = tombstones.get(&id)
        && tombs
            .range(..=bound)
            .next_back()
            .is_some_and(|&t| t > live_key.1)
    {
        return None;
    }
    Some(obj.clone())
}

/// The object at *exactly* `version`, address-owned by `owner` — the `Receiving` contract. Any
/// failure (absent / wrong version / wrong owner) yields `None`.
fn received_object(
    objects: &BTreeMap<ObjectKey, Object>,
    owner: ObjectID,
    id: ObjectID,
    version: SequenceNumber,
) -> Option<Object> {
    let obj = objects.get(&ObjectKey(id, version))?.clone();
    (obj.owner == Owner::AddressOwner(owner.into())).then_some(obj)
}

/// Resolve the transaction's declared input object kinds against `store` into [`InputObjects`],
/// mirroring `sui-replay-2`'s `get_input_objects_for_replay`. Returns an error if a declared input
/// object is not present in the checkpoint's object set.
///
/// `effects` supplies the per-transaction *consensus* (shared) object versions: a shared object's
/// declared input kind carries only its `initial_shared_version`, but each transaction reads it at
/// the specific version consensus assigned — and a hot shared object (e.g. a DeFi pool) is mutated
/// many times within one checkpoint, so the checkpoint-wide latest version is the wrong state for
/// all but the last reader. Owned/imm inputs already carry their exact version in the input ref.
pub fn resolve_input_objects(
    txn_data: &TransactionData,
    effects: &TransactionEffects,
    store: &ScanStore,
) -> anyhow::Result<InputObjects> {
    let consensus_inputs = effects.input_consensus_objects();
    // Per-tx version for the inputs we resolve against — only plain mutate/read-only consensus
    // inputs are materialized at a specific version in the checkpoint. Other kinds (cancelled,
    // ended consensus streams) carry no live version and fall through to the latest-version lookup,
    // which is expected to miss; `shared_kind` surfaces which in the error.
    let shared_versions: BTreeMap<ObjectID, SequenceNumber> = consensus_inputs
        .iter()
        .filter_map(|ico| match ico {
            InputConsensusObject::Mutate((id, version, _))
            | InputConsensusObject::ReadOnly((id, version, _)) => Some((*id, *version)),
            _ => None,
        })
        .collect();
    let mut resolved = Vec::new();
    for kind in txn_data.input_objects()? {
        match kind {
            InputObjectKind::MovePackage(id) => {
                let object = store
                    .get_package_object(&id)
                    .map_err(|e| anyhow::anyhow!("fetching package {id}: {e}"))?
                    .with_context(|| format!("package {id} not found"))?
                    .object()
                    .clone();
                resolved.push(ObjectReadResult {
                    input_object_kind: kind,
                    object: ObjectReadResultKind::Object(object),
                });
            }
            InputObjectKind::ImmOrOwnedMoveObject((id, version, _digest)) => {
                let object = store.get_object_by_key(&id, version).with_context(|| {
                    format!("imm-or-owned input object {id} v{version} not in object set")
                })?;
                resolved.push(ObjectReadResult {
                    input_object_kind: InputObjectKind::ImmOrOwnedMoveObject(
                        object.compute_object_reference(),
                    ),
                    object: ObjectReadResultKind::Object(object),
                });
            }
            InputObjectKind::SharedMoveObject { id, .. } => {
                // Prefer the per-tx assigned version from effects; fall back to latest only if the
                // object isn't in the consensus-input records (e.g. cancelled/stream-ended edges).
                let object = match shared_versions.get(&id) {
                    Some(&version) => store.get_object_by_key(&id, version).with_context(|| {
                        format!(
                            "{} shared input object {id} v{version} not in object set",
                            shared_kind(&consensus_inputs, id)
                        )
                    })?,
                    None => store.get_object(&id).with_context(|| {
                        format!(
                            "shared input object {id} ({}) not in object set",
                            shared_kind(&consensus_inputs, id)
                        )
                    })?,
                };
                resolved.push(ObjectReadResult {
                    input_object_kind: kind,
                    object: ObjectReadResultKind::Object(object),
                });
            }
        }
    }
    Ok(InputObjects::new(resolved))
}

/// The consensus access kind of shared input `id` (for the "not in object set" diagnostic). Scans
/// the (small) consensus-input list, called only when building an error message.
fn shared_kind(consensus_inputs: &[InputConsensusObject], id: ObjectID) -> &'static str {
    consensus_inputs
        .iter()
        .find(|ico| ico.id_and_version().0 == id)
        .map_or("absent", |ico| match ico {
            InputConsensusObject::Mutate(_) => "mutate",
            InputConsensusObject::ReadOnly(_) => "read-only",
            InputConsensusObject::ReadConsensusStreamEnded(..) => "read-stream-ended",
            InputConsensusObject::MutateConsensusStreamEnded(..) => "mutate-stream-ended",
            InputConsensusObject::Cancelled(..) => "cancelled",
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_types::base_types::{ObjectID, SuiAddress};
    use sui_types::object::{Object, Owner};

    fn obj(id: ObjectID, version: u64) -> Object {
        Object::with_id_owner_version_for_testing(
            id,
            SequenceNumber::from_u64(version),
            Owner::AddressOwner(SuiAddress::ZERO),
        )
    }

    fn index(objs: Vec<Object>) -> BTreeMap<ObjectKey, Object> {
        objs.into_iter()
            .map(|o| (ObjectKey(o.id(), o.version()), o))
            .collect()
    }

    /// A checkpoint object set can contain several versions of the same object (when an earlier
    /// transaction in the checkpoint mutated it). The index must key every (id, version) pair and
    /// track the highest version per id for version-less `get_object`.
    #[test]
    fn index_object_set_tracks_latest_version_per_id() {
        let id = ObjectID::random();
        let other = ObjectID::random();

        let mut set = ObjectSet::default();
        set.insert(obj(id, 3));
        set.insert(obj(id, 7));
        set.insert(obj(other, 5));

        let (objects, latest) = ScanStore::index_object_set(&set);

        assert_eq!(latest.get(&id), Some(&SequenceNumber::from_u64(7)));
        assert_eq!(latest.get(&other), Some(&SequenceNumber::from_u64(5)));
        // Every distinct (id, version) pair is indexed, including the superseded version.
        assert_eq!(objects.len(), 3);
        assert!(objects.contains_key(&ObjectKey(id, SequenceNumber::from_u64(3))));
        assert!(objects.contains_key(&ObjectKey(id, SequenceNumber::from_u64(7))));
    }

    /// A dynamic-field child read resolves to the greatest version `<= bound` (its "root version"),
    /// and is absent when the bound is below every known version or the id is unknown.
    #[test]
    fn read_child_object_returns_root_version() {
        let child = ObjectID::random();
        let objects = index(vec![obj(child, 3), obj(child, 7)]);
        let no_tombs = BTreeMap::new();
        let sv = SequenceNumber::from_u64;

        let at = |b| child_at_bound(&objects, &no_tombs, child, sv(b));
        assert_eq!(at(7).unwrap().version(), sv(7));
        assert_eq!(at(5).unwrap().version(), sv(3));
        assert_eq!(at(9).unwrap().version(), sv(7));
        assert!(at(2).is_none());
        assert!(child_at_bound(&objects, &no_tombs, ObjectID::random(), sv(100)).is_none());
    }

    /// A child removed within the checkpoint must read as absent at/after the tombstone version,
    /// even though the object set still carries its pre-deletion version — and read as present
    /// again if it was re-created afterward.
    #[test]
    fn read_child_object_respects_tombstones() {
        let child = ObjectID::random();
        // live v3, deleted (tombstone v5), re-created v8.
        let objects = index(vec![obj(child, 3), obj(child, 8)]);
        let tombs: BTreeMap<ObjectID, BTreeSet<SequenceNumber>> =
            [(child, [SequenceNumber::from_u64(5)].into_iter().collect())]
                .into_iter()
                .collect();
        let sv = SequenceNumber::from_u64;
        let at = |b| child_at_bound(&objects, &tombs, child, sv(b));

        assert_eq!(at(3).unwrap().version(), sv(3)); // before deletion: live v3
        assert!(at(5).is_none()); // at tombstone: deleted
        assert!(at(7).is_none()); // after deletion, before re-creation: still gone
        assert_eq!(at(8).unwrap().version(), sv(8)); // re-created: live again
        assert_eq!(at(9).unwrap().version(), sv(8));
    }

    /// A `Receiving` read needs the *exact* version and an `AddressOwner` matching `owner`; every
    /// other case (wrong version, wrong owner) is `None`.
    #[test]
    fn received_object_requires_exact_version_and_owner() {
        let id = ObjectID::random();
        let owner = ObjectID::random();
        let recv = Object::with_id_owner_version_for_testing(
            id,
            SequenceNumber::from_u64(5),
            Owner::AddressOwner(owner.into()),
        );
        let objects = index(vec![recv]);
        let sv = SequenceNumber::from_u64;

        assert!(received_object(&objects, owner, id, sv(5)).is_some());
        assert!(received_object(&objects, owner, id, sv(4)).is_none());
        assert!(received_object(&objects, ObjectID::random(), id, sv(5)).is_none());
    }
}
