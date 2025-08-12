// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use sui_types::transaction::ProgrammableMoveCall as NativeMoveCall;

use crate::api::scalars::sui_address::SuiAddress;

use super::TransactionArgument;

/// A call to a Move function.
#[derive(Clone)]
pub struct MoveCallCommand {
    pub native: NativeMoveCall,
}

#[Object]
impl MoveCallCommand {
    // TODO(DVX-1373): Replace package, module and function_name by MoveFunction type.
    /// The storage ID of the package the function being called is defined in.
    async fn package(&self) -> Option<SuiAddress> {
        Some(SuiAddress::from(self.native.package))
    }

    /// The name of the module the function being called is defined in.
    async fn module(&self) -> Option<String> {
        Some(self.native.module.clone())
    }

    /// The name of the function being called.
    async fn function_name(&self) -> Option<String> {
        Some(self.native.function.clone())
    }

    /// The actual function parameters passed in for this move call.
    async fn arguments(&self) -> Vec<TransactionArgument> {
        self.native
            .arguments
            .iter()
            .map(|arg| TransactionArgument::from(*arg))
            .collect()
    }
}
