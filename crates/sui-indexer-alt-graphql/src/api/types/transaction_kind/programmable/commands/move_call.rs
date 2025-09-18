// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use sui_types::transaction::ProgrammableMoveCall as NativeMoveCall;

use crate::{
    api::types::{move_function::MoveFunction, move_module::MoveModule, move_package::MovePackage},
    scope::Scope,
};

use super::TransactionArgument;

/// A call to a Move function.
#[derive(Clone)]
pub struct MoveCallCommand {
    pub native: NativeMoveCall,
    pub scope: Scope,
}

#[Object]
impl MoveCallCommand {
    /// The function being called.
    async fn function(&self) -> MoveFunction {
        let package = MovePackage::with_address(self.scope.clone(), self.native.package.into());
        let module = MoveModule::with_fq_name(package, self.native.module.clone());
        MoveFunction::with_fq_name(module, self.native.function.clone())
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
