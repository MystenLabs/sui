// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_rpc::proto::sui::rpc::v2::{
    ExecuteTransactionRequest, ExecuteTransactionResponse, SimulateTransactionRequest,
    SimulateTransactionResponse, transaction_execution_service_server::TransactionExecutionService,
};

use crate::context::Context;

/// Minimal transaction execution service placeholder for the runnable forking skeleton.
pub struct ForkingTransactionExecutionService {
    context: Context,
}

impl ForkingTransactionExecutionService {
    pub fn new(context: Context) -> Self {
        Self { context }
    }
}

#[tonic::async_trait]
impl TransactionExecutionService for ForkingTransactionExecutionService {
    async fn execute_transaction(
        &self,
        _request: tonic::Request<ExecuteTransactionRequest>,
    ) -> Result<tonic::Response<ExecuteTransactionResponse>, tonic::Status> {
        let _ = &self.context;
        todo!("execute_transaction is not implemented in the runnable skeleton")
    }

    async fn simulate_transaction(
        &self,
        _request: tonic::Request<SimulateTransactionRequest>,
    ) -> Result<tonic::Response<SimulateTransactionResponse>, tonic::Status> {
        let _ = &self.context;
        todo!("simulate_transaction is not implemented in the runnable skeleton")
    }
}
