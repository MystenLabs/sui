// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{self, Context as _};
use async_graphql::Object;
use fastcrypto::encoding::{Base64, Encoding};
use sui_package_resolver::{CleverError, ErrorConstants};
use sui_types::{
    execution_status::{
        ExecutionFailureStatus, ExecutionStatus as NativeExecutionStatus, MoveLocation,
    },
    transaction::ProgrammableTransaction,
};
use tokio::sync::OnceCell;

use crate::{api::scalars::big_int::BigInt, error::RpcError, scope::Scope};

#[derive(Clone)]
pub(crate) struct ExecutionError {
    native: ExecutionFailureStatus,
    clever: OnceCell<Option<CleverError>>,
    scope: Scope,
}

/// Represents execution error information for failed transactions.
#[Object]
impl ExecutionError {
    /// The error code of the Move abort, populated if this transaction failed with a Move abort.
    ///
    /// Returns the explicit code if the abort used `code` annotation (e.g., `abort(ERR, code = 5)` returns 5), otherwise returns the raw abort code containing encoded error information.
    async fn abort_code(&self) -> Option<BigInt> {
        let ExecutionFailureStatus::MoveAbort(_, raw_code) = &self.native else {
            return None;
        };

        // Use clever error code if available, otherwise fall back to raw code
        Some(BigInt::from(
            self.clever_error()
                .await
                .as_ref()
                .and_then(|err| err.error_code)
                .map_or(*raw_code, |code| code as u64),
        ))
    }

    /// The source line number for the abort. Only populated for clever errors.
    async fn source_line_number(&self) -> Option<u64> {
        Some(self.clever_error().await.as_ref()?.source_line_number as u64)
    }

    /// The instruction offset in the Move bytecode where the error occurred. Populated for Move aborts and primitive runtime errors.
    async fn instruction_offset(&self) -> Result<Option<u16>, RpcError> {
        match &self.native {
            ExecutionFailureStatus::MoveAbort(location, _) => Ok(Some(location.instruction)),
            ExecutionFailureStatus::MovePrimitiveRuntimeError(location_opt) => {
                Ok(location_opt.0.as_ref().map(|loc| loc.instruction))
            }
            _ => Ok(None),
        }
    }

    /// The error's name. Only populated for clever errors.
    async fn identifier(&self) -> Option<String> {
        match &self.clever_error().await.as_ref()?.error_info {
            ErrorConstants::None => None,
            ErrorConstants::Rendered { identifier, .. } => Some(identifier.clone()),
            ErrorConstants::Raw { identifier, .. } => Some(identifier.clone()),
        }
    }

    /// An associated constant for the error. Only populated for clever errors.
    ///
    /// Constants are returned as human-readable strings when possible. Complex types are returned as Base64-encoded bytes.
    async fn constant(&self) -> Option<String> {
        match &self.clever_error().await.as_ref()?.error_info {
            ErrorConstants::None => None,
            ErrorConstants::Rendered { constant, .. } => Some(constant.clone()),
            ErrorConstants::Raw { bytes, .. } => Some(Base64::encode(bytes)),
        }
    }
}

impl ExecutionError {
    /// Factory method to create ExecutionError from execution failure status.
    /// Resolves module ID in-place for Move aborts.
    pub(crate) async fn from_execution_status(
        scope: &Scope,
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
        resolve_module_id_for_move_abort(scope, &mut native_error, *command, programmable_tx)
            .await?;

        Ok(Some(Self {
            native: native_error,
            clever: OnceCell::new(),
            scope: scope.clone(),
        }))
    }

    /// Helper method to get the clever error, using OnceCell for lazy initialization.
    /// Returns the resolved clever error if available, or None if resolution fails.
    async fn clever_error(&self) -> &Option<CleverError> {
        let ExecutionFailureStatus::MoveAbort(location, raw_code) = &self.native else {
            // Not a Move abort, no clever error possible
            static NONE: Option<CleverError> = None;
            return &NONE;
        };

        self.clever
            .get_or_init(|| async {
                self.scope
                    .package_resolver()
                    .resolve_clever_error(location.module.clone(), *raw_code)
                    .await
            })
            .await
    }
}

/// Resolves the runtime module ID in Move aborts to the storage package ID.
///
/// This is necessary because when a Move abort occurs, the error contains the runtime
/// module ID, but to resolve clever errors we need the storage package ID where the
/// module actually lives. This is especially important for upgraded packages where
/// the runtime module ID might differ from the storage package ID.
async fn resolve_module_id_for_move_abort(
    scope: &Scope,
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

    let module_new = module.clone();

    // Resolve runtime module ID to storage package ID
    *module = scope
        .package_resolver()
        .resolve_module_id(module_new, ptb_call.package.into())
        .await
        .context("Error resolving Move location")?;

    Ok(())
}
