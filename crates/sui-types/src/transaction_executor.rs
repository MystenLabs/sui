// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::ObjectID;
use crate::effects::TransactionEffects;
use crate::effects::TransactionEvents;
use crate::error::ExecutionError;
use crate::error::SuiError;
use crate::execution::ExecutionResult;
use crate::full_checkpoint_content::ObjectSet;
use crate::storage::ObjectKey;
use crate::transaction::TransactionData;
use crate::transaction_driver_types::ExecuteTransactionRequestV3;
use crate::transaction_driver_types::ExecuteTransactionResponseV3;
use crate::transaction_driver_types::TransactionSubmissionError;

/// Trait to define the interface for how the gRPC service interacts with a  QuorumDriver or a
/// simulated transaction executor.
#[async_trait::async_trait]
pub trait TransactionExecutor: Send + Sync {
    async fn execute_transaction(
        &self,
        request: ExecuteTransactionRequestV3,
        client_addr: Option<std::net::SocketAddr>,
    ) -> Result<ExecuteTransactionResponseV3, TransactionSubmissionError>;

    fn simulate_transaction(
        &self,
        transaction: TransactionData,
        checks: TransactionChecks,
    ) -> Result<SimulateTransactionResult, SuiError>;
}

pub struct SimulateTransactionResult {
    pub effects: TransactionEffects,
    pub events: Option<TransactionEvents>,
    pub objects: ObjectSet,
    pub execution_result: Result<Vec<ExecutionResult>, ExecutionError>,
    pub mock_gas_id: Option<ObjectID>,
    pub unchanged_loaded_runtime_objects: Vec<ObjectKey>,
}

#[derive(Default, Debug, Copy, Clone)]
pub enum TransactionChecks {
    #[default]
    Enabled,
    Disabled,
}

impl TransactionChecks {
    pub fn disabled(self) -> bool {
        matches!(self, Self::Disabled)
    }

    pub fn enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}
