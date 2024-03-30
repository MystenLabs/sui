// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::consensus_handler::{
    SequencedConsensusTransactionKind, VerifiedSequencedConsensusTransaction,
};
use mysten_metrics::monitored_scope;
use sui_protocol_config::{ConsensusTransactionOrdering, ZeroGasPriceOverride};
use sui_types::messages_consensus::{ConsensusTransaction, ConsensusTransactionKind};
use sui_types::transaction::TransactionDataAPI;

pub struct PostConsensusTxReorder {}

impl PostConsensusTxReorder {
    pub fn reorder(
        transactions: &mut [VerifiedSequencedConsensusTransaction],
        kind: ConsensusTransactionOrdering,
        zero_gas_price_override: ZeroGasPriceOverride,
    ) {
        // TODO: make the reordering algorithm richer and depend on object hotness as well.
        // Order transactions based on their gas prices. System transactions without gas price
        // are put to the beginning of the sequenced_transactions vector.
        match kind {
            ConsensusTransactionOrdering::ByGasPrice => {
                Self::order_by_gas_price(transactions, zero_gas_price_override)
            }
            ConsensusTransactionOrdering::None => (),
        }
    }

    fn order_by_gas_price(
        transactions: &mut [VerifiedSequencedConsensusTransaction],
        zero_gas_price_override: ZeroGasPriceOverride,
    ) {
        let _scope = monitored_scope("HandleConsensusOutput::order_by_gas_price");
        transactions.sort_by_key(|txn| {
            // Reverse order, so that transactions with higher gas price are put to the beginning.
            std::cmp::Reverse({
                match &txn.0.transaction {
                    SequencedConsensusTransactionKind::External(ConsensusTransaction {
                        tracking_id: _,
                        kind: ConsensusTransactionKind::UserTransaction(cert),
                    }) => cert.transaction_data().gas_price(zero_gas_price_override),
                    // Non-user transactions are considered to have gas price of MAX u64 and are
                    // put to the beginning.
                    _ => u64::MAX,
                }
            })
        })
    }
}
