// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Write;

use anyhow::{self};
use async_graphql::{Context, Object};
use fastcrypto::encoding::Encoding;
use sui_indexer_alt_reader::package_resolver::PackageResolver;
use sui_package_resolver::{CleverError, ErrorConstants};
use sui_types::{
    execution_status::{ExecutionStatus as NativeExecutionStatus, MoveLocation},
    transaction::ProgrammableTransaction,
};

use crate::{api::scalars::big_int::BigInt, error::RpcError};

#[derive(Clone)]
pub(crate) struct ExecutionError {
    // Cached result of expensive resolve_clever_error() call
    clever_error: Option<CleverError>,
    // Raw Move abort data for when clever resolution fails
    raw_move_abort: Option<(MoveLocation, u64)>,
    // Raw error message for non-Move failures (without command prefix)
    raw_error_message: Option<String>,
    // Command prefix for errors that occurred within a specific command
    command_prefix: Option<String>,
}

/// Represents execution error information for failed transactions.
#[Object]
impl ExecutionError {
    /// Human-readable error message explaining why the transaction failed.
    /// For Move aborts, attempts to resolve to human-readable form using clever errors,
    /// otherwise falls back to displaying the abort code and location.
    async fn message(&self) -> Option<String> {
        let mut message = String::new();

        if let Some(command_prefix) = &self.command_prefix {
            message.push_str(command_prefix);
        }

        if let Some(clever_error) = &self.clever_error {
            // We have clever error information - format rich message
            message.push_str(
                &Self::format_clever_error_message(clever_error).unwrap_or_else(|e| e.to_string()),
            );
        } else if let Some((location, abort_code)) = &self.raw_move_abort {
            // Fallback to basic Move abort error message
            message.push_str(
                &Self::format_basic_error_message(location, *abort_code)
                    .unwrap_or_else(|e| e.to_string()),
            );
        } else if let Some(raw_message) = &self.raw_error_message {
            // Non-Move failure - add raw error message
            message.push_str(raw_message);
        } else {
            // No error content - shouldn't happen but return None
            return None;
        }

        Some(message)
    }

    /// The error code of the Move abort, populated if this transaction failed with a Move abort.
    async fn move_abort_code(&self) -> Option<BigInt> {
        if let Some(clever_error) = &self.clever_error {
            // Use clever error code if available, otherwise fall back to raw abort code
            if let Some(error_code) = clever_error.error_code {
                Some(BigInt::from(error_code as u64))
            } else {
                // Extract from raw abort info as fallback
                self.raw_move_abort
                    .as_ref()
                    .map(|(_, code)| BigInt::from(*code))
            }
        } else {
            // Use raw abort code
            self.raw_move_abort
                .as_ref()
                .map(|(_, code)| BigInt::from(*code))
        }
    }
}

impl ExecutionError {
    /// Factory method to create ExecutionError from any execution failure.
    /// Handles both Move aborts (with clever error resolution) and other execution failures.
    pub(crate) async fn from_execution_status(
        ctx: &Context<'_>,
        status: &NativeExecutionStatus,
        programmable_tx: Option<&ProgrammableTransaction>,
    ) -> Result<Option<Self>, RpcError> {
        use sui_types::execution_status::ExecutionFailureStatus;

        // First resolve the execution status (like resolve_native_status_impl in old GraphQL)
        let resolved_status =
            Self::resolve_execution_status_impl(ctx, status.clone(), programmable_tx).await?;

        match resolved_status {
            NativeExecutionStatus::Success => Ok(None),

            NativeExecutionStatus::Failure { error, command } => {
                // Determine command prefix if command exists
                let command_prefix = command.map(|cmd| {
                    let command = cmd + 1;
                    let suffix = match command % 10 {
                        1 if command % 100 != 11 => "st",
                        2 if command % 100 != 12 => "nd",
                        3 if command % 100 != 13 => "rd",
                        _ => "th",
                    };
                    format!("Error in {command}{suffix} command, ")
                });

                match error {
                    ExecutionFailureStatus::MoveAbort(location, abort_code) => {
                        // Move abort - try clever error resolution with already-resolved module ID
                        let resolver: &PackageResolver = ctx.data_unchecked();
                        let clever_error = resolver
                            .resolve_clever_error(location.module.clone(), abort_code)
                            .await;

                        Ok(Some(Self {
                            clever_error,
                            raw_move_abort: Some((location.clone(), abort_code)),
                            raw_error_message: None,
                            command_prefix,
                        }))
                    }

                    _ => {
                        // Non-Move failure - store raw error message without command prefix
                        Ok(Some(Self {
                            clever_error: None,
                            raw_move_abort: None,
                            raw_error_message: Some(error.to_string()),
                            command_prefix,
                        }))
                    }
                }
            }
        }
    }

