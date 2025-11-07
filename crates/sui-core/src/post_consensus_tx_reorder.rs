// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_metrics::monitored_scope;
use sui_protocol_config::ConsensusTransactionOrdering;
use sui_types::{
    executable_transaction::VerifiedExecutableTransaction, transaction::TransactionDataAPI as _,
};

pub struct PostConsensusTxReorder {}

impl PostConsensusTxReorder {
    pub fn reorder(
        transactions: &mut [VerifiedExecutableTransaction],
        kind: ConsensusTransactionOrdering,
    ) {
        match kind {
            ConsensusTransactionOrdering::ByGasPrice => Self::order_by_gas_price(transactions),
            ConsensusTransactionOrdering::None => (),
        }
    }

    fn order_by_gas_price(transactions: &mut [VerifiedExecutableTransaction]) {
        let _scope = monitored_scope("ConsensusCommitHandler::order_by_gas_price");
        transactions.sort_by_key(|tx| {
            // Reverse order, so that transactions with higher gas price are put to the beginning.
            std::cmp::Reverse(tx.transaction_data().gas_price())
        });
    }
}
