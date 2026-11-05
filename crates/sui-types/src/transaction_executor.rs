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
use crate::transaction::AllowedProposers;
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
        gas_payment: SimulationGasPayment,
    ) -> Result<SimulateTransactionResult, SuiError>;
}

/// How simulation supplies a gas coin that is not part of the transaction.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum SimulationGasPayment {
    /// Execute only with the transaction's declared gas payment.
    NoMock,
    /// Add a synthetic gas coin when the transaction has no gas payment.
    MockIfEmpty,
    /// Add synthetic funding alongside a declared coin payment for estimation.
    ///
    /// The declared payment remains the primary coin, preserving its identity and
    /// storage-rebate provenance. The synthetic coin exists only during this simulation.
    SupplementRealPayment,
}

/// Trait to let the gRPC service name the validators a transaction should be submitted to,
/// without depending on the transaction driver that tracks them.
pub trait ProposerSelector: Send + Sync {
    /// Up to `max` validators this node would prefer to submit to, as committee indices for the
    /// current epoch. `None` when no preference can be formed, in which case the transaction is
    /// left unrestricted rather than pinned to an arbitrary set.
    ///
    /// The returned indices are strictly increasing, as `TransactionExpiration::Validity`
    /// requires.
    fn preferred_proposers(&self, max: usize) -> Option<AllowedProposers>;
}

#[derive(Debug)]
pub struct SimulateTransactionResult {
    pub effects: TransactionEffects,
    pub events: Option<TransactionEvents>,
    pub objects: ObjectSet,
    pub execution_result: Result<Vec<ExecutionResult>, ExecutionError>,
    pub mock_gas_id: Option<ObjectID>,
    pub unchanged_loaded_runtime_objects: Vec<ObjectKey>,
    pub suggested_gas_price: Option<u64>,
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
