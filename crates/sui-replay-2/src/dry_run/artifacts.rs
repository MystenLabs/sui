// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Artifact serialization for local checkpoint dry-runs.

use super::LocalDryRunExecution;
use crate::{
    artifacts::{Artifact, ArtifactManager, MoveCallInfo, ReplayCacheSummary},
    tracing::save_trace_output,
};
use anyhow::Result;
use serde::Serialize;
use sui_types::{
    gas::SuiGasStatusAPI,
    transaction::{TransactionDataAPI, TransactionKind},
};

/// Save the replay artifacts that apply to a local checkpoint dry-run.
pub fn save_local_dry_run_artifacts(
    manager: &ArtifactManager<'_>,
    execution: &mut LocalDryRunExecution,
    network: String,
) -> Result<()> {
    if let Some(trace_builder) = execution.trace_builder.take() {
        save_trace_output(
            manager,
            trace_builder,
            &execution.object_cache,
            &execution.inner_store,
        )?;
    }

    save(manager, Artifact::TransactionData, &execution.transaction)?;
    save(
        manager,
        Artifact::TransactionGasReport,
        &execution.gas_status.gas_usage_report(),
    )?;
    save(
        manager,
        Artifact::ReplayCacheSummary,
        &ReplayCacheSummary::from_cache(
            execution.snapshot.epoch.epoch_id,
            execution.snapshot.checkpoint,
            network,
            execution.snapshot.epoch.protocol_version,
            &execution.object_cache,
        ),
    )?;
    if let TransactionKind::ProgrammableTransaction(ptb) = execution.transaction.kind() {
        save(
            manager,
            Artifact::MoveCallInfo,
            &MoveCallInfo::from_transaction(ptb, &execution.object_cache)?,
        )?;
    }
    save(manager, Artifact::TransactionEffects, &execution.effects)
}

fn save(manager: &ArtifactManager<'_>, artifact: Artifact, value: &impl Serialize) -> Result<()> {
    manager
        .member(artifact)
        .serialize_artifact(value)
        .transpose()?
        .unwrap();
    Ok(())
}
