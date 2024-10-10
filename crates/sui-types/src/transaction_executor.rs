// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::base_types::ObjectID;
use crate::effects::TransactionEffects;
use crate::effects::TransactionEvents;
use crate::error::SuiError;
use crate::object::Object;
use crate::quorum_driver_types::ExecuteTransactionRequestV3;
use crate::quorum_driver_types::ExecuteTransactionResponseV3;
use crate::quorum_driver_types::QuorumDriverError;
use crate::transaction::TransactionData;

/// Trait to define the interface for how the REST service interacts with a a QuorumDriver or a
/// simulated transaction executor.
#[async_trait::async_trait]
pub trait TransactionExecutor: Send + Sync {
    async fn execute_transaction(
        &self,
        request: ExecuteTransactionRequestV3,
        client_addr: Option<std::net::SocketAddr>,
    ) -> Result<ExecuteTransactionResponseV3, QuorumDriverError>;

    fn simulate_transaction(
        &self,
        transaction: TransactionData,
    ) -> Result<SimulateTransactionResult, SuiError>;
}

pub struct SimulateTransactionResult {
    pub effects: TransactionEffects,
    pub events: Option<TransactionEvents>,
    pub input_objects: BTreeMap<ObjectID, Object>,
    pub output_objects: BTreeMap<ObjectID, Object>,
    pub mock_gas_id: Option<ObjectID>,
}