    /// Resolves the module ID within a Move abort to the storage ID of the package that the
    /// abort occured in.
    /// * If the error is not a Move abort, or the Move call in the programmable transaction cannot
    ///   be found, this function will do nothing.
    /// * If the error is a Move abort and the storage ID is unable to be resolved an error is
    ///   returned.
    async fn resolve_execution_status_impl(
        ctx: &Context<'_>,
        mut status: NativeExecutionStatus,
        programmable_tx: Option<&ProgrammableTransaction>,
    ) -> Result<NativeExecutionStatus, RpcError> {
        use sui_types::{
            execution_status::{ExecutionFailureStatus, MoveLocationOpt},
            transaction::Command,
        };

        let resolver: &PackageResolver = ctx.data_unchecked();

        // Match the exact pattern from the original resolve_native_status_impl
        if let NativeExecutionStatus::Failure {
            error:
                ExecutionFailureStatus::MoveAbort(MoveLocation { module, .. }, _)
                | ExecutionFailureStatus::MovePrimitiveRuntimeError(MoveLocationOpt(Some(MoveLocation {
                    module,
                    ..
                }))),
            command: Some(command_idx),
        } = &mut status
        {
            // Get the Move call that this error is associated with.
            if let Some(ptb) = programmable_tx {
                if let Some(Command::MoveCall(ptb_call)) = ptb.commands.get(*command_idx) {
                    let module_new = module.clone();
                    // Resolve the runtime module ID in the Move abort to the storage ID of the package
                    // that the abort occurred in. This is important to make sure that we look at the
                    // correct version of the module when resolving the error.
                    *module = resolver
                        .resolve_module_id(module_new, ptb_call.package.into())
                        .await
                        .map_err(|e| anyhow::anyhow!("Error resolving Move location: {e}"))?;
                }
            }
        }

        Ok(status)
    }

    /// Format a clever error message with rich error information
    fn format_clever_error_message(clever_error: &CleverError) -> Result<String, std::fmt::Error> {
        let mut msg = String::new();

        // Start with module info
        write!(
            msg,
            "Move abort from '{}'",
            clever_error.module_id.to_canonical_display(true)
        )?;

        match &clever_error.error_info {
            ErrorConstants::Rendered {
                identifier,
                constant,
            } => {
                let error_code_str = clever_error
                    .error_code
                    .map(|code| format!("(code = {code})"))
                    .unwrap_or_default();

                write!(
                    msg,
                    " (line {}), abort{error_code_str} '{identifier}': {constant}",
                    clever_error.source_line_number
                )?;
            }
            ErrorConstants::Raw { identifier, bytes } => {
                let const_str = fastcrypto::encoding::Base64::encode(bytes);
                let error_code_str = clever_error
                    .error_code
                    .map(|code| format!("(code = {code})"))
                    .unwrap_or_default();

                write!(
                    msg,
                    " (line {}), abort{error_code_str} '{identifier}': {const_str}",
                    clever_error.source_line_number
                )?;
            }
            ErrorConstants::None => {
                let error_suffix = clever_error
                    .error_code
                    .map(|code| format!(" abort(code = {code})"))
                    .unwrap_or_default();

                write!(
                    msg,
                    " (line {}){}",
                    clever_error.source_line_number, error_suffix
                )?;
            }
        }

        Ok(msg)
    }

    /// Format a basic error message without clever error information
    fn format_basic_error_message(
        location: &MoveLocation,
        abort_code: u64,
    ) -> Result<String, std::fmt::Error> {
        let mut msg = String::new();

        write!(
            msg,
            "Move abort from '{}'",
            location.module.to_canonical_display(true)
        )?;

        if let Some(fname) = &location.function_name {
            write!(msg, "::{fname}")?;
        }

        write!(
            msg,
            " (instruction {}), abort code: {abort_code}",
            location.instruction
        )?;

        Ok(msg)
    }
}
