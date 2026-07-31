// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Execution-time object resolution within one finalized checkpoint snapshot.
//!
//! Replay executes recorded transaction prestate, while dry-run executes checkpoint-end state.
//! Reusing `ReplayStore` could mix those views because its exact-version reads are not checkpoint
//! anchored. This store binds every source read to one checkpoint and retains one live body per ID.
//! Synthetic inputs such as mock gas remain separate from checkpoint state.
//!
//! Here, a child is Sui's storage term for an object owned by another object.
//! `read_child_object` is the storage primitive used by the dynamic-field APIs to load those
//! objects.
//!
//! Child reads also carry a version bound, so checkpoint-end state is not a general answer for
//! historical roots. For example, if a child existed at version 7 and was deleted at version 20, a
//! read bounded at version 10 must return version 7 even though the child is absent at checkpoint
//! end.
//!
//! A root is the top-level transaction object through which Move reaches a child. Move passes the
//! root's version as the upper bound for reads of that child and any nested children.
//!
//! This dry-run can execute with the following roots:
//!
//! - Shared or consensus-owned inputs are loaded at their latest version in the checkpoint.
//! - Address-owned inputs are loaded at the submitted version and then required to match their
//!   latest checkpoint version.
//! - Immutable inputs cannot acquire a newer version.
//! - Objects created during this execution have no checkpoint history. Move uses a zero bound for
//!   their child reads.
//! - Reads that explicitly request the latest state use an unbounded `MAX` value.
//!
//! Object-owned objects cannot be top-level transaction inputs, and receiving inputs are rejected.
//! Any earlier transaction that mutated or removed a child also advanced its mutable top-level
//! root. Move carries that checkpoint-current root version into nested child reads.
//!
//! Consequently, every bound accepted here describes current checkpoint state, a new object with no
//! prior children, or an explicitly unbounded read. In each case, `None` from the checkpoint store
//! is valid absence. Supporting stale inputs, receiving objects, or general historical reads would
//! require a store that can resolve child state at an arbitrary older bound.
//!
//! `ObjectStore` cannot return source-store errors, so failed reads temporarily return `None` and latch
//! the error. The caller rejects any latched error after execution; otherwise execution could
//! accept a failed read as legitimate absence.

use super::input_loader::unsupported_error;
use anyhow::Result;
use std::{cell::RefCell, collections::BTreeMap};
use sui_data_store::{
    CheckpointExecutionContext, CheckpointObjectRequest, CheckpointObjectSelector,
    CheckpointObjectStore,
};
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, VersionNumber},
    committee::EpochId,
    error::{SuiError, SuiErrorKind, SuiResult},
    object::{Object, Owner},
    storage::{
        BackingPackageStore, ObjectStore, PackageObject, ParentSync, RuntimeObjectResolver,
        load_package_object_from_object_store,
    },
};
use tracing::trace;

/// One checkpoint-end body per object ID.
type CheckpointObjectCache = BTreeMap<ObjectID, Object>;
/// Object bodies arranged in the format consumed by replay artifact helpers.
type ObjectCache = BTreeMap<ObjectID, BTreeMap<u64, Object>>;

/// A `BackingStore` whose source reads and memoized bodies are bound to one checkpoint.
pub(super) struct CheckpointRuntimeStore<'a> {
    /// Chain, checkpoint, and epoch context shared by every source read.
    snapshot: &'a CheckpointExecutionContext,
    /// Source responsible for returning validated, checkpoint-qualified answers.
    store: &'a dyn CheckpointObjectStore,
    /// Execution-local checkpoint-end body for each observed object ID.
    checkpoint_objects: RefCell<CheckpointObjectCache>,
    /// Synthetic objects retained for artifacts without establishing checkpoint state.
    local_objects: Vec<Object>,
    /// First execution-storage error latched for post-execution rejection.
    deferred_error: RefCell<Option<String>>,
}

impl<'a> CheckpointRuntimeStore<'a> {
    /// Build the runtime view and retain synthetic local objects separately for artifact export.
    pub(super) fn new(
        snapshot: &'a CheckpointExecutionContext,
        store: &'a dyn CheckpointObjectStore,
        checkpoint_objects: CheckpointObjectCache,
        local_objects: Vec<Object>,
    ) -> Self {
        Self {
            snapshot,
            store,
            checkpoint_objects: RefCell::new(checkpoint_objects),
            local_objects,
            deferred_error: RefCell::new(None),
        }
    }

