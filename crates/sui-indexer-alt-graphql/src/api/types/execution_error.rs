// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Object;
use fastcrypto::encoding::{Base64, Encoding};
use std::fmt::Write;
use sui_package_resolver::{CleverError, ErrorConstants};
use sui_types::execution_status::{
    ExecutionFailureStatus, ExecutionStatus as NativeExecutionStatus,
};
use tokio::sync::OnceCell;

use crate::{
    api::{
        scalars::big_int::BigInt,
        types::{move_function::MoveFunction, move_module::MoveModule, move_package::MovePackage},
    },
    error::RpcError,
    scope::Scope,
};

#[derive(Clone)]
pub(crate) struct ExecutionError {
    native: ExecutionFailureStatus,
    command: Option<usize>,
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
        // Note: This uses full CleverError resolution rather than ErrorBitset bit manipulation for
        // abort code extraction. While ErrorBitset would be faster for isolated queries, we use
        // CleverError for consistency and to amortize the expensive resolution cost across multiple
        // fields (identifier, constant, etc.) that are commonly queried together in GraphQL.

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

    /// The module that the abort originated from. Only populated for Move aborts and primitive runtime errors.
    async fn module(&self) -> Option<MoveModule> {
        let location = match &self.native {
            ExecutionFailureStatus::MoveAbort(location, _) => Some(location),
            ExecutionFailureStatus::MovePrimitiveRuntimeError(location_opt) => {
                location_opt.0.as_ref()
            }
            _ => None,
        }?;

        // location.module is already the correct storage package ID thanks to resolve_module_id_for_move_abort
        let package =
            MovePackage::with_address(self.scope.clone(), (*location.module.address()).into());
        let module_name = location.module.name().to_string();

        Some(MoveModule::with_fq_name(package, module_name))
    }

    /// The function that the abort originated from. Only populated for Move aborts and primitive runtime errors that have function name information.
    async fn function(&self) -> Option<MoveFunction> {
        let location = match &self.native {
            ExecutionFailureStatus::MoveAbort(location, _) => Some(location),
            ExecutionFailureStatus::MovePrimitiveRuntimeError(location_opt) => {
                location_opt.0.as_ref()
            }
            _ => None,
        }?;

        let function_name = location.function_name.as_ref()?;

        // Create the module using the already-resolved module ID
        let package =
            MovePackage::with_address(self.scope.clone(), (*location.module.address()).into());
        let module_name = location.module.name().to_string();
        let module = MoveModule::with_fq_name(package, module_name);

        Some(MoveFunction::with_fq_name(module, function_name.clone()))
    }

    /// Human readable explanation of why the transaction failed.
    ///
    /// For Move aborts, the error message will be resolved to a human-readable form if possible, otherwise it will fall back to displaying the abort code and location.
    async fn message(&self) -> Result<String, RpcError> {
        self.format_error_message().await.map_err(|e| {
            anyhow::Error::from(e)
                .context("Failed to format error message")
                .into()
        })
    }
}

impl ExecutionError {
    /// Factory method to create ExecutionError from execution failure status.
    pub(crate) async fn from_execution_status(
        scope: &Scope,
        status: &NativeExecutionStatus,
    ) -> Result<Option<Self>, RpcError> {
        let NativeExecutionStatus::Failure { error, command } = status else {
            return Ok(None);
        };

        Ok(Some(Self {
            native: error.clone(),
            command: *command,
            clever: OnceCell::new(),
            scope: scope.clone(),
        }))
    }

    /// Helper method to get the clever error, using OnceCell for lazy initialization.
    /// Returns the resolved clever error if available, or None if resolution fails.
    ///
    /// Since protocol version 48, the Sui protocol layer automatically resolves module ID, which makes
    /// clever error resolution available. Before version 48, this will return None.
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

    /// Formats the error message with sophisticated logic.
    ///
    /// Handles command context, Move abort details, and clever error information.
    async fn format_error_message(&self) -> Result<String, std::fmt::Error> {
        match self.command {
            None => {
                // No command context, just return the error string
                Ok(self.native.to_string())
            }
            Some(command) => {
                // Add command context with ordinal suffix (1st, 2nd, 3rd, 4th, etc.)
                let command = command + 1;
                let suffix = match command % 10 {
                    1 if command % 100 != 11 => "st",
                    2 if command % 100 != 12 => "nd",
                    3 if command % 100 != 13 => "rd",
                    _ => "th",
                };

                let mut msg = String::new();
                write!(msg, "Error in {command}{suffix} command, ")?;

                // Handle Move aborts with detailed formatting. Otherwise, just append the error.
                let ExecutionFailureStatus::MoveAbort(loc, code) = &self.native else {
                    write!(msg, "{}", self.native)?;
                    return Ok(msg);
                };

                // Format Move abort with module and function info
                write!(msg, "from '{}", loc.module.to_canonical_display(true))?;
                if let Some(fname) = &loc.function_name {
                    write!(msg, "::{}'", fname)?;
                } else {
                    write!(msg, "'")?;
                }

                // Try to get clever error information
                let Some(CleverError {
                    source_line_number,
                    error_info,
                    error_code,
                    ..
                }) = self.clever_error().await.as_ref()
                else {
                    // No clever error, show basic abort info
                    write!(
                        msg,
                        " (instruction {}), abort code: {code}",
                        loc.instruction
                    )?;
                    return Ok(msg);
                };

                // Format with clever error details
                let error_code_str = match error_code {
                    Some(code) => format!("(code = {code})"),
                    _ => String::new(),
                };

                match error_info {
                    ErrorConstants::Rendered {
                        identifier,
                        constant,
                    } => {
                        write!(msg, " (line {source_line_number}), abort{error_code_str} '{identifier}': {constant}")?;
                    }
                    ErrorConstants::Raw { identifier, bytes } => {
                        let const_str = Base64::encode(bytes);
                        write!(msg, " (line {source_line_number}), abort{error_code_str} '{identifier}': {const_str}")?;
                    }
                    ErrorConstants::None => {
                        write!(
                            msg,
                            " (line {source_line_number}){}",
                            match error_code {
                                Some(code) => format!(" abort(code = {code})"),
                                _ => String::new(),
                            }
                        )?;
                    }
                }

                Ok(msg)
            }
        }
    }
}
