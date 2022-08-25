// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{TestCaseImpl, TestContext};
use async_trait::async_trait;
use sui_json_rpc_types::{SuiExecuteTransactionResponse, SuiExecutionStatus};
use sui_types::messages::ExecuteTransactionRequestType;
use tracing::info;

pub struct FullNodeExecuteTransactionTest;

#[async_trait]
impl TestCaseImpl for FullNodeExecuteTransactionTest {
    fn name(&self) -> &'static str {
        "FullNodeExecuteTransaction"
    }

    fn description(&self) -> &'static str {
        "Test executing transaction via Fullnode Quorum Driver"
    }

    async fn run(&self, ctx: &mut TestContext) -> Result<(), anyhow::Error> {
        ctx.get_sui_from_faucet(Some(3)).await;
        let mut txns = ctx.make_transactions(3).await;
        assert!(
            txns.len() >= 3,
            "Expect at least 3 txns, but only got {}. Do we get enough gas objects from faucet?",
            txns.len(),
        );

        let fullnode = ctx.get_fullnode();

        // Test WaitForEffectsCert
        let txn = txns.swap_remove(0);
        let txn_digest = *txn.digest();

        info!("Test execution with ImmediateReturn");
        let response = fullnode
            .quorum_driver()
            .execute_transaction_by_fullnode(
                txn.clone(),
                ExecuteTransactionRequestType::ImmediateReturn,
            )
            .await?;
        if let SuiExecuteTransactionResponse::ImmediateReturn { tx_digest } = response {
            assert_eq!(txn_digest, tx_digest);

            // Verify fullnode observes the txn
            ctx.let_fullnode_sync().await;

            fullnode
                .read_api()
                .get_transaction(tx_digest)
                .await
                .unwrap_or_else(|e| {
                    panic!(
                        "Failed get transaction {:?} from fullnode: {:?}",
                        txn_digest, e
                    )
                });
        } else {
            panic!("Expect ImmediateReturn but got {:?}", response);
        }

        info!("Test execution with WaitForTxCert");
        let txn = txns.swap_remove(0);
        let txn_digest = *txn.digest();
        let response = fullnode
            .quorum_driver()
            .execute_transaction_by_fullnode(
                txn.clone(),
                ExecuteTransactionRequestType::WaitForTxCert,
            )
            .await?;
        if let SuiExecuteTransactionResponse::TxCert { certificate } = response {
            assert_eq!(txn_digest, certificate.transaction_digest);

            // Verify fullnode observes the txn
            ctx.let_fullnode_sync().await;

            fullnode
                .read_api()
                .get_transaction(txn_digest)
                .await
                .unwrap_or_else(|e| {
                    panic!(
                        "Failed get transaction {:?} from fullnode: {:?}",
                        txn_digest, e
                    )
                });
        } else {
            panic!("Expect TxCert but got {:?}", response);
        }

        info!("Test execution with WaitForEffectsCert");
        let txn = txns.swap_remove(0);
        let txn_digest = *txn.digest();

        let response = fullnode
            .quorum_driver()
            .execute_transaction_by_fullnode(txn, ExecuteTransactionRequestType::WaitForEffectsCert)
            .await?;
        if let SuiExecuteTransactionResponse::EffectsCert {
            certificate,
            effects,
        } = response
        {
            assert_eq!(txn_digest, certificate.transaction_digest);
            if !matches!(effects.effects.status, SuiExecutionStatus::Success { .. }) {
                panic!(
                    "Failed to execute transfer tranasction {:?}: {:?}",
                    txn_digest, effects.effects.status
                )
            }
            // Verify fullnode observes the txn
            ctx.let_fullnode_sync().await;

            fullnode
                .read_api()
                .get_transaction(txn_digest)
                .await
                .unwrap_or_else(|e| {
                    panic!(
                        "Failed get transaction {:?} from fullnode: {:?}",
                        txn_digest, e
                    )
                });
        } else {
            panic!("Expect EffectsCert but got {:?}", response);
        }

        Ok(())
    }
}