    /// Take the first latched execution-storage error.
    pub(super) fn take_deferred_error(&self) -> Option<String> {
        self.deferred_error.borrow_mut().take()
    }

    /// Fold checkpoint and local objects into the replay artifact cache format.
    pub(super) fn into_object_cache(self) -> ObjectCache {
        let mut object_cache = BTreeMap::<ObjectID, BTreeMap<u64, Object>>::new();
        for object in self
            .checkpoint_objects
            .into_inner()
            .into_values()
            .chain(self.local_objects)
        {
            object_cache
                .entry(object.id())
                .or_default()
                .insert(object.version().value(), object);
        }
        object_cache
    }

    /// Return an already established checkpoint-end body.
    fn cached_checkpoint_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.checkpoint_objects.borrow().get(object_id).cloned()
    }

    /// Query one selector from the checkpoint source.
    fn fetch_checkpoint_object(
        &self,
        object_id: ObjectID,
        selector: CheckpointObjectSelector,
    ) -> Result<Option<Object>> {
        let request = CheckpointObjectRequest {
            object_id,
            selector,
        };
        let objects = self
            .store
            .get_checkpoint_objects(self.snapshot, &[request])?;
        Ok(objects.into_iter().next().flatten())
    }

    /// Return the latest checkpoint body, loading and retaining it when needed.
    fn latest_checkpoint_object(&self, object_id: ObjectID) -> Result<Option<Object>> {
        if let Some(object) = self.cached_checkpoint_object(&object_id) {
            return Ok(Some(object));
        }

        let object = self.fetch_checkpoint_object(object_id, CheckpointObjectSelector::Latest)?;
        if let Some(object) = &object {
            self.checkpoint_objects
                .borrow_mut()
                .insert(object.id(), object.clone());
        }
        Ok(object)
    }

    /// Resolve an exact runtime read only when it names the checkpoint-end body.
    fn exact_checkpoint_object(
        &self,
        object_id: ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>> {
        let Some(object) = self.latest_checkpoint_object(object_id)? else {
            return Ok(None);
        };
        if object.version() != version {
            return Err(unsupported_error("runtime exact version is not live"));
        }
        Ok(Some(object))
    }

    /// Latch a runtime-store failure and return it through a fallible storage interface.
    fn defer_runtime_error(&self, error: anyhow::Error) -> SuiError {
        let message = error.to_string();
        self.defer_error(message.clone());
        SuiErrorKind::Storage(message).into()
    }

    /// Latch the first error so later failures cannot overwrite its root cause.
    fn defer_error(&self, error: String) {
        let mut deferred_error = self.deferred_error.borrow_mut();
        if deferred_error.is_none() {
            *deferred_error = Some(error);
        }
    }

    /// Convert and latch inconsistent dynamic-child state as a storage error.
    fn child_error(&self, message: String) -> SuiError {
        self.defer_error(message.clone());
        SuiErrorKind::Storage(message).into()
    }

    /// Require a dynamically loaded child to be owned by the requested parent.
    fn validate_child_owner(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        object: &Object,
    ) -> SuiResult<()> {
        let expected_owner = Owner::ObjectOwner((*parent).into());
        if object.owner == expected_owner {
            return Ok(());
        }

        let error = SuiErrorKind::InvalidChildObjectAccess {
            object: *child,
            given_parent: *parent,
            actual_owner: object.owner.clone(),
        };
        self.defer_error(error.to_string());
        Err(error.into())
    }
}

impl BackingPackageStore for CheckpointRuntimeStore<'_> {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        trace!("get_package_object({package_id})");
        load_package_object_from_object_store(self, package_id)
    }
}

impl ObjectStore for CheckpointRuntimeStore<'_> {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        trace!("get_object({object_id})");
        self.latest_checkpoint_object(*object_id)
            .unwrap_or_else(|error| {
                self.defer_runtime_error(error);
                None
            })
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: VersionNumber) -> Option<Object> {
        trace!("get_object_by_key({object_id}, {version})");
        self.exact_checkpoint_object(*object_id, version)
            .unwrap_or_else(|error| {
                self.defer_runtime_error(error);
                None
            })
    }
}

