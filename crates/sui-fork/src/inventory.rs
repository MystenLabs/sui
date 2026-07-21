// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Lazy inventory initialization for the fork's secondary indexes.
//!
//! An *inventory* is the one-time, full remote enumeration of every object
//! owned by an address / owned by an object / matching a type at the fork
//! checkpoint. Running one backfills `sui-rpc-store`'s `object_by_owner` /
//! `object_by_type` index column families and records a per-owner (or
//! per-type) completion marker in the fork metadata sidecar, so later reads
//! serve straight from the local index instead of re-scanning the remote.

use std::sync::Arc;
use std::sync::RwLock;
use std::sync::RwLockWriteGuard;

use anyhow::Context as _;
use anyhow::anyhow;
use anyhow::bail;

use move_core_types::language_storage::StructTag;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::coin::CoinMetadata;
use sui_types::coin::RegulatedCoinMetadata;
use sui_types::coin::TreasuryCap;

use crate::fork_rpc_store::ForkRpcStore;
use crate::metadata::ForkMetadataStore;
use crate::remote::RemoteSource;

/// Runs inventory scans on first use and marks them complete.
///
/// Holds the same snapshot lock as `DataStore`'s local object writes, so an
/// inventory scan never interleaves with a local execution's raw-object and
/// live-state writes (derived index rows are the embedded indexer's job), and
/// concurrent scans of the same owner run once.
pub(crate) struct InventoryInitializer {
    remote: RemoteSource,
    metadata: ForkMetadataStore,
    rpc_store: ForkRpcStore,
    snapshot_lock: Arc<RwLock<()>>,
}

impl InventoryInitializer {
    pub(crate) fn new(
        remote: RemoteSource,
        metadata: ForkMetadataStore,
        rpc_store: ForkRpcStore,
        snapshot_lock: Arc<RwLock<()>>,
    ) -> Self {
        Self {
            remote,
            metadata,
            rpc_store,
            snapshot_lock,
        }
    }

    fn lock_snapshot(&self) -> anyhow::Result<RwLockWriteGuard<'_, ()>> {
        self.snapshot_lock
            .write()
            .map_err(|_| anyhow!("local snapshot lock poisoned"))
    }

    /// Lazily populate the address-owner index for `owner` from a full remote
    /// owned-objects scan at the fork checkpoint, then mark it complete so
    /// later reads hit the local rpc-store index. Idempotent: a completed
    /// owner (including one that legitimately owns nothing) is skipped. The
    /// snapshot guard serializes this initialization across cloned stores, and
    /// the marker is re-checked under the guard to avoid a duplicate scan.
    pub(crate) fn ensure_address_owner(&self, owner: SuiAddress) -> anyhow::Result<()> {
        if self.metadata.address_owner_inventory_complete(owner)? {
            return Ok(());
        }

        let _snapshot_guard = self.lock_snapshot()?;
        if self.metadata.address_owner_inventory_complete(owner)? {
            return Ok(());
        }

        let refs = self.remote.scan_address_owned(owner).with_context(|| {
            format!(
                "failed to initialize address-owned index for {owner} at checkpoint {}",
                self.remote.forked_at_checkpoint(),
            )
        })?;
        if refs.is_empty() {
            return self.metadata.mark_address_owner_inventory_complete(owner);
        }

        let object_refs: Vec<_> = refs.iter().map(|entry| entry.object_ref).collect();
        let objects = self
            .remote
            .objects_at_fork(&object_refs, "address-owned objects")?;
        for object in objects {
            self.rpc_store
                .save_address_owner_inventory_object(owner, &object)?;
        }

        self.metadata.mark_address_owner_inventory_complete(owner)
    }

    /// Lazily populate the object-owner index for children of `parent`; same
    /// contract as [`Self::ensure_address_owner`].
    pub(crate) fn ensure_object_owner(&self, parent: ObjectID) -> anyhow::Result<()> {
        if self.metadata.object_owner_inventory_complete(parent)? {
            return Ok(());
        }

        let _snapshot_guard = self.lock_snapshot()?;
        if self.metadata.object_owner_inventory_complete(parent)? {
            return Ok(());
        }

        let refs = self.remote.scan_object_owned(parent).with_context(|| {
            format!(
                "failed to initialize object-owned index for {parent} at checkpoint {}",
                self.remote.forked_at_checkpoint(),
            )
        })?;
        if refs.is_empty() {
            return self.metadata.mark_object_owner_inventory_complete(parent);
        }

        let object_refs: Vec<_> = refs.iter().map(|entry| entry.object_ref).collect();
        let objects = self
            .remote
            .objects_at_fork(&object_refs, "object-owned objects")?;
        for object in objects {
            self.rpc_store
                .save_object_owner_inventory_object(parent, &object)?;
        }

        self.metadata.mark_object_owner_inventory_complete(parent)
    }

    /// Initialize the type inventories needed to assemble RPC coin metadata.
    pub(crate) fn ensure_coin_info(&self, coin_type: &StructTag) -> anyhow::Result<()> {
        for wrapper_type in [
            CoinMetadata::type_(coin_type.clone()),
            TreasuryCap::type_(coin_type.clone()),
            RegulatedCoinMetadata::type_(coin_type.clone()),
        ] {
            self.ensure_type(&wrapper_type)?;
        }
        Ok(())
    }

    fn ensure_type(&self, object_type: &StructTag) -> anyhow::Result<()> {
        let type_filter = object_type.to_string();
        if self.metadata.type_inventory_complete(&type_filter)? {
            return Ok(());
        }

        let _snapshot_guard = self.lock_snapshot()?;
        if self.metadata.type_inventory_complete(&type_filter)? {
            return Ok(());
        }

        let refs = self
            .remote
            .scan_by_type(type_filter.clone())
            .with_context(|| {
                format!(
                    "failed to initialize type index for {type_filter} at checkpoint {}",
                    self.remote.forked_at_checkpoint(),
                )
            })?;
        if refs.is_empty() {
            return self.metadata.mark_type_inventory_complete(&type_filter);
        }

        let object_refs: Vec<_> = refs.iter().map(|entry| entry.object_ref).collect();
        let objects = self.remote.objects_at_fork(&object_refs, &type_filter)?;
        for object in objects {
            let object_struct_tag = object
                .struct_tag()
                .with_context(|| format!("object {} has no Move struct tag", object.id()))?;
            if object_struct_tag != *object_type {
                bail!(
                    "object {} has type {} but inventory expected {type_filter}",
                    object.id(),
                    object_struct_tag,
                );
            }
            self.rpc_store.save_type_inventory_object(&object)?;
        }

        self.metadata.mark_type_inventory_complete(&type_filter)
    }
}
