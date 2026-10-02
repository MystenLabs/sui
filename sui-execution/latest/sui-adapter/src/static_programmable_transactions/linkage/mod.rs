// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod analysis;
pub mod config;
pub mod resolution;
pub mod resolved_linkage;
pub mod single_linkage;

use crate::{
    data_store::VerifiedPackageStore,
    execution_mode::ExecutionMode,
    execution_value::ExecutionState,
    static_programmable_transactions::{
        linkage::{analysis::LinkageAnalyzer, resolution::MinVersionResolver},
        loading::ast as loading,
    },
};
use sui_protocol_config::ProtocolConfig;
use sui_types::error::ExecutionErrorTrait;

/// Refine the transaction's per-call linkages into a single, unified linkage for the whole
/// transaction (when enabled by the protocol config).
pub fn refine_linkage<Mode: ExecutionMode>(
    mut txn: loading::Transaction,
    linkage_analysis: &LinkageAnalyzer,
    package_store: &VerifiedPackageStore<'_>,
    protocol_config: &ProtocolConfig,
    execution_state: &dyn ExecutionState,
) -> Result<loading::Transaction, Mode::Error> {
    if !protocol_config.enable_unified_linkage() {
        return Ok(txn);
    }

    let minversion_resolver = |original_id| {
        execution_state
            .read_minversion(original_id)
            .map_err(|error| {
                Mode::Error::new_with_source(
                    sui_types::execution_status::ExecutionErrorKind::InvalidLinkage,
                    error,
                )
            })
    };
    let minversion_resolver = protocol_config
        .enable_package_minversion()
        .then_some(&minversion_resolver as &MinVersionResolver<'_, Mode::Error>);
    let policy_targets = single_linkage::refine_to_single_linkage::<Mode::Error>(
        &mut txn,
        linkage_analysis,
        package_store,
        protocol_config,
        minversion_resolver,
    )?;

    if protocol_config.enable_package_version_forbid_list() {
        enforce_forbid_list::<Mode>(&txn, &policy_targets, execution_state)?;
    }

    if protocol_config.enable_package_minversion() {
        enforce_publication_minversions::<Mode>(&policy_targets, execution_state)?;
    }

    Ok(txn)
}

fn enforce_forbid_list<Mode: ExecutionMode>(
    txn: &loading::Transaction,
    policy_targets: &single_linkage::PackagePolicyTargets,
    execution_state: &dyn ExecutionState,
) -> Result<(), Mode::Error> {
    let linkage = txn.unified_linkage.as_ref().ok_or_else(|| {
        Mode::Error::from_kind(sui_types::execution_status::ExecutionErrorKind::InvalidLinkage)
    })?;
    for original_id in &policy_targets.execution_original_ids {
        let Some(package_id) = linkage.0.linkage.get(original_id) else {
            return Err(Mode::Error::from_kind(
                sui_types::execution_status::ExecutionErrorKind::InvalidLinkage,
            ));
        };
        let Some(version) = linkage.0.resolved_version(package_id) else {
            return Err(Mode::Error::from_kind(
                sui_types::execution_status::ExecutionErrorKind::InvalidLinkage,
            ));
        };
        check_forbidden_package_version::<Mode>(*original_id, version, execution_state)?;
    }
    for (original_id, version) in &policy_targets.publication_versions {
        check_forbidden_package_version::<Mode>(*original_id, *version, execution_state)?;
    }
    Ok(())
}

fn check_forbidden_package_version<Mode: ExecutionMode>(
    original_id: sui_types::base_types::ObjectID,
    version: u64,
    execution_state: &dyn ExecutionState,
) -> Result<(), Mode::Error> {
    if execution_state
        .is_package_version_forbidden(original_id, version)
        .map_err(|error| {
            Mode::Error::new_with_source(
                sui_types::execution_status::ExecutionErrorKind::InvalidLinkage,
                error,
            )
        })?
    {
        return Err(Mode::Error::from_kind(
            sui_types::execution_status::ExecutionErrorKind::InvalidLinkage,
        ));
    }
    Ok(())
}

fn enforce_publication_minversions<Mode: ExecutionMode>(
    policy_targets: &single_linkage::PackagePolicyTargets,
    execution_state: &dyn ExecutionState,
) -> Result<(), Mode::Error> {
    for (original_id, version) in &policy_targets.publication_versions {
        let minversion = execution_state
            .read_minversion(*original_id)
            .map_err(|error| {
                Mode::Error::new_with_source(
                    sui_types::execution_status::ExecutionErrorKind::InvalidLinkage,
                    error,
                )
            })?;
        if minversion.is_some_and(|minversion| *version < minversion.version) {
            return Err(Mode::Error::from_kind(
                sui_types::execution_status::ExecutionErrorKind::InvalidLinkage,
            ));
        }
    }
    Ok(())
}
