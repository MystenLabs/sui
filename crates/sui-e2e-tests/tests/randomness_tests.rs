// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(msim)]
mod test {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use sui_macros::{clear_fail_point, register_fail_point_if, sim_test};
    use sui_sdk::wallet_context::WalletContext;
    use sui_test_transaction_builder::{TestTransactionBuilder, randomness_state_call_arg};
    use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
    use sui_types::effects::TransactionEffectsAPI;
    use sui_types::execution_status::{ExecutionErrorKind, ExecutionStatus};
    use sui_types::transaction::Transaction;
    use test_cluster::TestClusterBuilder;

    async fn build_randomness_transaction(context: &WalletContext) -> Transaction {
        let (sender, gas_object) = context.get_one_gas_object().await.unwrap().unwrap();
        let gas_price = context.get_reference_gas_price().await.unwrap();
        let random_call_arg = randomness_state_call_arg(context).await;

        context
            .sign_transaction(
                &TestTransactionBuilder::new(sender, gas_object, gas_price)
                    .move_call(
                        SUI_FRAMEWORK_PACKAGE_ID,
                        "random",
                        "new_generator",
                        vec![random_call_arg],
                    )
                    .build(),
            )
            .await
    }

    #[sim_test]
    async fn test_randomness_recovers_after_dkg_timeout() {
        let suppress_dkg = Arc::new(AtomicBool::new(true));
        let suppress_dkg_for_failpoint = suppress_dkg.clone();
        register_fail_point_if("rb-dkg", move || {
            suppress_dkg_for_failpoint.load(Ordering::Relaxed)
        });
        let _protocol_config_guard =
            sui_protocol_config::ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
                config.set_random_beacon_dkg_timeout_round_for_testing(0);
                config
            });

        let test_cluster = TestClusterBuilder::new()
            .with_num_validators(4)
            .with_epoch_duration_ms(600_000)
            .build()
            .await;
        let transaction = build_randomness_transaction(&test_cluster.wallet).await;
        let response = test_cluster
            .wallet
            .execute_transaction_may_fail(transaction)
            .await
            .unwrap();
        let initial_epoch = response.effects.executed_epoch();
        assert!(matches!(
            response.effects.status(),
            ExecutionStatus::Failure(failure)
                if failure.error
                    == ExecutionErrorKind::ExecutionCancelledDueToRandomnessUnavailable
        ));

        suppress_dkg.store(false, Ordering::Relaxed);
        test_cluster.stop_all_validators().await;
        test_cluster.start_all_validators().await;

        tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                let transaction = build_randomness_transaction(&test_cluster.wallet).await;
                let response = test_cluster
                    .wallet
                    .execute_transaction_may_fail(transaction)
                    .await
                    .unwrap();
                assert_eq!(
                    initial_epoch,
                    response.effects.executed_epoch(),
                    "epoch changed during DKG recovery; recovery must complete within the \
                     initial epoch for this test to be meaningful"
                );
                match response.effects.status() {
                    ExecutionStatus::Success => break,
                    ExecutionStatus::Failure(failure)
                        if failure.error
                            == ExecutionErrorKind::ExecutionCancelledDueToRandomnessUnavailable =>
                    {
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                    status => panic!("unexpected randomness transaction status: {status:?}"),
                }
            }
        })
        .await
        .expect("DKG should recover before the readiness deadline");

        // Verify the epoch still advances cleanly after mid-epoch DKG recovery.
        test_cluster.trigger_reconfiguration().await;
        let post_reconfig_epoch = test_cluster
            .fullnode_handle
            .sui_node
            .with(|node| node.state().epoch_store_for_testing().epoch());
        assert!(post_reconfig_epoch > initial_epoch);

        clear_fail_point("rb-dkg");
    }

    /// Verifies that when DKG times out and never recovers (sends stay suppressed for the
    /// whole test), randomness-using transactions are canceled and epoch change still
    /// succeeds.
    #[sim_test]
    async fn test_epoch_change_succeeds_without_dkg_recovery() {
        register_fail_point_if("rb-dkg", || true);
        let _protocol_config_guard =
            sui_protocol_config::ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
                config.set_random_beacon_dkg_timeout_round_for_testing(0);
                config
            });

        let test_cluster = TestClusterBuilder::new()
            .with_num_validators(4)
            .with_epoch_duration_ms(600_000)
            .build()
            .await;

        let transaction = build_randomness_transaction(&test_cluster.wallet).await;
        let response = test_cluster
            .wallet
            .execute_transaction_may_fail(transaction)
            .await
            .unwrap();
        let initial_epoch = response.effects.executed_epoch();
        assert!(matches!(
            response.effects.status(),
            ExecutionStatus::Failure(failure)
                if failure.error
                    == ExecutionErrorKind::ExecutionCancelledDueToRandomnessUnavailable
        ));

        // DKG remains incomplete; epoch change must not be blocked by it.
        test_cluster.trigger_reconfiguration().await;
        let post_reconfig_epoch = test_cluster
            .fullnode_handle
            .sui_node
            .with(|node| node.state().epoch_store_for_testing().epoch());
        assert!(post_reconfig_epoch > initial_epoch);

        clear_fail_point("rb-dkg");
    }
}