impl RuntimeObjectResolver for CheckpointRuntimeStore<'_> {
    /// Resolve an object-owned child using state at the selected checkpoint.
    ///
    /// The supported bounds are a root's current checkpoint version, zero for a newly created root,
    /// or `MAX` for an unbounded read. In each case, no child at checkpoint end means no child for
    /// this read. A live child newer than a finite bound indicates stale or mixed state and is
    /// rejected.
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        trace!("read_child_object({parent}, {child}, {child_version_upper_bound})");
        let object = self
            .latest_checkpoint_object(*child)
            .map_err(|error| self.defer_runtime_error(error))?;
        let Some(object) = object else {
            return Ok(None);
        };
        if child_version_upper_bound != SequenceNumber::MAX
            && object.version() > child_version_upper_bound
        {
            return Err(self.child_error("live child version exceeds parent bound".to_string()));
        }
        self.validate_child_owner(parent, child, &object)?;
        Ok(Some(object))
    }

    /// Reject receiving-object resolution until checkpoint marker state is available.
    fn get_object_received_at_version(
        &self,
        _owner: &ObjectID,
        _receiving_object_id: &ObjectID,
        _receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        let message = "local checkpoint dry-run does not support receiving objects".to_string();
        self.defer_error(message.clone());
        Err(SuiErrorKind::UnsupportedFeatureError { error: message }.into())
    }
}

