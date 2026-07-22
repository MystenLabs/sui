// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{TestCaseImpl, TestContext};
use async_trait::async_trait;
use sui_types::effects::TransactionEffectsAPI;
use tracing::info;

pub struct FullNodeExecuteTransactionTest;

#[async_trait]
impl TestCaseImpl for FullNodeExecuteTransactionTest {
    fn name(&self) -> &'static str {
        "FullNodeExecuteTransaction"
    }

    fn description(&self) -> &'static str {
        "Test executing transactions via the gRPC TransactionExecutionService"
    }

    async fn run(&self, ctx: &mut TestContext) -> Result<(), anyhow::Error> {
        let txn_count = 2;
        let mut txns = ctx.make_transactions(txn_count).await;
        assert!(
            txns.len() >= txn_count,
            "Expect at least {} txns, but only got {}. Do we generate enough gas objects during genesis?",
            txn_count,
            txns.len(),
        );

        // This test intentionally drives execution directly rather than through
        // the shared `sign_and_execute` helper, because its whole purpose is to
        // exercise both gRPC execution paths.

        // Path 1: immediate execution (`TransactionExecutionService`). The
        // response carries the effects immediately, before checkpointing. We
        // then wait for ledger/checkpoint visibility and retrieve the tx.
        info!("Test immediate execute_transaction");
        let txn = txns.swap_remove(0);
        let txn_digest = *txn.digest();

        let mut client = ctx.get_grpc_client();
        let response = client.execute_transaction(&txn).await?;
        assert!(
            response.effects.status().is_ok(),
            "Failed to execute transaction {:?}: {:?}",
            txn_digest,
            response.effects.status(),
        );

        // Wait for checkpoint visibility, then retrieve the transaction from the
        // ledger (`LedgerService`).
        ctx.wait_for_txns(&[txn_digest]).await;
        let fetched = client.get_transaction(&txn_digest).await?;
        assert!(
            fetched.effects.status().is_ok(),
            "Fetched transaction {:?} has non-success effects",
            txn_digest,
        );

        // Path 2: execute-and-wait-for-checkpoint helper. The returned result
        // must carry a checkpoint.
        info!("Test execute_transaction_and_wait_for_checkpoint");
        let txn = txns.swap_remove(0);
        let txn_digest = *txn.digest();

        let response = client
            .execute_transaction_and_wait_for_checkpoint(&txn)
            .await?;
        assert!(
            response.effects.status().is_ok(),
            "Failed to execute transaction {:?}: {:?}",
            txn_digest,
            response.effects.status(),
        );
        assert!(
            response.checkpoint.is_some(),
            "execute_transaction_and_wait_for_checkpoint should return a checkpoint for {:?}",
            txn_digest,
        );

        Ok(())
    }
}
