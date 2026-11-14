// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::reader::indexes_disabled;
use crate::{ErrorReason, Result, RpcError, RpcService};
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::{
    ListSystemPackagesRequest, ListSystemPackagesResponse, PackageVersion,
};
use sui_types::{SYSTEM_PACKAGE_ADDRESSES, base_types::ObjectID, storage::RpcIndexes};

/// List the system packages that existed as of the end of `at_checkpoint`, each at its latest
/// version as of that checkpoint, ordered by package id.
///
/// The answer is the complete set or an error: `at_checkpoint` is checked against the shared
/// checkpoint-pinned read bounds, the upper bound before the reads and the lower bound after
/// them (see `StateReader::at_checkpoint_upper_bound`), so a checkpoint outside the indexed range
/// fails the whole request.
///
/// The candidates are the system package ids compiled into this binary. A fullnode cannot execute
/// past a protocol upgrade it does not know, so every system package in the data it serves is in
/// its compiled list. Only the rpc indexes are read, never object contents, so nodes that keep
/// only each object's latest version answer too.
#[tracing::instrument(skip(service))]
pub fn list_system_packages(
    service: &RpcService,
    request: ListSystemPackagesRequest,
) -> Result<ListSystemPackagesResponse> {
    let at_checkpoint = request.at_checkpoint.ok_or_else(|| {
        FieldViolation::new("at_checkpoint")
            .with_description("missing at_checkpoint")
            .with_reason(ErrorReason::FieldMissing)
    })?;

    service
        .reader
        .at_checkpoint_upper_bound()?
        .check(at_checkpoint)?;

    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(indexes_disabled)?;

    let system_packages = system_package_versions(indexes, at_checkpoint);

    service
        .reader
        .at_checkpoint_lower_bound()?
        .check(at_checkpoint)?;
    system_packages.map(ListSystemPackagesResponse::new)
}

/// The version of each compiled system package as of the end of `at_checkpoint`, ordered by
/// package id, omitting packages that did not exist yet. Each version carries the checkpoint it
/// was published in, unset when the indexes' history does not reach back to it.
fn system_package_versions(
    indexes: &dyn RpcIndexes,
    at_checkpoint: u64,
) -> Result<Vec<PackageVersion>> {
    let mut package_ids: Vec<ObjectID> = SYSTEM_PACKAGE_ADDRESSES
        .iter()
        .copied()
        .map(ObjectID::from)
        .collect();
    package_ids.sort();

    let mut system_packages = Vec::with_capacity(package_ids.len());
    for package_id in package_ids {
        // System packages are never deleted or wrapped, so a resolved version is never a
        // tombstone: it is the live package.
        let Some(version) = indexes
            .get_object_version_at_checkpoint(package_id, at_checkpoint)
            .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
        else {
            continue;
        };
        let version = version.value();

        // A system package is its own original package, so its versions are listed under its id.
        let published = indexes
            .package_versions_iter(package_id, version, version.saturating_add(1), false)
            .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
            .next()
            .transpose()
            .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
            .and_then(|entry| entry.checkpoint);

        let mut entry = PackageVersion::new(&package_id.into(), version)
            .with_original_id(package_id.to_canonical_string(true));
        entry.checkpoint = published;
        system_packages.push(entry);
    }
    Ok(system_packages)
}
