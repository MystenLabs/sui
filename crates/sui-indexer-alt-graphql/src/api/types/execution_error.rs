// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{self, Context as _};
use async_graphql::{Context, Object};
use sui_indexer_alt_reader::package_resolver::PackageResolver;
use sui_package_resolver::CleverError;
use sui_types::{
    execution_status::{
        ExecutionFailureStatus, ExecutionStatus as NativeExecutionStatus, MoveLocation,
    },
    transaction::ProgrammableTransaction,
};
use tokio::sync::OnceCell;

use crate::{api::scalars::big_int::BigInt, error::RpcError};

/// Resolves the runtime module ID in Move aborts to the storage package ID.
///
/// This is necessary because when a Move abort occurs, the error contains the runtime
/// module ID, but to resolve clever errors we need the storage package ID where the
/// module actually lives. This is especially important for upgraded packages where
/// the runtime module ID might differ from the storage package ID.
async fn resolve_module_id_for_move_abort(
    ctx: &Context<'_>,
    native_error: &mut ExecutionFailureStatus,
    command: Option<usize>,
    programmable_tx: Option<&ProgrammableTransaction>,
) -> Result<(), RpcError> {
    use sui_types::execution_status::MoveLocationOpt;
    use sui_types::transaction::Command;

    // Only resolve for Move aborts that have location information
    let module = match native_error {
        ExecutionFailureStatus::MoveAbort(MoveLocation { module, .. }, _) => module,
        ExecutionFailureStatus::MovePrimitiveRuntimeError(MoveLocationOpt(Some(
            MoveLocation { module, .. },
        ))) => module,
        _ => return Ok(()),
    };

    // We need both a command index and a programmable transaction to resolve
    let Some(command_idx) = command else {
        return Ok(());
    };
    let Some(ptb) = programmable_tx else {
        return Ok(());
    };

    // Find the Move call command that caused this abort
    let Some(Command::MoveCall(ptb_call)) = ptb.commands.get(command_idx) else {
        return Ok(());
    };

    let resolver: &PackageResolver = ctx.data()?;
    let module_new = module.clone();

    // Resolve runtime module ID to storage package ID
    *module = resolver
        .resolve_module_id(module_new, ptb_call.package.into())
        .await
        .context("Error resolving Move location")?;

    Ok(())
}

#[derive(Clone)]
pub(crate) struct ExecutionError {
    native: ExecutionFailureStatus,
    clever: OnceCell<Option<CleverError>>,
}

/// Represents execution error information for failed transactions.
#[Object]
impl ExecutionError {
    /// The error code of the Move abort, populated if this transaction failed with a Move abort.
    ///
    /// Returns the explicit code if the abort used `code` annotation (e.g., `abort(ERR, code = 5)` returns 5), otherwise returns the raw abort code containing encoded error information.
    async fn abort_code(&self, ctx: &Context<'_>) -> Result<Option<BigInt>, RpcError> {
        let ExecutionFailureStatus::MoveAbort(_, raw_code) = &self.native else {
            return Ok(None);
        };

        let clever_error = self.clever_error(ctx).await?;

        // Use clever error code if available, otherwise fall back to raw code
        Ok(Some(BigInt::from(
            clever_error
                .as_ref()
                .and_then(|err| err.error_code)
                .map_or(*raw_code, |code| code as u64),
        )))
    }
}

impl ExecutionError {
    /// Factory method to create ExecutionError from execution failure status.
    /// Resolves module ID in-place for Move aborts.
    pub(crate) async fn from_execution_status(
        ctx: &Context<'_>,
        status: &NativeExecutionStatus,
        programmable_tx: Option<&ProgrammableTransaction>,
    ) -> Result<Option<Self>, RpcError> {
        let NativeExecutionStatus::Failure { error, command } = status else {
            return Ok(None);
        };

        // Clone the error so we can modify it in-place
        let mut native_error: ExecutionFailureStatus = error.clone();

        // Resolve the module ID for Move aborts to ensure we use the correct package version
        // when resolving clever errors later. This is critical for package upgrades.
        resolve_module_id_for_move_abort(ctx, &mut native_error, *command, programmable_tx).await?;

        Ok(Some(Self {
            native: native_error,
            clever: OnceCell::new(),
        }))
    }

    /// Helper method to get the clever error, using OnceCell for lazy initialization.
    /// Returns the resolved clever error if available, or None if resolution fails.
    async fn clever_error(&self, ctx: &Context<'_>) -> Result<&Option<CleverError>, RpcError> {
        let ExecutionFailureStatus::MoveAbort(location, raw_code) = &self.native else {
            // Not a Move abort, no clever error possible
            static NONE: Option<CleverError> = None;
            return Ok(&NONE);
        };

        self.clever
            .get_or_try_init(|| async {
                let resolver: &PackageResolver = ctx.data()?;
                let result = resolver
                    .resolve_clever_error(location.module.clone(), *raw_code)
                    .await;
                Ok(result)
            })
            .await
    }
}