impl ParentSync for CheckpointRuntimeStore<'_> {
    fn get_latest_parent_entry_ref_deprecated(&self, _object_id: ObjectID) -> Option<ObjectRef> {
        unreachable!("deprecated parent reads are rejected during preparation")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_data_store::EpochData;
    use sui_types::{
        base_types::SuiAddress,
        digests::{ChainIdentifier, CheckpointDigest},
    };

    struct CheckpointSource {
        context: CheckpointExecutionContext,
        responses: Vec<(CheckpointObjectRequest, Option<Object>)>,
        fail_selector: Option<CheckpointObjectSelector>,
        calls: RefCell<Vec<CheckpointObjectRequest>>,
    }

    impl CheckpointSource {
        fn new(context: CheckpointExecutionContext) -> Self {
            Self {
                context,
                responses: Vec::new(),
                fail_selector: None,
                calls: RefCell::new(Vec::new()),
            }
        }

        fn insert(
            &mut self,
            object_id: ObjectID,
            selector: CheckpointObjectSelector,
            object: Option<Object>,
        ) {
            self.responses.push((
                CheckpointObjectRequest {
                    object_id,
                    selector,
                },
                object,
            ));
        }
    }

    impl CheckpointObjectStore for CheckpointSource {
        fn get_checkpoint_objects(
            &self,
            context: &CheckpointExecutionContext,
            requests: &[CheckpointObjectRequest],
        ) -> Result<Vec<Option<Object>>> {
            assert_eq!(context, &self.context);
            self.calls.borrow_mut().extend_from_slice(requests);
            if requests
                .iter()
                .any(|request| Some(request.selector) == self.fail_selector)
            {
                anyhow::bail!("injected checkpoint source failure");
            }
            Ok(requests
                .iter()
                .map(|request| {
                    self.responses
                        .iter()
                        .find(|(candidate, _)| candidate == request)
                        .and_then(|(_, object)| object.clone())
                })
                .collect())
        }
    }

    fn context() -> CheckpointExecutionContext {
        CheckpointExecutionContext {
            chain_identifier: ChainIdentifier::random(),
            checkpoint: 17,
            checkpoint_digest: CheckpointDigest::random(),
            epoch: EpochData {
                epoch_id: 3,
                protocol_version: 42,
                rgp: 1_000,
                start_timestamp: 123,
            },
        }
    }

    fn object(object_id: ObjectID, version: u64, owner: Owner) -> Object {
        Object::with_id_owner_version_for_testing(
            object_id,
            SequenceNumber::from_u64(version),
            owner,
        )
    }

    fn checkpoint_objects(objects: impl IntoIterator<Item = Object>) -> CheckpointObjectCache {
        objects
            .into_iter()
            .map(|object| (object.id(), object))
            .collect()
    }

    /// A prepared body serves compatible latest and exact reads without another source query.
    #[test]
    fn prepared_checkpoint_body_satisfies_latest_and_matching_exact() {
        let context = context();
        let object_id = ObjectID::random();
        let current = object(
            object_id,
            2,
            Owner::AddressOwner(SuiAddress::random_for_testing_only()),
        );
        let source = CheckpointSource::new(context.clone());
        let store = CheckpointRuntimeStore::new(
            &context,
            &source,
            checkpoint_objects([current.clone()]),
            vec![],
        );

        assert_eq!(
            ObjectStore::get_object(&store, &object_id),
            Some(current.clone())
        );
        assert_eq!(
            ObjectStore::get_object_by_key(&store, &object_id, SequenceNumber::from_u64(2)),
            Some(current),
        );
        assert!(source.calls.borrow().is_empty());
    }

    /// A cold exact read cannot expose historical state as checkpoint-end state.
    #[test]
    fn cold_exact_read_rejects_a_historical_version() {
        let context = context();
        let object_id = ObjectID::random();
        let latest = object(
            object_id,
            2,
            Owner::AddressOwner(SuiAddress::random_for_testing_only()),
        );
        let mut source = CheckpointSource::new(context.clone());
        source.insert(object_id, CheckpointObjectSelector::Latest, Some(latest));
        let store = CheckpointRuntimeStore::new(&context, &source, BTreeMap::new(), vec![]);

        assert_eq!(
            ObjectStore::get_object_by_key(&store, &object_id, SequenceNumber::from_u64(1)),
            None,
        );
        assert!(store.take_deferred_error().is_some());
        assert_eq!(source.calls.borrow().len(), 1);
        assert_eq!(
            source.calls.borrow()[0].selector,
            CheckpointObjectSelector::Latest,
        );
    }

    /// A synthetic local body is exported for artifacts but cannot satisfy checkpoint reads.
    #[test]
    fn local_body_is_not_checkpoint_state() {
        let context = context();
        let object_id = ObjectID::random();
        let local = object(
            object_id,
            1,
            Owner::AddressOwner(SuiAddress::random_for_testing_only()),
        );
        let source = CheckpointSource::new(context.clone());
        let store =
            CheckpointRuntimeStore::new(&context, &source, BTreeMap::new(), vec![local.clone()]);

        assert_eq!(
            ObjectStore::get_object_by_key(&store, &object_id, SequenceNumber::from_u64(1)),
            None,
        );
        assert_eq!(source.calls.borrow().len(), 1);
        assert!(store.take_deferred_error().is_none());
        assert_eq!(store.into_object_cache()[&object_id][&1], local);
    }

    /// An error hidden by `ObjectStore`'s infallible API is latched for outer rejection.
    #[test]
    fn source_error_is_deferred_from_infallible_read() {
        let context = context();
        let object_id = ObjectID::random();
        let mut source = CheckpointSource::new(context.clone());
        source.fail_selector = Some(CheckpointObjectSelector::Latest);
        let store = CheckpointRuntimeStore::new(&context, &source, BTreeMap::new(), vec![]);

        assert_eq!(ObjectStore::get_object(&store, &object_id), None);
        assert!(
            store
                .take_deferred_error()
                .unwrap()
                .to_string()
                .contains("injected checkpoint source failure")
        );
    }

    /// A definitive source miss remains ordinary absence and does not poison execution.
    #[test]
    fn definitive_absence_is_not_deferred() {
        let context = context();
        let source = CheckpointSource::new(context.clone());
        let store = CheckpointRuntimeStore::new(&context, &source, BTreeMap::new(), vec![]);

        assert_eq!(ObjectStore::get_object(&store, &ObjectID::random()), None);
        assert!(store.take_deferred_error().is_none());
    }

    /// An absent child under a current finite root bound is ordinary absence, not a deferred error.
    #[test]
    fn current_root_child_absence_is_not_deferred() {
        let context = context();
        let parent = object(
            ObjectID::random(),
            3,
            Owner::AddressOwner(SuiAddress::random_for_testing_only()),
        );
        let child = ObjectID::random();
        let source = CheckpointSource::new(context.clone());
        let store = CheckpointRuntimeStore::new(
            &context,
            &source,
            checkpoint_objects([parent.clone()]),
            vec![],
        );

        assert_eq!(
            store
                .read_child_object(&parent.id(), &child, parent.version())
                .unwrap(),
            None,
        );
        assert!(store.take_deferred_error().is_none());
        assert_eq!(
            source.calls.borrow().as_slice(),
            &[CheckpointObjectRequest {
                object_id: child,
                selector: CheckpointObjectSelector::Latest,
            }]
        );
    }

    /// A finite child read reuses the live checkpoint body when it is within the parent bound.
    #[test]
    fn finite_child_uses_latest_checkpoint_body() {
        let context = context();
        let parent = ObjectID::random();
        let child = object(ObjectID::random(), 3, Owner::ObjectOwner(parent.into()));
        let mut source = CheckpointSource::new(context.clone());
        source.insert(
            child.id(),
            CheckpointObjectSelector::Latest,
            Some(child.clone()),
        );
        let store = CheckpointRuntimeStore::new(&context, &source, BTreeMap::new(), vec![]);

        assert_eq!(
            store
                .read_child_object(&parent, &child.id(), SequenceNumber::from_u64(4))
                .unwrap(),
            Some(child.clone()),
        );
        assert_eq!(ObjectStore::get_object(&store, &child.id()), Some(child));
        assert_eq!(source.calls.borrow().len(), 1);
    }

    /// A child newer than its finite parent bound cannot be exposed to execution.
    #[test]
    fn finite_child_rejects_latest_above_parent_bound() {
        let context = context();
        let parent = ObjectID::random();
        let child_id = ObjectID::random();
        let latest = object(child_id, 4, Owner::ObjectOwner(parent.into()));
        let mut source = CheckpointSource::new(context.clone());
        source.insert(child_id, CheckpointObjectSelector::Latest, Some(latest));
        let store = CheckpointRuntimeStore::new(&context, &source, BTreeMap::new(), vec![]);

        assert!(
            store
                .read_child_object(&parent, &child_id, SequenceNumber::from_u64(3))
                .is_err()
        );
        assert!(store.take_deferred_error().is_some());
    }

    /// An unbounded child read uses the latest checkpoint body.
    #[test]
    fn max_child_bound_uses_latest_checkpoint_state() {
        let context = context();
        let parent = ObjectID::random();
        let child = object(ObjectID::random(), 3, Owner::ObjectOwner(parent.into()));
        let mut source = CheckpointSource::new(context.clone());
        source.insert(
            child.id(),
            CheckpointObjectSelector::Latest,
            Some(child.clone()),
        );
        let store = CheckpointRuntimeStore::new(&context, &source, BTreeMap::new(), vec![]);

        assert_eq!(
            store
                .read_child_object(&parent, &child.id(), SequenceNumber::MAX)
                .unwrap(),
            Some(child),
        );
        assert_eq!(source.calls.borrow().len(), 1);
        assert_eq!(
            source.calls.borrow()[0].selector,
            CheckpointObjectSelector::Latest,
        );
    }

    /// A dynamic child is returned only for its actual object-owning parent.
    #[test]
    fn child_owner_is_validated() {
        let context = context();
        let expected_parent = ObjectID::random();
        let actual_parent = ObjectID::random();
        let child = object(
            ObjectID::random(),
            3,
            Owner::ObjectOwner(actual_parent.into()),
        );
        let mut source = CheckpointSource::new(context.clone());
        source.insert(
            child.id(),
            CheckpointObjectSelector::Latest,
            Some(child.clone()),
        );
        let store = CheckpointRuntimeStore::new(&context, &source, BTreeMap::new(), vec![]);

        assert!(
            store
                .read_child_object(&expected_parent, &child.id(), SequenceNumber::from_u64(3),)
                .is_err()
        );
        assert_eq!(source.calls.borrow().len(), 1);
        assert!(store.take_deferred_error().is_some());
    }
}
